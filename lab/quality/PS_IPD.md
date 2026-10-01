# IPD/OPD for HE v2 encode (TASK-134, part 2)

Recorded 2026-09-27. Decision only: the encoder is unchanged
(`enable_ext` stays 0, no IPD/OPD). No new encode was run. Fine IID
(`iid_mode` 4) is out of scope. Numbers below are quoted; a cell that
is absent from HE_QUALIFY.md and HE_V2.md is **not measured**.

## Decision

**NO-GO.** One IPD/OPD envelope on the shipped `iid_mode` 1 grid costs
36 bits at the Huffman floor (0.84 kbps at 48 kHz, one access unit per
2048 output samples). That is already over the ~0.5 kbps side-info
gate, and no measured ICC gain is large enough to waive it. Ambience
with the tool on was not measured, so the no-regression gate is unmet
too.

## Writer today

`PsWriter::frame` writes `iid_mode` 1 (20-band coarse IID), `icc_mode`
1, `enable_ext = 0`, FIX class, one envelope or a zero-envelope hold.
The configuration header repeats every 8 access units and forces
frequency-differential rows. Hand vector (HE_V2.md): neutral header
55 bits, repeat 46 bits, hold 4 bits. No phase payload.

## Decoder

The shipped decoder accepts and applies IPD/OPD. It does not reject
the extension and it does not ignore id 0.

- `PsData::parse` (`ps_data.rs`), when `enable_ext` is set, reads a
  byte-counted extension. Id 0 is `enable_ipdopd`, then per envelope
  `ipd_dt`, `nr_ipdopd_par` Huffman IPD deltas, `opd_dt`, the same
  count of OPD deltas, and one reserved bit. Books are
  `HUFF_IPD_{DF,DT}` and `HUFF_OPD_{DF,DT}` (raw delta 0..7, `lav = 0`).
  An unknown id skips the rest of the block as fill.
- `resolve` accumulates IPD/OPD modulo 8. `enable_ipdopd = 0` clears
  the phase state (index 0).
- `PsDecoder::process_prepared` → `PsStereo::mix_to_qmf` →
  `fill_envelope_h` rotates the mixing matrix when `enable_ipdopd` is
  set: `e^(j φ_opd)` on H11/H21 and `e^(j (φ_opd − φ_ipd))` on
  H12/H22, after the three-point smoother on the `π/4` ladder.

Product HE v2 streams never set `enable_ext`, so the goldens do not
exercise the rotation. The code that would apply it is on the decode
path, not test-only.

`nr_ipdopd_par` is `NR_IPDOPD_PAR_TAB[iid_mode]` =
`[5, 11, 17, 5, 11, 17]`. Mode 1 therefore sends **11** phase bands,
zero-extended to the 20-band parameter grid before mapping. Stereo
bands 11..19 stay at phase index 0.

## Bit cost of one envelope

Shipped grid, one FIX envelope, extension id 0. This is the syntax
count, not a codec-average guess:

```text
bits = 2 (ps_extension_id) + 1 (enable_ipdopd)
     + 1 (ipd_dt) + Σ len(HUFF_IPD[Δ])    // 11 terms
     + 1 (opd_dt) + Σ len(HUFF_OPD[Δ])    // 11 terms
     + 1 (reserved)
     + pad so the extension body is a whole number of bytes
     + ps_extension_size                    // 4 bits if body < 15 bytes,
                                            // else 4 + 8
```

In-tree code lengths: delta 0 is 1 bit on all four books. Any other
delta is 3–5 bits (IPD DF max 4, IPD DT max 5, OPD DF max 5, OPD DT
max 5).

| case | parameter bits | content | after pad + size |
|---|---:|---:|---:|
| floor, every Δ = 0 | 22 × 1 | 28 | **36** |
| flat IPD (DF index 4, then ten Δ = 0) and OPD 0 | 4 + 10×1 + 11×1 | 31 | **36** |
| ceiling, every symbol at 5 bits | 22 × 5 | 116 | **132** (15-byte body, size escape) |

Rate at 48 kHz output, one access unit per 2048 samples
(`48000/2048 = 23.4375` frames/s):

```text
kbps = bits_per_frame × (48000/2048) / 1000
```

