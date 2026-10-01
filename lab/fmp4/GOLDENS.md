# TASK-126 — committed fMP4 golden manifest (contract cells of REPORT.md §2)

Every committed `src/goldens/fmp4_*` fixture, the contract cell it covers,
its generator and its sha256 pin. `verify_pins.py` re-checks the pins.
Generators are offline (`gen_goldens.sh`, ffmpeg 7.0.2-static per PIN.md;
`strip_tfdt.py`; `gen_timeline_oracle.py`); product tests replay the
committed bytes and never spawn ffmpeg/ffprobe.

## Contract-cell matrix (REPORT §2)

| cell | fixture(s) | test |
|---|---|---|
| tfhd explicit `base_data_offset` | `fmp4_lc.mp4` | stream_fmp4_tests, timeline |
| tfhd `default_base_is_moof` | `fmp4_lc_dbmoof.mp4` | stream_fmp4_tests |
| tfhd implicit base (prev fragment end) | `fmp4_lc_omit.mp4` | stream_fmp4_tests |
| trun v0 / v1 | `fmp4_lc.mp4` / `fmp4_lc_v1.mp4` | stream_fmp4_tests, timeline |
| defaults chain trun → tfhd → trex | `fmp4_lc_cmaf.mp4` (+ synthetic units) | stream_fmp4_tests, isomp4_frag_tests |
| dash init + segments (styp/sidx, init elst) | `fmp4_dash_init.m4s` + `fmp4_dash_seg1/2.m4s` | stream_fmp4_tests |
| HE v1 / v2 explicit-ASC | `fmp4_he.mp4` / `fmp4_he2.mp4` | stream_fmp4_tests, timeline (v1) |
| priming elst vs untrimmed mov-muxer | `fmp4_dash_*` vs `fmp4_lc.mp4` + `fmp4_lc_flat.m4a` | isomp4_frag_timeline_tests, stream_fmp4_tests |
| tfdt-less dts accumulation | `fmp4_lc_notfdt.mp4` | timeline, stream_fmp4_qualify_tests |
| mfhd sequence gap | `fmp4_lc.mp4` mutated off the oracle sequence | isomp4_frag_timeline_tests, stream_fmp4_tests, isomp4_frag_tests |
| truncated init / moof / mdat, dangling header | `fmp4_lc.mp4` cut in test + `corpus/fuzz/fmp4-truncated.bin` | stream_fmp4_qualify_tests, fuzz_smoke_tests, hostile_campaign_tests |
| encrypted (cenc: `enca`/`saiz`/`saio`/`senc`) | `fmp4_cenc.mp4` | stream_fmp4_tests (+ synthetic traf-level units) |
| multi-track | `fmp4_2track.mp4` | stream_fmp4_tests |
| inconsistent declared base (ffmpeg `global_sidx`, §4.2) | `fmp4_lc_sidx.mp4` | stream_fmp4_tests |
| third-party GPAC-muxed vector | `fmp4_bbb_init.m4a` + `fmp4_bbb_seg1.m4a` | stream_fmp4_qualify_tests, timeline |
| ffprobe timeline oracle (per-sample dts/cts, elst priming vs untrimmed, tfdt accumulation, mfhd sequence) | `fmp4_timeline.txt` (lc, notfdt, he, bbb, dash) | isomp4_frag_timeline_tests |
| zero duration without `duration_is_empty` | synthetic boxes | `zero_duration_needs_duration_is_empty` |
| nonzero composition-time offset | synthetic boxes | `rejections_are_typed_never_silent` |
| sample-description switch | synthetic boxes (`tfhd` index ≠ 1) | `rejections_are_typed_never_silent` |
| zero composition offset on trun v1 | `fmp4_lc_v1.mp4` + synthetic | `zero_cto_and_trun_v1_are_accepted`, stream_fmp4_tests |

## sha256 pins

| fixture | sha256 |
|---|---|
| `fmp4_lc.mp4` | `5abb2331c4ca03d6924b1f479a653571e72a9103f370a6993ed43171260fed15` |
| `fmp4_lc_dbmoof.mp4` | `e93ac3acbaeffc92f3b61a63cc2bfe7022918a59b256703fa32bd3d2b66ed6ff` |
| `fmp4_lc_omit.mp4` | `8306684e909d2e1a407fa024f86957d4cfbfcb9a683a3830d0e8a1f8c6734f63` |
| `fmp4_lc_v1.mp4` | `9f50f632842c1fa515aa51c78e29c6142b69a52e6c9e3e8febb4713dfbb57050` |
| `fmp4_lc_cmaf.mp4` | `4b817c3e8ee5db6d00a5fda3c355b54d5fd622850c114f9fd327c362f1e82225` |
| `fmp4_lc_sidx.mp4` | `a3a4afbe4eb59117a0a3ce5368b382187a41cdfcba27244636f431380a142a94` |
| `fmp4_lc_flat.m4a` | `503d30833257b60e79c3110e8bfd7fdbd688ba97841a19e2dcfc37f51a056c51` |
| `fmp4_lc_notfdt.mp4` | `7f455d38fbf9c74168e39249d220c703a210d1fa18bd156138a683b22319e8ef` |
| `fmp4_he.mp4` | `582e1f0e3840a002a168b96b26ba9b34fee9fadfbcee688cf398db0e7fd7b056` |
| `fmp4_he2.mp4` | `c4d5995958cc0759618e0d74eb12ae383217c6616d001c60b2b242ad36caa89a` |
| `fmp4_dash_init.m4s` | `d9f9cc69360eb47a121218094b836eb4231fa92e4fe841ce471365d8754f8d30` |
| `fmp4_dash_seg1.m4s` | `0e418d95afbf87addf09d351dbdebe886a6285fef3662c330fb3428a898c3d36` |
| `fmp4_dash_seg2.m4s` | `e2e49cabd011d266a111e730609493108ce03269b38ccc3ef26c9273a4660083` |
| `fmp4_cenc.mp4` | `ab603e1e19ebdf9094687652e5f6c3b029dfbe5e55778aba69f258eaea0bf672` |
| `fmp4_2track.mp4` | `2b09d1d58b859a3c3a53408c035203cbf9d48f1b9da5ca449936de65eb0d8711` |
| `fmp4_bbb_init.m4a` | `4846d82d3e0045ba8d23b4338fa5ce2c592d70f84444df51a126567f9e80b3a0` |
| `fmp4_bbb_seg1.m4a` | `02f822bccf4add90234e09eea08a7b8629ae2c0f3d847d2ef995a1adce51b9ec` |
| `fmp4_timeline.txt` | pinned by content: regenerated and diffed by `gen_timeline_oracle.py` |

## Provenance notes

- `fmp4_bbb_init.m4a` / `fmp4_bbb_seg1.m4a` are byte-identical to the
  Akamai-hosted GPAC-muxed BBB DASH vector (`bbb_a64k_0/1.m4a`; Big Buck
  Bunny, Blender Foundation, CC-BY) — the pins above equal the PIN.md
  pins of the fetched originals. Third-party content, independent of both
  syom and ffmpeg.
- All other fixtures wrap the deterministic lavfi synthetic stereo source
  or the in-tree HE goldens (no third-party content); see
  corpus/PROVENANCE.md §5.
- `fmp4_lc_notfdt.mp4` derives from `fmp4_lc_dbmoof.mp4` by box surgery
  (`strip_tfdt.py`): every `tfdt` removed, trun `data_offset` shrunk by
  the removed bytes. `crosscheck.py` confirms ffprobe resolves the same
  timeline by accumulation.
