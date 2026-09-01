use super::bytes_to_pcm16;
use crate::aac_fixture::AAC_MP4;
use crate::error::SyomError;
use crate::wav;

#[test]
fn test_bytes_to_pcm16_rejects_garbage() {
    assert!(bytes_to_pcm16(b"not audio").is_err());
}

#[test]
fn test_bytes_to_pcm16_decodes_aac_mp4_fixture() -> Result<(), SyomError> {
    let pcm = bytes_to_pcm16(AAC_MP4)?;
    assert!(
        pcm.len() >= 4_000 && pcm.len() <= 16_000,
        "expected ~0.25 s of 16 kHz s16le, got {} bytes",
        pcm.len()
    );
    let energy: i32 = pcm
        .chunks_exact(2)
        .map(|c| {
            let lo = c.first().copied().unwrap_or(0);
            let hi = c.get(1).copied().unwrap_or(0);
            i32::from(i16::from_le_bytes([lo, hi])).abs()
        })
        .sum();
    assert!(energy > 1_000, "decoded AAC was silent, energy {energy}");
    Ok(())
}

#[test]
fn test_wav_round_trip_16k_mono() -> Result<(), SyomError> {
    let pcm = vec![0u8, 64, 0, 128];
    let wav = wav::encode_16k_mono_s16(&pcm);
    let got = wav::decode_wav(&wav)?;
    assert_eq!(got.len(), pcm.len());
    Ok(())
}
