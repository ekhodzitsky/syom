# syom-aac

AAC-LC for [syom](https://github.com/ekhodzitsky/syom) extract.
Product path: MP4 `mp4a` / ADTS → native-rate planar f32. `rustfft` for
IMDCT with owned pre/post twiddles. HE-AAC SBR/PS is Media.

Demux: ISO-BMFF `mp4a` + `esds` AudioSpecificConfig.

Comments under `src/engine/` cite **ISO/IEC 14496-3** (MPEG-4 Audio)
section numbers (`§4.6.x`). Typical instructional MP4s are AAC-LC.
