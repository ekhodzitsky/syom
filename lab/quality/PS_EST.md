# TASK-91 — parametric-stereo analysis (HE v2 encoder, stage 1)

`engine/enc_ps_est.rs`: stereo in, energy-compensated mono downmix plus
per-frame IID / ICC indices out. Crate-internal; payload syntax is TASK-92
and the public HE v2 mode TASK-93.

## Mode and method

- Baseline PS: 20 stereo bands, coarse IID (index −7..7), ICC (0..7), one
  envelope per 32-slot frame, no IPD/OPD.
- Analysis on the encoder's 64-band QMF of L and R; QMF bands 0–2 split by
  the standard 13-tap hybrid filters (Type A ×8 merged to 6, Type B ×2,
  ×2), twiddles from `det_math`. All bands are delayed alike, so the
  estimator lags its input by **6 QMF slots (384 output samples)**; TASK-93
  must align the parameter stream to the decoder's frame with that lag.
- Per band: `e_L`, `e_R`, `Re Σ l·r*` over the frame plus a quarter of the
  previous frame; `IID = 10·log10(e_L/e_R)`, `ICC = Re/√(e_L·e_R)`, each to
  the nearest grid value. A silent side keeps `ICC = 1`.
- Downmix: `m = g·(l+r)/2` per 2048-sample block,
  `g = √((E_L+E_R)/(2·E_M))` in `[1, 2]`, limited by the block's headroom,
  ramped linearly from the previous block. Blocks are the unit of state, so
  any feed chunking the caller buffers into blocks gives the same result;
  `reset` restores the initial state (tested).

## Screening results (`enc_ps_est_tests.rs`)

Reconstruction model: `e_L' = 2·e_M·c/(1+c)`, `e_R' = 2·e_M/(1+c)`,
`c = 10^(IID/10)`, against the source band energies, over bands holding at
least 1% of the frame energy.

| signal | IID index | ICC index | worst model error L / R / L+R |
|---|---|---|---|
| centre (L = R) | 0 | 0 | 0.0 / 0.0 / 0.0 dB |
| pan 6 dB | 3 | 0 | 0.2 / 0.8 / 0.0 dB |
| pan 14 dB | 5 | 0 | 0.0 / 0.0 / 0.0 dB |
| hard left | 7 | 0 | 0.0 / — / 0.0 dB |
| mirrored pans | negated | 0 | same |
| decorrelated noise | within ±2 | 3..6 | 0.7 / 3.3 / 2.8 dB (total within 1 dB) |
| anti-phase (R = −L) | 0 | 7 | mono core is empty |
| R = −0.5·L | — | 7 | core survives |
| lecture + noise bed on R | in range | in range | IID moves ≤ 6 steps per frame, no clipping |

Targets used: panned material ≤ 1 dB per channel; ambience ≤ 4 dB per
band and ≤ 1 dB in total (a passive downmix loses 3 dB there).

## Irreversible loss

Exact anti-phase content cancels in the downmix: the estimator reports
`ICC = −1`, but the core carries nothing to rebuild from. A band-wise
downmix with phase alignment (or IPD/OPD) would be the fix; it needs an
encoder-side QMF synthesis and is left as a measured follow-up.

## Cost and determinism

83.7 µs per 2048-sample block (510× realtime, Ryzen AI 9 HX 370), about
19.6 KB of state, no allocation after construction apart from the 13-slot
ring. Decision path: `det_math` log2 / sincos, exact sqrt and division,
fixed summation order; a pinned FNV hash of parameters and downmix bits is
the cross-architecture tripwire.

Limits: synthetic references plus one short natural clip; no listening;
the ambience band error comes from the broadband (not band-wise) gain.

## TASK-134 — fine IID grid (2026-09-27) — NO-GO

**NO-GO.** `iid_mode` 4 does not halve mean |ILD| versus mode 1 on
ambience or mix-st, so the product path stays coarse `iid_mode` 1.
Side info was inside the cap (at most +0.079 kbps) and was not the
reason.

Same cells and broadband ILD/ICC as `syom_he_qualify` (signed error =
decoded − source, after the declared 3018-sample lag refined by ±128).
Neutral decode: ffmpeg 7.0.2. Whole-stream target 24/32/48 kbps; actual
ADTS kbps stays matched (largest gap 25.97 vs 26.10 on ambience at 24k).
`ps kbps` is `ps_data()` bits over source seconds; `ext kbps` is the
on-wire extended-data block (size field + byte payload), which is what
the rate loop spends. PS bits do not depend on the target rate.

Mean |ILD| is the mean of the three rates. Halving would need ambience
≤ 0.019 dB and mix-st ≤ 2.365 dB.

| clip | mode | actual k (24/32/48) | ILD err (24/32/48) | mean \|ILD\| | ICC err (24/32/48) | ps kbps | ext kbps |
|---|---:|---|---|---:|---|---:|---:|
| ambience | 1 | 25.97 / 34.17 / 50.56 | −0.025 / −0.062 / −0.029 | 0.039 | +0.034 / +0.035 / +0.034 | 1.758 | 2.012 |
| ambience | 4 | 26.10 / 34.18 / 50.56 | +0.085 / +0.051 / +0.083 | 0.073 | +0.012 / +0.012 / +0.012 | 1.837 | 2.084 |
| mix-st | 1 | 26.04 / 34.11 / 50.47 | −4.737 / −4.722 / −4.733 | 4.730 | −0.013 / −0.012 / −0.013 | 1.691 | 1.952 |
| mix-st | 4 | 26.04 / 34.11 / 50.48 | −4.656 / −4.641 / −4.652 | 4.650 | −0.017 / −0.016 / −0.016 | 1.734 | 1.976 |
| tremolo | 1 | 26.37 / 34.59 / 51.43 | +0.062 / +0.062 / +0.062 | 0.062 | 0.000 / 0.000 / 0.000 | 1.127 | 1.284 |
| tremolo | 4 | 26.37 / 34.59 / 51.43 | +0.062 / +0.062 / +0.062 | 0.062 | 0.000 / 0.000 / 0.000 | 1.127 | 1.284 |

Side-info delta (mode 4 − mode 1): ambience +0.079 ps / +0.072 ext kbps,
mix-st +0.043 / +0.024, tremolo +0.000 / +0.000. All ≤ 0.3 kbps.

Reading: mix-st's −4.7 dB image shift barely moves (the loss is the
mono downmix, not the coarse grid). Ambience is already inside 0.1 dB
on mode 1 after the TASK-133 core tune (the older HE_QUALIFY.md row of
about −0.3 dB is not this build) and the fine grid does not tighten it.
Tremolo's constant ~2 dB pan lands on the same grid point either way,
so the payload is unchanged. ICC on ambience improves (0.034 → 0.012)
but that was not an adopt gate.

`EncodeOptions::with_he_v2` does not set the hidden `ps_iid_fine` flag.
Mode 4 remains reachable for this measurement
(`cargo run --release --manifest-path lab/quality/Cargo.toml --bin syom_he_qualify -- iid`).
