use super::bytes_to_pcm16;
use crate::aac_fixture::AAC_MP4;
use crate::error::SyomError;
use crate::mp4_fixture::{patch_fourcc, write_mp4, write_pcm_mp4};
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

#[test]
fn test_bytes_to_pcm16_decodes_sowt_mp4() -> Result<(), SyomError> {
    let src = tone_s16le(160, 8_000);
    let mp4 = write_mp4(b"VV", &src)?;
    let pcm = bytes_to_pcm16(&mp4)?;
    assert_eq!(pcm.len(), src.len());
    assert!(pcm_energy(&pcm) > 1_000, "sowt decode was silent");
    Ok(())
}

#[test]
fn test_bytes_to_pcm16_resamples_48k_sowt() -> Result<(), SyomError> {
    let src = tone_s16le(48, 8_000);
    let mp4 = write_pcm_mp4(b"VV", &src, *b"sowt", 1, 16, 48_000)?;
    let pcm = bytes_to_pcm16(&mp4)?;
    assert_eq!(pcm.len(), 32);
    Ok(())
}

#[test]
fn test_bytes_to_pcm16_decodes_twos_mp4() -> Result<(), SyomError> {
    let src = tone_s16be(32, 8_000);
    let mp4 = write_pcm_mp4(b"VV", &src, *b"twos", 1, 16, 16_000)?;
    let pcm = bytes_to_pcm16(&mp4)?;
    assert_eq!(pcm.len(), 64);
    assert!(pcm_energy(&pcm) > 1_000, "twos decode was silent");
    Ok(())
}

#[test]
fn test_bytes_to_pcm16_accepts_ipcm_lpcm_raw() -> Result<(), SyomError> {
    for fcc in [*b"ipcm", *b"lpcm", *b"raw "] {
        let src = tone_s16le(16, 4_000);
        let mut mp4 = write_mp4(b"VV", &src)?;
        patch_fourcc(&mut mp4, b"sowt", &fcc)?;
        let pcm = bytes_to_pcm16(&mp4)?;
        assert_eq!(pcm.len(), src.len());
    }
    Ok(())
}

#[test]
fn test_bytes_to_pcm16_rejects_alac_mp4_as_media() -> Result<(), SyomError> {
    let mut mp4 = write_mp4(b"VV", &tone_s16le(16, 1_000))?;
    patch_fourcc(&mut mp4, b"sowt", b"alac")?;
    match bytes_to_pcm16(&mp4) {
        Err(SyomError::Media(msg)) => {
            assert!(
                msg.contains("alac"),
                "expected Media to name the fourcc, got {msg}"
            )
        }
        other => panic!("expected Media, got {other:?}"),
    }
    Ok(())
}

fn tone_s16le(frames: usize, amp: i16) -> Vec<u8> {
    let mut o = Vec::with_capacity(frames.saturating_mul(2));
    for _ in 0..frames {
        o.extend_from_slice(&amp.to_le_bytes());
    }
    o
}

fn tone_s16be(frames: usize, amp: i16) -> Vec<u8> {
    let mut o = Vec::with_capacity(frames.saturating_mul(2));
    for _ in 0..frames {
        o.extend_from_slice(&amp.to_be_bytes());
    }
    o
}

fn pcm_energy(pcm: &[u8]) -> i32 {
    pcm.chunks_exact(2)
        .map(|c| {
            let lo = c.first().copied().unwrap_or(0);
            let hi = c.get(1).copied().unwrap_or(0);
            i32::from(i16::from_le_bytes([lo, hi])).abs()
        })
        .sum()
}
