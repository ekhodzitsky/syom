# Preregistered AAC listening protocol (TASK-14)

Locked 2026-09-14, before TASK-109 qualification. Procedure:
`scripts/listen_protocol.py`. Dry-run scores are SYNTHETIC and are
**not listening evidence**. Grades that did not come from a human
listener do not count (DOC-3).

References: [ITU-R BS.1534-3](https://www.itu.int/rec/R-REC-BS.1534/en)
(MUSHRA), [ITU-R BS.1116-3](https://www.itu.int/rec/R-REC-BS.1116/en),
DOC-3 § Encoder quality.

## 1. Method selection (locked)

| Cell | Method | Reason |
|---|---|---|
| LC ≤ 96 kbps, any class | BS.1534-3 MUSHRA | intermediate impairment |
| LC any rate, speech / radio / percussion-transient | MUSHRA | pre-echo and texture stay intermediate (TASK-16/70) |
| LC ≥ 128 kbps, tonal-music / dense-music | BS.1116-3 | near-transparency; MUSHRA ceiling |
| HE v1/v2 encode | deferred | no HE encoder (TASK-90/93) |

A trial never mixes methods. Two sittings are allowed if both methods
appear; one analysis plan still applies per method.

## 2. Reference, anchors, conditions

MUSHRA (per item): labeled open reference (original PCM); hidden
conditions = hidden reference, 3.5 kHz low-pass (mandatory), 7 kHz
mid-anchor, syom LC, lavc FFmpeg 9.0.1 native `aac`. Scale 0–100.

BS.1116 (per item, per codec): triple-stimulus hidden reference. A is
the open reference; B/C are the hidden reference and the codec in
random order. Scale 1.0–5.0 impairment (0.1 steps).

Optional peers (FDK 2.0.3, FAAC 1.31.1, glint 0.11.0, Apple AAC) enter
a trial only when a matched actual-bitrate encode of the **same** PCM
exists (DOC-3 ±1% elementary-stream over the excerpt). They are not
placeholders.

## 3. Level, trim, duration

- Decode every candidate through the same independent decoder.
- Skip encoder priming (1024 core samples, TASK-40). Trim remainder /
  `elst` presentation end (TASK-43).
- Loudness: EBU R128 / BS.1770-4 −23 LUFS, true-peak ≤ −1 dBTP.
- 20 ms fade in/out on every condition.
- Excerpts 10–12 s (protocol uses 12 s).
- After pad, **every hidden condition in a trial has identical
  `n_samples`**. Unequal coded drain must not leak through duration.
- Filenames are `itemNN/cKK.meta.json` plus a sealed `key.json` that
  listeners never see. Blind IDs are 8-char Crockford-style tokens.
  UI order is an independent Fisher–Yates shuffle (SHA-256 seed).

## 4. Inclusion and screening

Pre: self-reported normal hearing, experienced listeners (BS.1534-3
§4), age recorded but not a gate.

Post (MUSHRA, BS.1534-3): exclude a listener who scores the hidden
reference **below 90** on more than **15%** of MUSHRA items, or scores
the 3.5 kHz anchor **above 90** on more than 15%. BS.1116: exclude if
the hidden reference is not identified on more than 15% of pairs.

Excluded listeners are reported, not imputed.

## 5. Sample size, stopping, analysis

Planning SD = **10 MUSHRA points** (literature residual). Live TASK-109
replaces this with a **dev-split** pilot (never holdout).

Noninferiority: one-sided 95%, power 80%, margin **3 MUSHRA points**
against the strongest same-rate included peer (DOC-3).

```
n = ceil(((1.64485 + 0.84162) * σ / 3)²)
```

With σ = 10, **n = 69** valid listeners after screening. n = 20 is
**exploratory only** (DOC-3). BS.1116 analogue margin = **0.1 grade**.

Stopping: **fixed n**. No optional peek-and-add. An inconclusive CI at
n is reported as inconclusive (TASK-109), not as a win.

Primary: paired per-listener mean of (syom − strongest peer) over
items, t 95% CI. Noninferior if CI lower bound > −3. Superior only if
the CI lies entirely above 0. Per-category tables are visible.
rmANOVA of condition × item is descriptive.

## 6. Corpus, peers, burden

Held-out **recordings** (TASK-2, not neighboring excerpts):

`openslr12-spk-1580`, `openslr12-spk-1995`, `openslr12-spk-2094`,
`commonvoice-ar-corpus`, `commonvoice-ja-corpus`,
`librivox-war-of-the-worlds`, `musopen-chopin-op28`,
`fma-cc0-percussion`.

Required peers: syom LC (revision frozen by TASK-108) and FFmpeg 9.0.1
native `aac`. Match actual elementary-stream bitrate; report transport
overhead separately.

Burden: ≤ 12 items/block, training ≤ 5 min, session ≤ 45 min. Current
8-item packet: 7 MUSHRA + 1 BS.1116 ≈ 17.5 min. One sitting.

## 7. What this task is not

- Not a completed listening study (TASK-109).
- Not PEAQ/ViSQOL (unavailable here; SNR is not a substitute).
- Synthetic dry-run grades must keep the label
  `SYNTHETIC-NOT-LISTENING-EVIDENCE`.
