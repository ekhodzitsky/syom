# Listening protocol pin (TASK-14)

Isolated from the `syom` package. **Not invoked by `cargo test` except
the ordinary tests that spawn `scripts/listen_protocol.py` on SYNTHETIC
scores.** No ffmpeg, FDK, PEAQ, ViSQOL, or human listeners.

## Procedure

```sh
python3 scripts/listen_protocol.py dry-run --out lab/listen/dryrun --seed task-14
python3 scripts/listen_protocol.py verify --out lab/listen/dryrun
```

Seed `task-14` is the committed dry-run. Output is labeled
`SYNTHETIC-NOT-LISTENING-EVIDENCE`.

## ITU / DOC-3

| Pin | Value |
|---|---|
| MUSHRA | ITU-R BS.1534-3 |
| Near-transparency | ITU-R BS.1116-3 |
| Noninferiority margin | 3 MUSHRA points (DOC-3) |
| Planning SD | 10 points (literature; live pilot replaces) |
| Qualification n | 69 valid after screening |
| Exploratory n | 20 (not decisive) |
| Hidden-ref screen | < 90 on > 15% of items |
| Low-anchor screen | > 90 on > 15% of items |
| Excerpt | 12 s |
| Session | ≤ 45 min, ≤ 12 items |
| Loudness | −23 LUFS, true-peak ≤ −1 dBTP |
| Priming skip | 1024 (TASK-40) |

## Peers

| Engine | Status on this host |
|---|---|
| syom LC | available (in-tree) |
| FFmpeg 9.0.1 native `aac` | available (TASK-6/16 lab) |
| FDK 2.0.3 | prefix not installed |
| FAAC 1.31.1 | prefix not installed |
| glint-audio 0.11.0 | encode-pcm rebuild |
| Apple AAC | no AudioToolbox host (TASK-10) |
