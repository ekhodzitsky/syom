# TASK-133 — LC encoder core tuning on the TASK-95 deficits

Recorded 2026-09-24. Host: Linux x86_64 (AMD Ryzen AI 9 HX 370), rustc
1.97.1, ffmpeg 7.0.2-static (neutral decode, offline oracle only).
Evidence harness: `lab/quality` bins `syom_tune133` (syom-only cells,
same clips/metrics as `syom_he_qualify`) and `syom_det133` (offline
attack-detector sweep); peers from the pinned `syom_he_qualify` run
(fdk-aac 2.0.3 numbers as recorded in HE_QUALIFY.md, re-verified on the
final tree — see the matrix appendix in the task notes).

## Deficits (TASK-95, lab/quality/HE_QUALIFY.md)

- tremolo (8 Hz AM tonal stereo) 48 kbps: syom-lc SNR 21.0 / LF 25.9 dB
  vs fdk-lc 37.5 / 40.5, oxideav-lc 44.8 / 53.8 at matched actual rate.
- lecture 24 kbps: syom-he1 LF 15.1 vs fdk-he1 34.4; lecture-m mono:
  syom-lc LF 28.1 vs fdk-lc 31.9.

## Root causes found (each measured in isolation)

1. **Attack detector fired on smooth AM.** The flat mean of the previous
   16 sub-blocks (two frames) sits far below any point of an AM rise, so
   every 8 Hz lobe flagged an attack and the LongStart → EightShort →
   LongStop cluster smeared the tone. fdk-aac's `block_switch` instead
   compares each high-passed sub-block against a *leaky* accumulator fed
   the previous sub-block (`acc = 0.65·acc + 0.35·e_prev`, ratio 8).
   Offline sweep (`syom_det133`): tremolo 10 → 0 false attacks, all 8
   click trains still fire, tones/silence stay quiet. Effect on tremolo
   48 kbps LC: LF 25.9 → 29.3 dB (plus the 24 kbps alignment drift gone).
2. **TNS span force-coding was a budget bomb.** Every band in the TNS
   span was force-coded at `target_q = 1.0`; where TNS fired hardest
   (stream-start frame with a 30-band span, onset/offset and click
   clusters) the forced span blew the frame budget and `drop_bands`
   gutted the spectrum (frame 1 of tremolo: offset 240, block SNR 9.1
   dB). A dropped span band feeds the decoder's recursion zeros whose
   IIR tail decays within a few bins — fdk-aac does the same. Removing
   the force-coding: tremolo 48 kbps LF 29.3 → 48.3 dB, click-st SNR
   2.3 → 12.3 dB, lecture LC 24k LF 28.5 → 30.4 dB, lecture-m 28.1 →
   30.7 dB (TNS itself kept ON; its mono-speech gain survives).
3. **18 dB SMR was untuned.** Sweep 15/18/21/24/27/30 dB over the
   deficit cells and guards: 24–27 dB closed the remaining low-rate
   speech gap; 27 dB (SMR 501.19) was best on lecture/lecture-m LC and
   neutral-or-better everywhere else, 30 dB started to cost noise-st.
   At 128 kbps the change is neutral (bits were never scarce).

Remeasured 2026-09-27 on this tree (`syom_tune133`, ffmpeg 7.0.2
neutral decode). Every gated cell reproduced the table below to 0.1 dB:
tremolo LC 48/64, lecture and lecture-m at 24 kbps LC and HE v1, and the
noise-st / mix-st guards at 48/64.

## Final numbers (final tree, `syom_tune133`, neutral ffmpeg decode)

| cell | before | after | fdk (matched actual rate) | gate |
|---|---|---|---|---|
| tremolo lc 48k SNR / LF | 21.0 / 25.9 | **44.8 / 54.7** | 37.5 / 40.5 | within 1 dB → ahead by 14.2 |
| tremolo lc 64k SNR / LF | 27.4 / 31.0 | **46.2 / 55.9** | 37.9 / 41.4 | ahead |
| lecture lc 24k LF | 28.5 | **42.6** | 31.5 | gap closed (was 3.0) |
| lecture he1 24k LF | 15.1 | **31.5** | 34.4 | 85 % of the 19.3 dB gap closed |
| lecture-m lc 24k LF | 28.1 | **38.9** | 31.9 | gap closed (was 3.8) |
| lecture-m he1 24k LF | 22.2 | **32.2** | 32.7 | 95 % of the 10.5 dB gap closed |
| noise-st 48k SNR / LF | 0.9 / 2.5 | 0.9 / 2.5 | — | guard: no loss |
| noise-st 64k SNR / LF | 1.4 / 3.4 | 1.4 / 3.4 | — | guard: no loss |
| mix-st 48k SNR / LF | 14.8 / 19.5 | 17.6 / 23.7 | — | guard: improved |
| mix-st 64k SNR / LF | 15.0 / 19.7 | 18.0 / 24.2 | — | guard: improved |

