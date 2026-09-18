# TASK-104 — provenance of tables, algorithms, fixtures and the package

Audit date 2026-09-18. This is an inventory, not a legal opinion: nothing
here assesses patents or the copyright status of standardized numeric
tables. Machine-checked parts: `corpus/oracles/provenance.json`
(`scripts/verify_oracle_provenance.py`), `corpus/manifest.json`
(`scripts/verify_corpus.py`), the packaged crate
(`scripts/check-package.py`).

## 1. What the published crate contains

`cargo package` ships 143 UTF-8 text files: library sources (no
`*_tests.rs`), `README.md`, `CHANGELOG.md`, `LICENSE` (MIT), `NOTICE`
(the patent disclaimer the README links to — this audit found it had
dropped out of the package and restored it), the manifests. The audit script fails on any non-text file, on a missing MIT
notice, on any dependency, build script or `links` key. **No fixture,
corpus item, comparator adapter or binary is distributed in the crate**, so
the MIT claim covers exactly: original Rust code plus the numeric tables
of §2.

## 2. Non-original numeric tables (all in `src/engine/`)

Origin for every row: ISO/IEC 14496-3:2009 (Subparts 4 and 8), cited per
table in the file header. They are interoperability constants: a decoder
cannot be conformant with other values. No ISO text is reproduced.

| file | content | cited source |
|---|---|---|
| `huff_quad.rs`, `huff_pair.rs`, `huff_esc.rs` | spectral Huffman books 1–11 | Tables 4.A.2–4.A.12 |
| `sf_tab.rs` | scalefactor Huffman book | Table 4.A.1 |
| `swb.rs` | scalefactor-band offsets | Tables 4.129–4.141 |
| `tns.rs` | TNS max bands | Table 4.137-class limits, §4.6.9 |
| `sbr_huffman.rs` | SBR envelope / noise Huffman books | Annex 4.A.6.1 |
| `sbr_qmf.rs` | 640-tap QMF prototype | Table 4.A.89 |
| `sbr_noise_table.rs` | 512 complex noise values | Table 4.A.91 |
| `sbr_freq_bands.rs` | frequency-band helper constants | §4.6.18.3.2 |
| `ps_huffman.rs` | IID / ICC / IPD / OPD Huffman books | Tables 8.B.17–8.B.20 class |
| `ps_map.rs`, `ps_stereo.rs`, `ps_decorr.rs`, `ps_hybrid.rs` | band maps, IID / ICC grids, all-pass and hybrid prototypes | Tables 8.25–8.49, Annex 8.A |
| `enc_ps_est.rs` | copies of the IID / ICC grids and two hybrid prototypes (f32) | same tables |

Verification that the values are right is empirical and independent of
how they were typed in: decode goldens match libavcodec within 1 LSB, and
the writers' output is decoded by libavcodec.

The owner's `NOTICE` states the Huffman / SWB tables "are ISO numbers,
not a third-party decoder dump".

**Unresolved (non-blocking, explicit):** these files entered the
repository in the initial import commit `0ce5d40` (2026-09-02). The
extraction method (typed from the standard, generated, or cross-checked
against another decoder's tables) is not recorded for that import. Comments
name FAAD2 and libavcodec only as behavioural oracles ("order as FAAD2
reads it", "lavc layout order"); no source file of either is present, and a
search for licence headers or upstream identifiers in `src/` finds none.

## 3. Independently authored algorithms

Written for this repository, with their design notes and measurements in
`lab/quality/` — none is a port:

- LC encoder: attack detector, block switching, Bark psy model,
  noise-to-mask allocation, rate loop, TNS / M/S / PNS / IS decisions,
  section planner (`REPORT.md`, `LOWRATE.md`, `SPEED.md`, …).
- HE v1 encoder: halfband + QMF front end, SBR estimator, SBR bit writer
  (`SBR_PREP.md`, `SBR_EST.md`, `SBR_BITS.md`, `HE_AU.md`).
- HE v2 encoder: PS estimator, downmix, PS writer (`PS_EST.md`,
  `HE_V2.md`). Surround orchestration and allocation (`MC_ENC.md`,
  `MC_ALLOC.md`). `det_math` (deterministic log2 / exp2 / sincos / atan).
- Decoder numerics follow the standard's formulas; where libavcodec
  differs (f32 TNS), syom keeps its own path (`tns_lavc_tests.rs`).

## 4. Comparators stay in the lab

| directory | upstream licence | what is in the tree |
|---|---|---|
| `lab/fdk` | Fraunhofer FDK AAC licence (not MIT) | 8 files, 19 KB: own adapter, driver, Makefile, PIN |
| `lab/faad2` | GPL-2.0-or-later | 7 files, 15 KB: own process wrapper |
| `lab/faac` | LGPL-2.1-or-later | 10 files, 16 KB: own adapter |
| `lab/libavcodec` | LGPL / GPL (build-dependent) | 4 files, 21 KB: own adapter |
| `lab/fdk-aac-rust`, `lab/glint` | per their PIN files | own drivers only |

No upstream source, object or binary is vendored; adapters link against
what the operator installs. None of `lab/` ships in the crate.
Dev-dependencies (criterion, symphonia (MPL-2.0), rusty_aac, oxideav-aac)
are bench-only and absent from the packaged dependency table.

## 5. Fixtures (repository only, not in the crate)

- 71 oracle records; 3 without a provenance gap; 68 carry an explicit gap.
  For records minted in this programme the gap is only "ffmpeg 7.0.2-static
  build flags not recorded". For the historical records (import commit and
  early September) the encoder identity and command are not recorded.
- All tonal / noise / impulse fixtures are synthetic; a bitstream produced
  by running an encoder on synthetic input embeds no third-party work.
- `ps48.adts` (commit `0ce2ac6`) is described as a "real-stereo" HE v2
  golden; its source and encoder are not recorded.

## 6. Release blockers

| # | item | why | owner | how to clear |
|---|---|---|---|---|
| B1 | `src/goldens/lecture.m4a`, `corpus/rate/lecture.m4a` | natural speech; the manifest says "MIT (syom fixture; not a licensed natural recording)" but records no speaker, source or consent | repository owner | record origin and permission in `corpus/manifest.json`, or replace with a recording the owner made or a CC0 clip and re-mint the dependent goldens |
| B2 | `src/goldens/ps48.adts` | "real-stereo" source and encoder unrecorded | repository owner | same as B1, or re-mint from a synthetic stereo programme with `with_he_v2` plus an ffmpeg oracle |

Both block a **repository** release that advertises redistributable
fixtures; neither affects the crate, which ships no fixture. Non-blocking
gaps: table extraction method (§2) and historical encoder identities (§5).

## 7. Statements this audit does not make

No patent clearance, no claim that AAC can be implemented royalty-free in
any jurisdiction, no claim about the copyright status of ISO tables. The
README's licence line ("MIT") describes the code in the crate only.
