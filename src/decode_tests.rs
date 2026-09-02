//! Public decode path: sniff, caps, lavc native goldens.

use super::{
    AacError, ChannelMode, DecodeOptions, decode, decode_bytes, read, sniff_aac, sniff_is_adts,
    sniff_is_latm,
};

fn assert_native_matches_lavc(
    input: &[u8],
    gold: &[u8],
    rate: u32,
    label: &str,
) -> Result<(), AacError> {
    assert_native_matches_lavc_with(input, gold, rate, label, &DecodeOptions::speech())
}

fn deinterleave_s16(gold: &[u8], n_ch: usize) -> Vec<Vec<i16>> {
    let mut planes = vec![Vec::new(); n_ch.max(1)];
    for (i, chunk) in gold.chunks_exact(2).enumerate() {
        planes[i % n_ch].push(i16::from_le_bytes([chunk[0], chunk[1]]));
    }
    planes
}

fn score_plane(ours: &[f32], gold: &[i16], label: &str) {
    assert_eq!(ours.len(), gold.len(), "{label} plane length");
    let mut max_lsb = 0u32;
    let mut ps = 0.0f64;
    let mut pe = 0.0f64;
    let mut peak = 0u16;
    for (i, &gv) in gold.iter().enumerate() {
        peak = peak.max(gv.unsigned_abs());
        let ov = (f64::from(ours.get(i).copied().unwrap_or(0.0)) * 32768.0)
            .round()
            .clamp(-32768.0, 32767.0) as i16;
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
}

fn assert_native_matches_lavc_with(
    input: &[u8],
    gold: &[u8],
    rate: u32,
    label: &str,
    opts: &DecodeOptions,
) -> Result<(), AacError> {
    let decoded = crate::decode_with(input, opts)?;
    assert_eq!(decoded.sample_rate, rate, "{label} sample rate");
    let n = decoded
        .channels
        .first()
        .map(Vec::len)
        .ok_or_else(|| AacError::decode("aac empty"))?;
    assert!(n > 0, "{label} empty pcm");
    assert_eq!(gold.len() % 2, 0, "{label} odd golden");
    let gold_i16 = gold.len() / 2;
    assert_eq!(gold_i16 % n, 0, "{label} golden not a multiple of {n}");
    let gold_ch = gold_i16 / n;
    assert!(
        gold_ch == 1 || gold_ch == 2,
        "{label} golden channels {gold_ch}"
    );
    match opts.channel_mode {
        ChannelMode::Split => assert_eq!(
            decoded.channels.len(),
            gold_ch,
            "{label} split channels {} vs golden {gold_ch} (missing PS?)",
            decoded.channels.len()
        ),
        ChannelMode::Mono => assert_eq!(decoded.channels.len(), 1, "{label} speech not mono"),
    }
    let planes = deinterleave_s16(gold, gold_ch);
    if matches!(opts.channel_mode, ChannelMode::Mono) && gold_ch == 2 {
        let mix: Vec<i16> = planes[0]
            .iter()
            .zip(&planes[1])
            .map(|(l, r)| ((i32::from(*l) + i32::from(*r)) / 2) as i16)
            .collect();
        score_plane(&decoded.channels[0], &mix, label);
    } else {
        for (i, plane) in planes.iter().enumerate() {
            score_plane(&decoded.channels[i], plane, &format!("{label} ch{i}"));
        }
    }
    Ok(())
}

#[test]
fn sniff_rejects_short_and_mp3ish() {
    assert!(!sniff_is_adts(&[]));
    assert!(!sniff_is_adts(&[0xFF, 0xFB, 0, 0, 0, 0]));
    assert!(!sniff_is_adts(&[0xFF, 0xF1, 0x3C, 0, 0, 0]));
    assert!(!sniff_is_latm(&[]));
    assert!(!sniff_aac(b"hello"));
}

#[test]
fn empty_decode_is_not_aac() {
    assert!(matches!(decode(&[]), Err(AacError::NotAac)));
    assert!(matches!(decode(b"hello"), Err(AacError::NotAac)));
}

#[test]
fn decode_bytes_matches_decode() -> Result<(), AacError> {
    let adts = include_bytes!("goldens/sine48.adts");
    let a = decode(adts)?;
    let b = decode_bytes(adts)?;
    assert_eq!(a.sample_rate, b.sample_rate);
    assert_eq!(a.channels, b.channels);
    Ok(())
}

#[test]
fn lecture_m4a_native_matches_lavc_golden() -> Result<(), AacError> {
    assert_native_matches_lavc(
        include_bytes!("goldens/lecture.m4a"),
        include_bytes!("goldens/aac_mp4_48k_mono.s16"),
        48_000,
        "lecture",
    )
}

#[test]
fn sine441_m4a_native_matches_lavc_golden() -> Result<(), AacError> {
    assert_native_matches_lavc(
        include_bytes!("goldens/sine441.m4a"),
        include_bytes!("goldens/sine441_44k.s16"),
        44_100,
        "sine441",
    )
}

#[test]
fn sine48_adts_native_matches_lavc_golden() -> Result<(), AacError> {
    assert_native_matches_lavc(
        include_bytes!("goldens/sine48.adts"),
        include_bytes!("goldens/sine48_48k.s16"),
        48_000,
        "sine48",
    )
}

#[test]
fn tns48_adts_native_matches_lavc_golden() -> Result<(), AacError> {
    assert_native_matches_lavc(
        include_bytes!("goldens/tns48.adts"),
        include_bytes!("goldens/tns48_48k.s16"),
        48_000,
        "tns48",
    )
}

#[test]
fn pns48_adts_native_matches_lavc_golden() -> Result<(), AacError> {
    assert_native_matches_lavc(
        include_bytes!("goldens/pns48.adts"),
        include_bytes!("goldens/pns48.s16"),
        48_000,
        "pns48",
    )
}

#[test]
fn latm48_native_matches_lavc_golden() -> Result<(), AacError> {
    let latm = include_bytes!("goldens/latm48.latm");
    assert!(sniff_is_latm(latm));
    assert_native_matches_lavc(latm, include_bytes!("goldens/latm48.s16"), 48_000, "latm48")
}

#[test]
fn he_sbr_upsample_is_not_media() -> Result<(), AacError> {
    // HE-AAC decode path: SBR upsampler is wired; LC ADTS still decodes
    // (implicit SBR may be absent). AOT-5 ASC must parse as LC core+SBR.
    use crate::engine::asc::AudioSpecificConfig;
    use crate::engine::bits::BitWriter;
    let mut w = BitWriter::new();
    w.write(5, 5);
    w.write(3, 4);
    w.write(1, 4);
    w.write(2, 5);
    w.write(0, 3);
    let (asc, _) =
        AudioSpecificConfig::parse(&w.finish()).map_err(|e| AacError::decode(format!("{e:?}")))?;
    assert!(asc.sbr_present);
    assert_eq!(asc.aot, 2);
    Ok(())
}

#[test]
fn he48_first_frame_sbr_rate() -> Result<(), AacError> {
    use crate::engine::adts::AdtsHeader;
    use crate::engine::decode::StreamDecoder;
    let he = include_bytes!("goldens/he48.adts");
    let (hdr, off) = AdtsHeader::parse(he).map_err(|e| AacError::decode(format!("{e:?}")))?;
    let fl = usize::from(hdr.aac_frame_length);
    let payload = &he[off..fl];
    let mut dec = StreamDecoder::new();
    let frame = dec
        .decode_frame(&hdr, payload)
        .map_err(|e| AacError::decode(format!("{e:?}")))?;
    assert_eq!(frame.sample_rate, 48_000, "decode_frame he first rate");
    let mut dec = StreamDecoder::new();
    let mut pcm = Vec::new();
    let rate = dec
        .decode_raw_mono_f32(
            hdr.audio_object_type(),
            hdr.sampling_frequency_index,
            hdr.sample_rate(),
            hdr.channel_configuration,
            1,
            payload,
            &mut pcm,
        )
        .map_err(|e| AacError::decode(format!("{e:?}")))?;
    assert_eq!(rate, 48_000, "mono_f32 he first rate n={}", pcm.len());
    Ok(())
}

#[test]
fn he48_speech_decode_len() -> Result<(), AacError> {
    assert_native_matches_lavc(
        include_bytes!("goldens/he48.adts"),
        include_bytes!("goldens/he48.s16"),
        48_000,
        "he48-adts-speech",
    )?;
    assert_native_matches_lavc(
        include_bytes!("goldens/he48.m4a"),
        include_bytes!("goldens/he48_m4a.s16"),
        48_000,
        "he48-m4a-speech",
    )
}

#[test]
fn he48_adts_unbounded_is_ps_stereo() -> Result<(), AacError> {
    let d = crate::decode_with(
        include_bytes!("goldens/he48.adts"),
        &DecodeOptions::unbounded(),
    )?;
    assert_eq!(d.sample_rate, 48_000);
    assert_eq!(
        d.channels.len(),
        2,
        "HE-AACv2 Split must emit PS stereo, got {} ch n={}",
        d.channels.len(),
        d.channels.first().map(Vec::len).unwrap_or(0)
    );
    assert_eq!(d.channels[0].len(), d.channels[1].len());
    Ok(())
}

#[test]
fn he48_adts_native_matches_lavc_golden() -> Result<(), AacError> {
    assert_native_matches_lavc_with(
        include_bytes!("goldens/he48.adts"),
        include_bytes!("goldens/he48.s16"),
        48_000,
        "he48-adts",
        &DecodeOptions::unbounded(),
    )
}

#[test]
fn he48_m4a_native_matches_lavc_golden() -> Result<(), AacError> {
    assert_native_matches_lavc_with(
        include_bytes!("goldens/he48.m4a"),
        include_bytes!("goldens/he48_m4a.s16"),
        48_000,
        "he48-m4a",
        &DecodeOptions::unbounded(),
    )
}

#[test]
fn speech_caps_reject_long_duration() {
    let o = DecodeOptions::speech().with_max_duration_secs(0.01);
    let adts = include_bytes!("goldens/sine48.adts");
    match crate::decode_with(adts, &o) {
        Err(AacError::TooLong { .. }) => {}
        other => panic!("expected too-long, got {other:?}"),
    }
}

#[test]
fn speech_caps_reject_high_rate() {
    let o = DecodeOptions::speech().with_max_sample_rate(8_000);
    let adts = include_bytes!("goldens/sine48.adts");
    match crate::decode_with(adts, &o) {
        Err(AacError::UnsupportedSampleRate {
            rate: 48_000,
            max: 8_000,
        }) => {}
        other => panic!("expected sample-rate reject, got {other:?}"),
    }
}

#[test]
fn unbounded_keeps_split() {
    let o = DecodeOptions::unbounded();
    assert_eq!(o.channel_mode, ChannelMode::Split);
}

#[test]
fn split_sine48_has_one_channel() -> Result<(), AacError> {
    let adts = include_bytes!("goldens/sine48.adts");
    let d = crate::decode_with(adts, &DecodeOptions::unbounded())?;
    assert_eq!(d.channels.len(), 1);
    assert_eq!(d.sample_rate, 48_000);
    assert!(d.channels[0].iter().any(|s| s.abs() > 1e-4));
    Ok(())
}

#[test]
fn lecture_unbounded_is_stereo_split() -> Result<(), AacError> {
    let m4a = include_bytes!("goldens/lecture.m4a");
    let d = crate::decode_with(m4a, &DecodeOptions::unbounded())?;
    assert_eq!(d.sample_rate, 48_000);
    assert_eq!(d.channels.len(), 2);
    assert_eq!(d.channels[0].len(), d.channels[1].len());
    assert!(d.channels[0].iter().any(|s| s.abs() > 1e-4));
    Ok(())
}

#[test]
fn read_with_speech_matches_decode() -> Result<(), AacError> {
    let bytes = include_bytes!("goldens/sine48.adts");
    let dir = std::env::temp_dir().join(format!("syom-readw-{}.adts", std::process::id()));
    std::fs::write(&dir, bytes)?;
    let a = crate::read_with(&dir, &DecodeOptions::speech())?;
    let _ = std::fs::remove_file(&dir);
    let b = decode(bytes)?;
    assert_eq!(a.sample_rate, b.sample_rate);
    assert_eq!(a.channels.len(), b.channels.len());
    Ok(())
}

#[test]
fn bitreader_wide_and_signed_helpers() -> Result<(), AacError> {
    use crate::engine::bits::BitReader;
    let bytes = [0xA5, 0x5A, 0xFF, 0x00, 0x80, 0x11];
    let e = |err| AacError::decode(format!("{err:?}"));
    let mut br = BitReader::with_position(&bytes, 0);
    assert_eq!(br.read_u1().map_err(e)?, 1);
    assert!(br.peek_u32(7).map_err(e)? > 0);
    let _ = br.read_u32(7).map_err(e)?;
    let mut br = BitReader::new(&bytes);
    let _ = br.read_i32(16).map_err(e)?;
    let mut br = BitReader::new(&bytes);
    let _ = br.read_u64(40).map_err(e)?;
    let mut br = BitReader::new(&bytes);
    br.consume(3).map_err(e)?;
    br.align_to_byte().map_err(e)?;
    assert!(br.is_byte_aligned());
    assert_eq!(br.byte_position(), 1);
    Ok(())
}

#[test]
fn read_round_trip_tmp() -> Result<(), AacError> {
    let bytes = include_bytes!("goldens/tns48.adts");
    let dir = std::env::temp_dir().join(format!("syom-read-{}.adts", std::process::id()));
    std::fs::write(&dir, bytes)?;
    let a = read(&dir)?;
    let _ = std::fs::remove_file(&dir);
    assert!(!a.channels.is_empty());
    assert!(a.channels[0].iter().any(|s| s.abs() > 1e-4));
    Ok(())
}
