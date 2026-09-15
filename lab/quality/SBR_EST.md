# HE v1 SBR parameter estimation (TASK-87)

Recorded 2026-09-15. Host: Linux x86_64, rustc 1.97.1, on top of
`3f9b65a` (TASK-86 64-band bank). Crate-internal
`engine/enc_sbr_est.rs` (`SbrEstimator`, `SbrFrameParams`,
`he_header`). `encode()` stays LC; no bits are written (TASK-88); no
LC/SBR muxing or delay accounting (TASK-89). Original work on the
in-tree ISO tables (`sbr_freq_bands`, `sbr_huffman`, `sbr_hf_gen`
patches, `sbr_time_grid`); no peer encoder source.

## What it decides, per channel-frame (32 analysis slots × 64 bands)

| Parameter | Rule (v1, deterministic, bounded) |
|---|---|
| Header | fixed per rate: `bs_amp_res=1` (3.0 dB), `freq_scale=2`, `alter_scale=1`, `noise_bands=2`, limiter 2/2, interpol 1, smoothing 1, no extras; `(start,stop)` = 48k/44.1k `(10,8)`, 32k/24k/22.05k `(12,9)`, 16k `(12,11)` |
| Grid | `FIXFIX`; envelopes 1 / 2 / 4 when the peak HF slot energy exceeds 3× / 8× the mean of the previous 8 slots (state crosses frames; −90 dBFS floor). The LC attack detector runs at the core rate and cannot see the top octave, so this is not a second copy of it. `freq_res` high for 1 envelope, low otherwise |
| Envelope `E_Q` | `round(a·(log2(mean|X|²) + 24))`, `a` = 2 (1.5 dB, forced by single-envelope FIXFIX) or 1; +24 = decoder core at 16-bit scale (+30) minus the `64·2^(E/a)` offset (−6); clamped to the start-value field and to ±LAV of the band below so the frequency direction is always codable |
| Noise floor `Q` | per noise band: `nf` = one-tap complex prediction error / energy across slots (a stationary tone predicts exactly, noise does not) for the original HF band and for the LF subbands the decoder will patch in (in-tree `build_patches`); `gap = max(nf_hf − nf_lf, 0)`; `Q = round(6 − log2(gap / (1 − nf_hf)))` in `0..=30` (`Q=30` = no added noise) |
| `bs_invf_mode` | `gap > 0.15 / 0.35 / 0.6` → 1 / 2 / 3, else 0 (noise floor 0 only) |
| `bs_add_harmonic` | off (flag 0) |
| `bs_df_env` / `bs_df_noise` | frequency direction (start value + `f_huffman`) vs time direction (`t_huffman` against the previous envelope in frame, else the previous frame's last, with the decoder's `ref_band` resolution mapping); the cheaper valid one; first frame after `reset` has no reference |
| `est_bits` | `bs_data_extra + grid + dtdf + invf + envelope + noise + add_harmonic flag + extended_data flag` for `sbr_single_channel_element()` |

Grid alignment (for TASK-89): envelope time slot `t` of decoder frame
`n` reads `XHigh` column `2t + tHFAdj(2)` while the frame's own core
analysis starts at column `tHFGen = 8`, so the estimator for frame `n`
takes full-rate analysis slots `[32n − 6, 32n + 26)` (`GRID_LEAD = 6`
in the tests). The LC priming delay is on top of that.

## Measurements (`cargo test --lib enc_sbr_est -- --nocapture`)

Pinned band tables (`header_and_band_tables_are_pinned_per_rate`, a
determinism tripwire for the libm-derived master table):

| fs_sbr | k0 (Hz) | k2 (Hz) | NHigh | NLow | NQ |
|---:|---:|---:|---:|---:|---:|
| 16000 | 28 (3500) | 60 (7500) | 10 | 5 | 2 |
| 22050 | 24 (4134) | 52 (8958) | 12 | 6 | 2 |
| 24000 | 25 (4688) | 52 (9750) | 10 | 5 | 2 |
| 32000 | 25 (6250) | 52 (13000) | 10 | 5 | 2 |
| 44100 | 19 (6546) | 43 (14815) | 12 | 6 | 2 |
| 48000 | 18 (6750) | 41 (15375) | 12 | 6 | 2 |

