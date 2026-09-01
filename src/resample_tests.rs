use super::{Pcm16Mono, to_pcm16_mono_16k};
use crate::error::SyomError;

#[test]
fn test_resample_rejects_zero_rate_or_channels() {
    assert!(to_pcm16_mono_16k(&[0.1, 0.2], 0, 16_000).is_err());
    assert!(to_pcm16_mono_16k(&[0.1, 0.2], 1, 0).is_err());
}

#[test]
fn test_resample_rejects_empty_frames() {
    assert!(to_pcm16_mono_16k(&[], 1, 16_000).is_err());
}

#[test]
fn test_resample_16k_mono_keeps_sample_count() -> Result<(), SyomError> {
    let n = 160;
    let src = vec![0.5f32; n];
    let pcm = to_pcm16_mono_16k(&src, 1, 16_000)?;
    assert_eq!(pcm.len(), n.saturating_mul(2));
    Ok(())
}

#[test]
fn test_resample_48k_stereo_to_16k_mono_is_shorter() -> Result<(), SyomError> {
    let mut src = Vec::new();
    for i in 0..48 {
        let v = (i as f32) / 48.0;
        src.push(v);
        src.push(-v);
    }
    let pcm = to_pcm16_mono_16k(&src, 2, 48_000)?;
    assert!(
        pcm.len() >= 24 && pcm.len() <= 48,
        "got {} bytes",
        pcm.len()
    );
    Ok(())
}

#[test]
fn test_pcm16_mono_new_ok() -> Result<(), SyomError> {
    let _ = Pcm16Mono::new(1, 16_000)?;
    Ok(())
}
