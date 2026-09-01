use super::{decode_wav, encode_16k_mono_s16, sniff_wav};
use crate::error::SyomError;

#[test]
fn test_sniff_wav_rejects_short() {
    assert!(!sniff_wav(b"RIFF"));
    assert!(!sniff_wav(b"not wav at all!!"));
}

#[test]
fn test_decode_wav_rejects_non_pcm() {
    assert!(decode_wav(b"RIFF....WAVE").is_err());
}

#[test]
fn test_encode_has_riff_wave_header() -> Result<(), SyomError> {
    let wav = encode_16k_mono_s16(&[0, 0, 1, 0]);
    assert!(sniff_wav(&wav));
    assert_eq!(wav.get(8..12), Some(&b"WAVE"[..]));
    Ok(())
}
