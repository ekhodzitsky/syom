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
    let track = syom_aac::parse_aac_track(AAC_MP4).map_err(SyomError::from)?;
    assert_eq!(track.skip_samples(48_000), 1024);
    let pcm = bytes_to_pcm16(AAC_MP4)?;
    // Audio elst.media_time=1024 @ 48 kHz: drop the encoder-delay frame,
    // then 12×1024 → 4096 samples @ 16 kHz = 8192 bytes.
    assert_eq!(
        pcm.len(),
        8192,
        "expected encoder-delay skip (8192 bytes @ 16 kHz), got {}",
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

fn assert_native_matches_lavc(
    input: &[u8],
    gold: &[u8],
    rate: u32,
    label: &str,
) -> Result<(), SyomError> {
    let decoded = syom_aac::decode(input).map_err(SyomError::from)?;
    assert_eq!(decoded.sample_rate, rate, "{label} sample rate");
    let ch = decoded
        .channels
        .first()
        .ok_or_else(|| crate::error::media("aac empty"))?;
    assert_eq!(
        ch.len().saturating_mul(2),
        gold.len(),
        "{label} native length {} vs golden {}",
        ch.len().saturating_mul(2),
        gold.len()
    );
    let mut max_lsb = 0u32;
    let mut ps = 0.0f64;
    let mut pe = 0.0f64;
    let mut peak = 0u16;
    for (i, chunk) in gold.chunks_exact(2).enumerate() {
        let gv = i16::from_le_bytes([chunk[0], chunk[1]]);
        peak = peak.max(gv.unsigned_abs());
        let sample = ch.get(i).copied().unwrap_or(0.0);
        let ov = (sample * 32768.0).round().clamp(-32768.0, 32767.0) as i16;
        max_lsb = max_lsb.max((i32::from(gv) - i32::from(ov)).unsigned_abs());
        let gs = f64::from(gv);
        ps += gs * gs;
        let e = gs - f64::from(ov);
        pe += e * e;
    }
    assert!(peak >= 1000, "{label} lavc golden peak {peak} is inaudible");
    assert!(max_lsb <= 1, "{label} native max lsb {max_lsb}");
    let snr = if pe == 0.0 {
        200.0
    } else {
        10.0 * (ps / pe).log10()
    };
    assert!(snr >= 70.0, "{label} native SNR {snr} dB");
    Ok(())
}

#[test]
fn test_aac_mp4_native_matches_lavc_golden() -> Result<(), SyomError> {
    // Minted once offline with ffmpeg 8.1.1 native AAC:
    // `-vn -ac 1 -ar 48000 -f s16le`. Runtime does not shell to ffmpeg.
    assert_native_matches_lavc(
        AAC_MP4,
        include_bytes!("goldens/aac_mp4_48k_mono.s16"),
        48_000,
        "AAC_MP4",
    )
}

#[test]
fn test_sine441_m4a_native_matches_lavc_golden() -> Result<(), SyomError> {
    let m4a = include_bytes!("goldens/sine441.m4a");
    let track = syom_aac::parse_aac_track(m4a).map_err(SyomError::from)?;
    assert_eq!(track.skip_samples(44_100), 1024);
    let pcm = bytes_to_pcm16(m4a)?;
    assert!(pcm_energy(&pcm) > 1_000, "sine441 product was silent");
    assert_native_matches_lavc(
        m4a,
        include_bytes!("goldens/sine441_44k.s16"),
        44_100,
        "sine441",
    )
}

#[test]
fn test_sine48_adts_native_matches_lavc_golden() -> Result<(), SyomError> {
    let adts = include_bytes!("goldens/sine48.adts");
    let pcm = bytes_to_pcm16(adts)?;
    assert!(pcm_energy(&pcm) > 1_000, "sine48 product was silent");
    assert_native_matches_lavc(
        adts,
        include_bytes!("goldens/sine48_48k.s16"),
        48_000,
        "sine48",
    )
}

#[test]
fn test_tns48_adts_native_matches_lavc_golden() -> Result<(), SyomError> {
    let adts = include_bytes!("goldens/tns48.adts");
    let pcm = bytes_to_pcm16(adts)?;
    assert!(pcm_energy(&pcm) > 1_000, "tns48 product was silent");
    assert_native_matches_lavc(
        adts,
        include_bytes!("goldens/tns48_48k.s16"),
        48_000,
        "tns48",
    )
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
fn test_bytes_to_pcm16_rejects_heaac_aot5_as_media() -> Result<(), SyomError> {
    let mut bytes = AAC_MP4.to_vec();
    let needle = [0x05u8, 0x80, 0x80, 0x80, 0x05, 0x11, 0x90];
    let pos = bytes
        .windows(needle.len())
        .position(|w| w == needle)
        .ok_or_else(|| crate::error::media("fixture missing DecoderSpecificInfo"))?;
    let asc = pos + 5;
    let slot = bytes
        .get_mut(asc)
        .ok_or_else(|| crate::error::media("fixture ASC"))?;
    *slot = 0x29; // AOT 5, same leftover bits as 0x11
    match bytes_to_pcm16(&bytes) {
        Err(SyomError::Media(_)) => Ok(()),
        other => panic!("expected Media for HE-AAC AOT 5, got {other:?}"),
    }
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
