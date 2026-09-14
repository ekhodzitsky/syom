# TASK-70 causal pre-echo vs attack position

Recorded 2026-09-14. Host: Linux x86_64. Decoder: syom `decode_with`
unbounded of ADTS from shipped `encode_with`. 10 ms (480 samples)
pre-onset error energy on digital silence + a 32-sample click at
in-frame position `pos` of frame 5 / 14.

## Named baseline cases

| Case | Window | Notes |
|---|---|---|
| causal pos 50–400 | LongStart this frame | LongStart stays flat for the first ~448 NEW samples |
| causal pos 700–900 | LongStart + shorts next | click in the start-window tail; already ≥6 dB vs long-only |
| lookahead pos 50–400 | LongStart previous, shorts here | +1 frame latency, explicit `with_lookahead` |

Predeclared failing subset: **causal pos < 448**.

## Lookahead vs causal (shipped APIs)

| pos | Δ pre-onset energy |
|---:|---:|
| 50 | 19.5 dB |
| 100 | 32.7 dB |
| 200 | 33.9 dB |
| 400 | 37.2 dB |

Late click (pos 700) still reconstructs (peak > 0.05). Causal sine/silence
bytes unchanged. Lookahead decoded length matches causal (one extra MDCT
drain both ways; lookahead's extra latency is internal).

## Rejected candidates (not shipped)

| Candidate | Result |
|---|---|
| LongStart 4× `target_q` (same coded mask) | **+0.6 dB** on raw-payload pos 100 (below 3 dB). Lowering the floor to −80 dB coded *more* smeared bins and the rate loop coarsened (**−1.0 dB** on ADTS pos 50). |
| `ATTACK_RATIO` 8 → 4 | `detector_is_quiet_on_tones_and_silence` **fails** (silence flagged). |

Causal early-in-frame smear is the long MDCT basis, not scalefactor depth.
≥3 dB on that subset without extra latency was not achieved. Lookahead
already exceeds the gate at +1 frame.

## Go / no-go

**No-go** a new default temporal-masking / attack-threshold change.
**Keep** explicit `with_lookahead` (TASK-47) as the ≥3 dB fix. Causal mode
gains no latency. Goldens not reminted. CPU of rejected psy path not
promoted (default identical).

Reproduce: `cargo test --lib encode_preecho -- --nocapture` (ordinary tests;
ffmpeg never spawned).
