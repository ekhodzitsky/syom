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
