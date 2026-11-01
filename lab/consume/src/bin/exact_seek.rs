//! Exact seek integration against the packaged crate's public API.
use std::io::Cursor;
use syom::{AacError, DecodeOptions, EncodeOptions, ExactSeekHistory, M4aSeek};

fn main() -> Result<(), AacError> {
    let pcm: Vec<_> = (0..48_000).map(|i| 0.2 * (i as f32 * 0.13).sin()).collect();
    let bytes = syom::encode_with(&[&pcm], 48_000, &EncodeOptions::m4a())?;
    let reference = syom::decode_with(&bytes, &DecodeOptions::audio())?;
    let mut source = M4aSeek::open(Cursor::new(&bytes), DecodeOptions::audio())?;
    let start = 24_017;
    let plan = source.exact_seek_plan(start, 48_000)?;
    assert_eq!(plan.history(), ExactSeekHistory::PrefixReplay);
    assert_eq!(plan.access_units().start, 0);
    let replay = plan.replay_access_units();
    let payload_bytes: u64 = plan.media_ranges().map(|r| r.end - r.start).sum();
    assert!(matches!(
        source.seek_exact(start, 0),
        Err(AacError::SeekReplayRequired { .. })
    ));
    assert_eq!(source.position(), 0);
    source.seek_exact(start, replay)?;
    let position = source.save_exact_position()?;
    let mut at = start as usize;
    source.decode(|frame| {
        for (got, want) in frame.planar[0].iter().zip(&reference.channels[0][at..]) {
            assert_eq!(got.to_bits(), want.to_bits());
        }
        at += frame.samples;
        Ok(())
    })?;
    assert_eq!(at, reference.channels[0].len());
    source.restore_exact_position(&position, replay)?;
    assert_eq!(source.position(), start as u64);
    let mut foreign = M4aSeek::open(Cursor::new(&bytes), DecodeOptions::audio())?;
    assert!(matches!(
        foreign.restore_exact_position(&position, replay),
        Err(AacError::ForeignSeekPosition)
    ));
    println!("cell exact-seek replay_aus={replay} payload_bytes={payload_bytes} native_differences=0 foreign_position=rejected budget=pre_io");
    Ok(())
}