| payload | kbps if every AU carries it |
|---|---:|
| 36-bit floor | **0.84** |
| 132-bit ceiling | **3.09** |
| extension present, `enable_ipdopd = 0` (4 content bits → 12 with the size field) | 0.28 |

The ~0.5 kbps gate is 21.3 bits/frame. The floor of an envelope that
actually carries phase is already over it. The 0.28 kbps line carries
no phase.

Byte ceiling: the body is 4 bytes (floor) or 15 bytes (ceiling).
`PS_EXT_MAX_BYTES` is 270 and the SBR fill ceiling is 269 bytes, so
this does not blow the container budget. The gate it fails is the rate.

Outer `bs_extended_data` also byte-aligns `(2 + ps_bits)`. Adding the
36-bit floor to the 55-bit header grows that block by 32 bits; adding
it to the 46-bit repeat grows it by 40. With a header every 8 access
units the average is 39 bits/frame ≈ **0.91 kbps**, still over 0.5.

## Cited cells

HE v2 vs FDK HE v2, ILD err / ICC err in dB, from the HE_QUALIFY.md
matrix. HE v2 rows exist at 24/32/48 kbps only.

| clip | rate | syom-he2 ILD / ICC | fdk-he2 ILD / ICC |
|---|---:|---|---|
| click-st | 24 | 0.0 / 1.3 | −0.0 / 1.5 |
| click-st | 32 | 0.0 / 1.3 | 0.0 / 1.5 |
| click-st | 48 | 0.1 / 1.3 | 0.0 / 1.5 |
| ambience | 24 | −0.3 / 0.0 | 0.1 / 0.0 |
| ambience | 32 | −0.4 / 0.0 | 0.2 / 0.0 |
| ambience | 48 | −0.3 / 0.0 | 0.1 / 0.0 |
| anti-phase | — | not measured | not measured |

The 48 kbps spatial summary matches those rows: click-st syom-he2
+0.1 / +1.3 against fdk-he2 +0.0 / +1.5; ambience syom-he2 −0.3 / 0.0
against fdk-he2 +0.1 / 0.0. HE_QUALIFY.md calls click-st ICC
+1.1…+1.5 a shared SBR/PS artifact (regenerated HF correlates on both
encoders). syom is already 0.2 better than FDK on that cell without
IPD/OPD. Nothing in the matrix is an IPD/OPD-on minus IPD/OPD-off
delta.

Anti-phase ILD/ICC: **not measured** in HE_QUALIFY.md or HE_V2.md.
HE_V2.md only states the mechanism: no IPD/OPD, and the time-domain
downmix cancels exact anti-phase. HE_QUALIFY.md marks that as a
documented no-go by reference to PS_EST.md (estimator screen: ICC
index 7, mono core empty — not a qualify ILD/ICC error). A decoder
rotation cannot rebuild a core the downmix already zeroed, and at
`iid_mode` 1 the phase vector does not cover the upper 9 of 20 bands.

Ambience ICC error is already 0.0 with the tool off. Ambience with
IPD/OPD on: **not measured**.

## Why the gates fail

Adopt only if the side-info stays bounded and ambience does not
regress. Neither is shown.

1. Side info. The current cadence is one envelope per access unit.
   Floor 36 bits/frame = 0.84 kbps (0.91 kbps once the SBR extension
   is byte-aligned). Over ~0.5 kbps. Container ceiling is not the
   problem. There is no large measured ICC gain to set against that
   cost: click-st is already at parity with FDK.
2. Ambience. No on-versus-off ILD/ICC, so "no regression" is not
   shown. Anti-phase, the other decision cell, has no qualify score
   at all.

## Single measurement required later

One paired run, not a new default: anti-phase fixture with an IPD/OPD
envelope on versus today's writer, same whole-stream rate. Report
post-decode L/R correlation (or core energy), the ambience cell's
ILD/ICC, and the measured PS side-info kbps. Revisit this note only
if correlation actually moves, ambience ILD/ICC does not regress, and
average side info stays ≤ ~0.5 kbps — or if the ICC gain is large
enough that the rate overage is stated explicitly. A phase-aligned
downmix is a different change; this note does not authorize it.
