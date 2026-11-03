//! Packaged-crate native LC resume contract; synthetic, no recordings/oracles.
use std::io::Cursor;
use syom::{AacError, DecodeOptions, EncodeOptions, M4aSeek, PresentationCompletion};

fn main() -> syom::Result<()> {
    let mut random = 144u32;
    let pcm: Vec<f32> = (0..44_113)
        .map(|_| {
            random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            ((random >> 8) as f32 / 16_777_216.0 - 0.5) * 0.4
        })
        .collect();
    let encoded = syom::encode_with(&[&pcm, &pcm], 44_100, &EncodeOptions::m4a().with_pns(true))?;
    let open = || M4aSeek::open(Cursor::new(&encoded), DecodeOptions::speech());
    let mut reference = Vec::new();
    let info = open()?.decode(|frame| {
        reference.extend(frame.planar[0].iter().map(|v| v.to_bits()));
        Ok(())
    })?;
    assert_eq!(
        info.presentation_completion,
        PresentationCompletion::Complete
    );
    let mut prefix = Vec::new();
    let mut interrupted = open()?;
    let result = interrupted.decode(|frame| {
        prefix.extend(frame.planar[0].iter().map(|v| v.to_bits()));
        Err(AacError::decode("intentional durable boundary"))
    });
    assert!(result.is_err());
    let checkpoint = interrupted.checkpoint().expect("qualified LC state");
    assert!(checkpoint.len() < 16 * 1024);
    // The consumer binds this state to these same immutable source bytes,
    // options, packaged implementation and execution platform.
    let mut restored = open()?;
    restored.restore_checkpoint(&checkpoint)?;
    let tail = restored.decode(|frame| {
        prefix.extend(frame.planar[0].iter().map(|v| v.to_bits()));
        Ok(())
    })?;
    assert_eq!(prefix, reference);
    assert!(restored.presentation_complete());
    assert_eq!(
        tail.presentation_completion,
        PresentationCompletion::Complete
    );
    println!(
        "checkpoint packaged consumer: {} native samples, {} state bytes, bit-identical resume",
        prefix.len(),
        checkpoint.len()
    );
    Ok(())
}