Also improved, not gated: tremolo he1 24–64k LF 21.7–37.9 → 42.4–49.3;
voice-like / ambience / click-st LC at all rates (click-st 48k SNR
2.3 → 12.7); 128 kbps rows neutral-or-better (tremolo LF 57.7 → 64.0).

## Rejected / not done

- TNS off by default: better than force-coding on tremolo, but loses
  1.4 dB on lecture-m mono and the force-coding removal alone recovers
  the rest. TNS stays on.
- SMR 30 dB: lecture-m he1 slightly better, lecture LC and noise-st
  worse. SMR 24 dB: lecture LC 30.2 / lecture-m 30.8 pass but with less
  margin than 27 dB. 27 dB chosen.
- MC common-vs-fixed split: the retuned core flips the TASK-115 fixture
  ranking. Re-measured 2026-09-27 (`allocation_report`): the fixed split
  leads the front pair by 1.4 dB at 5.1/128k and still mutes the LFE,
  while the common offset holds the LFE at 20.4 dB. Common stays the ABR
  path (`MC_ALLOC.md`, TASK-135). No weight change.

## Low-rate HE core cutoff (2026-09-30)

At or below 24 kbps per channel, each core frame's psy cutoff is the top
of the highest 64-band analysis bin below the SBR crossover that sits
within 20 dB of that frame's peak. A flat spectrum and silence stay at
the crossover. `syom_tune133` lecture he1 24, neutral ffmpeg decode:

| clip | LF before | LF after | FDK |
|---|---:|---:|---:|
| lecture he1 24 | 31.5 | 36.6 | 34.4 |
| lecture-m he1 24 | 32.2 | 37.8 | 32.7 |

SNR moved 28.3 → 36.0 and 30.0 → 37.5. HF error moved 17.0 → 17.6 and
21.4 → 21.5. The same run's guards stayed on the printed tenth: lecture
LC 42.6 / 38.9, tremolo LC 48/64 LF 54.7 / 55.9, noise-st 48k 0.9/2.5,
noise-st 64k 1.4/3.4. Tremolo HE v1 at 24 kbps, remeasured after the
cutoff change, is LF 43.9 (inside the post-tune 24–64 kbps band
42.4–49.3; recorded FDK on that cell is 38.7).

## Second speech clip (2026-10-01)

The 20 dB constant was fit on `lecture.m4a` only. `syom_tune133 speech2`
rebuilds a second clip from `voice()` (not that file, not a sine) and
scores the same he1 24 cell: whole-stream 24 kbps, neutral ffmpeg 7.0.2
decode, declared-delay ±128. FDK is `lab/fdk/fdk_driver` encode-pcm AOT 5
(fdk-aac 2.0.3, afterburner off). PCM is not stored.

Two seconds at 48 kHz. Left: seed 41, f0 132 Hz, formants 270 and 2290 Hz.
Right: seed 73, f0 196 Hz, formants 730 and 1090 Hz. The second formant's
resonator is scaled to a time-domain peak 6 dB under the first. An 8 ms
highpassed noise burst sits at sample 0 and at 1.05 s (sample 50400) on
each channel, then the channel is peak-limited to 0.9. Mono (`speech2-m`)
is the mean, same construction as lecture-m. These seeds and f0 are not
the voice-like pair (3/4, 150/113 Hz).

| clip | layout | syom SNR | FDK SNR | syom LF | FDK LF | syom HF | FDK HF |
|---|---|---:|---:|---:|---:|---:|---:|
| speech2 | stereo | 12.4 | 2.5 | 15.5 | 5.6 | 22.2 | 17.1 |
| speech2-m | mono | 17.8 | 13.3 | 22.0 | 16.9 | 17.5 | 6.6 |
| lecture | stereo | 36.0 | 30.4 | 36.6 | 34.4 | 17.6 | 21.0 |
| lecture-m | mono | 37.5 | 28.5 | 37.8 | 32.7 | 21.5 | 23.6 |

Lecture LF stays on the 2026-09-30 tenths (36.6 / 37.8) and above FDK
34.4 / 32.7. On speech2, LF also clears FDK (15.5 ≥ 5.6, 22.0 ≥ 16.9).
HF does not. Lecture's accepted HF gap is syom under FDK (17.6 vs 21.0,
21.5 vs 23.6). speech2 is over FDK by 5.1 dB stereo and 10.9 dB mono.

Forcing the cutoff back to the crossover (not shipped) moves speech2 HF
only to 21.3 / 18.3 and drops lecture LF to 31.5 / 32.2, under the floor.
That swing is too small to close the HF gap and too large on lecture LF
to ship. No new ratio was searched. `FORMANT_RATIO` and the cutoff rule
stay. Encoder bytes and `src/goldens/he48*` stay.

## Reproduce

```sh
cargo run --release --manifest-path lab/quality/Cargo.toml --bin syom_tune133
cargo run --release --manifest-path lab/quality/Cargo.toml --bin syom_tune133 -- speech2
cargo run --release --manifest-path lab/quality/Cargo.toml --bin syom_det133
cargo run --release --manifest-path lab/quality/Cargo.toml --bin syom_he_qualify
```