Reference reconstruction — the estimated parameters are decoded by the
in-tree `SbrDecoder` over the ideal halfband core (no LC coding, core
×32768), 12 frames at 48 kHz mono, both signals re-analysed with the
64-band bank, compared per `fTableHigh` band over the SBR range
(`reference_reconstruction_matches_hf_band_energies`). Bands ≥ 20 dB
under a neighbour (or under the core's top subband) are spectral
edges: the decoder's subband noise is one subband wide and the
overlapping QMF re-analysis sees it, so they are reported, not gated.

| cell | interior mean \|err\| | interior worst | total HF | frame-4 Q / invf |
|---|---:|---:|---:|---|
| 150 Hz comb to 6.6 kHz + noise 8–15 kHz | 0.26 dB | 0.50 dB | −0.19 dB | [30, 30] / [0, 0] |
| white noise | 0.16 dB | 0.54 dB | −0.15 dB | [9, 30] / [0, 0] |
| noise 0.2–6.7 kHz + 12.1 kHz tone | 0.14 dB | 0.91 dB | −0.12 dB | [30, 30] / [0, 0] |

Edge bands in those cells sit +13 to +45 dB above targets that are
33–94 dB under the peak: subband-resolution spread, inaudible against
the neighbour, and the tone cell's 12.1 kHz line comes back as a
two-subband noise band (no `bs_add_harmonic` in v1).

Transient (`reference_reconstruction_keeps_hf_onset_within_one_envelope`):
HF noise burst starting mid-frame on quiet LF noise → the frame is
coded with 4 envelopes (low resolution); onset of the reconstruction's
HF energy lags the original by **10 blocks of 64 samples (640)** — the
QMF analysis+synthesis pipeline — and the 16 blocks before the onset
are ≥ 77 dB down (no spread across the frame). Envelope resolution is
one FIXFIX envelope = 4 time slots = 512 output samples.

Bits and work (release, 10 frames, noise + 440 Hz tone, mono):

| cell | value |
|---|---:|
| `estimate` per frame | 3.5 µs |
| LC `encode_with` same PCM, per frame | 387 µs |
| ratio | 0.009× |
| `est_bits` mean | 61 bits/frame (≈ 1.4 kbps at 23.4 frames/s) |
| stationary mixed cell (`bit_estimate_fits_the_he_side_budget`) | ≤ 128 bits/frame gate; measured mean under it |
| `size_of::<SbrEstimator>()` | 280 B + band-table `Vec`s |

Search work is bounded: one pass over the slots per decision, two
codebook sums per envelope/floor; no rate loop.

## Findings that constrain the follow-on tasks

- **Core content must reach k0.** A patch-source subband with no
  energy gives `E_curr ≈ 0`; the limiter caps its gain to the limiter
  band's average and the noise floor is limited with it
  (`Q_M_lim = Q_M·G_max/G`), so the HF band stays empty (−26 dB seen
  with a 500 Hz comb ending at 5.4 kHz under k0 = 6.75 kHz). TASK-89
  must keep the LC core's psy cutoff ≥ k0 or lower `bs_start_freq`
  with the bitrate.
- Re-analysis through overlapping QMF bands cannot resolve finer than
  one subband; per-band gates need the edge rule above.
- The LC attack detector is blind above the core rate; the HF surge
  rule here is what codes an isolated cymbal with 4 envelopes.

## Determinism / not claimed

- `enc_sbr_est.rs` uses `det_math::log2` only; energies and tonality
  are plain `f32` sums (`no_libm_on_the_decision_path`). Band tables
  come from the decoder's libm master-table code once per stream and
  are pinned above.
- Reset + repeat is bit-identical; history matters without reset
  (`reset_and_repeat_are_identical`). Raw deltas round-trip through the
  decoder's DPCM (`deltas_round_trip_through_the_decoder_dpcm_…`).
- No `VARVAR`/`FIXVAR`, no `bs_add_harmonic`, no coupling, no
  listening or PEAQ; the tonality-gap rules for `Q`/invf are v1
  hypotheses to be retuned at TASK-89/95 against the HF-energy
  screening gate.
- No aarch64 run this session.
