# TASK-115 — surround bit allocation: one allowed-noise offset per frame

Decision: **go**. ABR surround frames search one allowed-noise offset for
all elements (`McEncoder::encode_common`); the fixed weight split of
TASK-114 stays only for quality VBR (fixed offset anyway) and as the lab
A/B switch.

## Method

Programme (deterministic, 97 frames, 48 kHz): FL/FR a 12-harmonic tone
over a −34 dB noise floor (FR adds a 2 kHz tone), FC amplitude-modulated
noise, LFE 60 Hz tone, BL/BR quiet ambience noise, SL/SR (7.1) a soft
tone. Per-plane SNR of the syom decode against the source; total ADTS
bytes; wall time of the whole encode (release, Ryzen AI 9 HX 370).
`cargo test --release --lib -- --ignored allocation_report --nocapture`.

| case | split | ADTS kbps | ms | SNR dB per plane (FL FR FC LFE BL BR [SL SR]) |
|---|---|---:|---:|---|
| 5.1 128k | fixed | 131.1 | 42.9 | 13.2 11.6 0.9 **0.0** 0.9 1.0 |
| 5.1 128k | common | 131.0 | 38.1 | 15.8 15.5 1.3 16.2 0.8 0.8 |
| 5.1 256k | fixed | 259.3 | 47.3 | 16.5 16.6 2.5 26.8 2.5 2.6 |
| 5.1 256k | common | 258.7 | 50.1 | 16.4 16.3 4.5 16.7 3.9 3.8 |
| 7.1 192k | fixed | 195.4 | 52.7 | 14.7 12.7 1.0 **0.0** 1.1 1.1 15.5 16.1 |
| 7.1 192k | common | 195.0 | 56.7 | 15.9 15.6 2.3 16.6 1.7 1.8 15.4 15.9 |
| 7.1 320k | fixed | 323.7 | 60.1 | 16.4 16.3 2.1 18.5 2.2 2.2 16.8 16.3 |
| 7.1 320k | common | 323.1 | 65.7 | 16.4 16.3 5.0 16.7 4.4 4.3 16.6 16.1 |

ADTS kbps includes 2.7 kbps of headers; payload is within ±3% of the
request in every row. Stereo LC 128k on the front pair alone: 19.4 ms.

## Reading

- Under shortage the fixed split starves the busy front pair (its share is
  2/5.1 of the stream) while quiet planes turn their share into stuffing,
  and the 0.1-weight LFE is **silent**. The common offset gives the fronts
  +2.6 / +3.9 dB (5.1) and +1.2 / +2.9 dB (7.1) and keeps the LFE at 16 dB.
- With spare bits both modes put the tonal planes at the psy target
  (about 16.5 dB SNR on this material: the model's 18 dB signal-to-mask
  ratio, the same figure stereo LC reaches at quality level 5); the common
  offset then spends the surplus on the noise planes (+2 dB) instead of
  the LFE, which the split over-served (26.8 dB).
- Cost: 6.4 ms per plane (5.1 at 128k) against 9.7 ms per plane for
  stereo; the search runs about 11 builds per element per frame, the same
  bound as stereo.

## LFE policy

Cut-off 120 Hz, long windows, no TNS (TASK-114). Allowed noise: the common
offset **minus 16 steps (12 dB)**. A lone sub-bass tone has nothing else in
its element to mask the noise; without the bias the LFE sat at 10.7 dB at
128k. The element costs a few dozen bits per frame, so the bias is free.

## Hard cap

Short-window bursts can exceed the budget even at the offset ceiling (the
short path has no water level). Each element then drops top bands to its
proportional share, as the stereo loop's hard cap does; the stress test
(noise + clicks at 96–320 kbps) holds every frame within 1.5× budget and
the mean within [0.90, 1.03].

Limit of the evidence: SNR on synthetic material, no listening; the
common offset equalizes noise-to-mask by construction, SNR only shows
where the bits went.

## After the TASK-133 core (2026-09-27)

Same programme, 97 frames, release, this tree
(`cargo test --release --lib -- --ignored allocation_report --nocapture`).
The 27 dB mask ratio lifted every tonal plane by about 5–8 dB versus the
table above. It also flipped the shortage ranking: the fixed split no
longer pays the old TNS-span tax, so the busy front pair is slightly
ahead of the common offset.

| case | split | ADTS kbps | ms | SNR dB per plane (FL FR FC LFE BL BR [SL SR]) |
|---|---|---:|---:|---|
| 5.1 128k | fixed | 130.9 | 39.0 | 23.1 22.9 1.0 **0.0** 0.9 1.0 |
| 5.1 128k | common | 130.8 | 40.0 | 21.6 21.7 1.4 20.4 1.0 1.0 |
| 5.1 256k | fixed | 259.2 | 47.0 | 23.5 23.5 2.5 27.1 2.5 2.6 |
| 5.1 256k | common | 258.8 | 50.2 | 22.9 22.6 4.8 25.3 4.2 4.3 |
| 7.1 192k | fixed | 195.0 | 60.8 | 23.3 23.1 1.0 **0.0** 1.1 1.1 21.1 21.6 |
| 7.1 192k | common | 195.0 | 54.3 | 22.3 22.2 2.5 24.2 2.0 2.0 20.3 20.7 |
| 7.1 320k | fixed | 323.6 | 63.2 | 23.4 23.5 2.2 25.1 2.2 2.2 21.8 22.5 |
| 7.1 320k | common | 322.7 | 62.6 | 23.1 22.9 6.5 25.2 6.0 6.0 20.6 21.0 |

Front-pair gap, fixed minus common: 1.4 dB at 5.1/128k, 0.8 dB at
5.1/256k, 1.0 dB at 7.1/192k, 0.5 dB at 7.1/320k. At the two shortage
rates the fixed split still encodes the LFE as silence (0.0 dB) while
the common offset, with the same −16 step bias, holds it at 20.4 and
24.2 dB. Payload stays inside the ±3 % band on every row.

Decision: keep one common offset for ABR. A weight change that handed
the front pair that 1.4 dB back would spend the LFE's few dozen bits
and recreate the silence the bias exists to prevent. The fixed split
stays the quality-VBR path only. No encoder bytes change.
