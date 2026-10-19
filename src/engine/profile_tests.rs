//! Main, LTP, and SSR decode to planar f32.
//!
//! Frames are hand-built (one codebook-1 quad in the first scalefactor band)
//! so the predictor, the LTP lag, and the SSR filterbank are actually used.
//! Main and LTP reference PCM is planar f32 from an independent float
//! decoder; the test itself does not spawn one. SSR has no such decoder
//! on this machine (FAAD was built without SSR, and libavcodec keeps the
//! LC filterbank and skips gain control), so SSR is checked by ADTS/raw
//! agreement and by leaving the LC time signal.

use super::adts::AdtsHeader;
use super::asc::{AOT_LTP, AOT_MAIN, AOT_SSR};
use super::bits::BitWriter;
use super::huff_quad::{H1_CODE, H1_LEN};
use crate::{AacError, DecodeOptions, Decoder, decode_with};

const FS: u8 = 3;
const GAIN: u32 = 140;
const FRAMES: usize = 4;

fn sf_zero(w: &mut BitWriter) {
    // Table 4.A.1 index 60 is DPCM delta 0: one zero bit.
    w.write(0, 1);
}

fn quad_1000(w: &mut BitWriter) {
    // Signed quad index 67 is (1, 0, 0, 0). One 4-bin band, one symbol.
    let idx = 67usize;
    w.write(u32::from(H1_CODE[idx]), u32::from(H1_LEN[idx]));
}

fn quad_zero(w: &mut BitWriter) {
    // Index 40 is (0, 0, 0, 0), a single zero bit.
    w.write(u32::from(H1_CODE[40]), u32::from(H1_LEN[40]));
}

/// Tone in bins 256..260 (SSR band 1) so gain control is not a no-op.
fn raw_band1(gain: bool) -> Vec<u8> {
    let mut w = BitWriter::new();
    w.write(0, 3);
    w.write(0, 4);
    w.write(GAIN, 8);
    w.write_bit(false);
    w.write(0, 2);
    w.write_bit(false);
    w.write(27, 6);
    w.write_bit(false);
    w.write(1, 4);
    w.write(27, 5);
    for _ in 0..27 {
        sf_zero(&mut w);
    }
    w.write_bit(false);
    w.write_bit(false);
    if gain {
        w.write_bit(true);
        w.write(1, 2);
        w.write(1, 3);
        w.write(8, 4); // 2^(8/4) = 4
        w.write(0, 5);
    } else {
        w.write_bit(false);
    }
    for _ in 0..64 {
        quad_zero(&mut w);
    }
    quad_1000(&mut w);
    quad_zero(&mut w);
    w.write(7, 3);
    w.finish()
}

fn raw_frame(aot: u8, predict: bool, ltp_lag: u16, ltp_coef: u8, gain: bool) -> Vec<u8> {
    let mut w = BitWriter::new();
    w.write(0, 3); // SCE
    w.write(0, 4); // tag
    w.write(GAIN, 8);
    w.write_bit(false); // ics reserved
    w.write(0, 2); // only long
    w.write_bit(false); // sine
    w.write(1, 6); // max_sfb
    if predict {
        w.write_bit(true);
        if aot == AOT_MAIN {
            w.write_bit(false); // no reset
            w.write_bit(true); // prediction_used[0]
        } else if aot == AOT_LTP {
            w.write_bit(true); // ltp_data_present
            w.write(u32::from(ltp_lag), 11);
            w.write(u32::from(ltp_coef), 3);
            w.write_bit(true); // ltp_long_used[0]
        }
    } else {
        w.write_bit(false);
    }
    w.write(1, 4); // codebook 1
    w.write(1, 5); // one band
    sf_zero(&mut w);
    w.write_bit(false); // pulse
    w.write_bit(false); // tns
    if gain {
        w.write_bit(true);
        w.write(1, 2); // max_band: band 1 only
        w.write(1, 3); // one adjustment
        w.write(4, 4); // alev = 4 → 2^(4/4) = 2
        w.write(8, 5); // aloc 8 → sample 128 on a 16-sample grid
    } else {
        w.write_bit(false);
    }
    quad_1000(&mut w);
    w.write(7, 3); // END
    w.finish()
}

fn adts(profile: u8, payload: &[u8]) -> Vec<u8> {
    let header = AdtsHeader {
        mpeg_version_mpeg2: false,
        protection_absent: true,
        profile,
        sampling_frequency_index: FS,
        channel_configuration: 1,
        aac_frame_length: (7 + payload.len()) as u16,
        adts_buffer_fullness: 0x7FF,
        number_of_raw_data_blocks_in_frame: 1,
    };
    let mut out = header.write().to_vec();
    out.extend_from_slice(payload);
    out
}

fn stream(profile: u8, aot: u8, predict: bool, lag: u16, coef: u8, gain: bool) -> Vec<u8> {
    let raw = raw_frame(aot, predict, lag, coef, gain);
    let mut out = Vec::new();
    for _ in 0..FRAMES {
        out.extend(adts(profile, &raw));
    }
    out
}

fn asc(aot: u8) -> Vec<u8> {
    let mut w = BitWriter::new();
    w.write(u32::from(aot), 5);
    w.write(u32::from(FS), 4);
    w.write(1, 4);
    w.write(0, 3);
    w.finish()
}

fn decode_adts(bytes: &[u8]) -> Result<crate::DecodedAac, AacError> {
    decode_with(bytes, &DecodeOptions::unbounded())
}

fn gold_max_abs(got: &[f32], gold: &[u8]) -> f32 {
    assert_eq!(gold.len(), got.len() * 4);
    let mut max = 0.0f32;
    for (i, &sample) in got.iter().enumerate() {
        let mut buf = [0u8; 4];
        buf.copy_from_slice(&gold[i * 4..i * 4 + 4]);
        max = max.max((sample - f32::from_le_bytes(buf)).abs());
    }
    max
}

fn decode_au(aot: u8, raw: &[u8]) -> Result<Vec<f32>, AacError> {
    let mut dec = Decoder::from_asc(&asc(aot), DecodeOptions::unbounded())?;
    let mut pcm = Vec::new();
    for _ in 0..FRAMES {
        dec.decode_au(raw, |f| {
            pcm.extend_from_slice(f.planar[0]);
            Ok(())
        })?;
    }
    dec.finish(|_| Ok(()))?;
    Ok(pcm)
}

#[test]
fn main_prediction_changes_later_frames_and_matches_raw_au() -> Result<(), AacError> {
    let off = stream(0, AOT_MAIN, false, 0, 0, false);
    let on = stream(0, AOT_MAIN, true, 0, 0, false);
    let a = decode_adts(&off)?;
    let b = decode_adts(&on)?;
    assert_eq!(a.sample_rate, 48_000);
    assert_eq!(a.channels.len(), 1);
    assert_eq!(a.channels[0].len(), FRAMES * 1024);
    assert_eq!(b.channels[0].len(), FRAMES * 1024);
    // The first frame's prediction is zero; later frames must move.
    assert_eq!(&a.channels[0][..1024], &b.channels[0][..1024]);
    assert_ne!(a.channels[0], b.channels[0]);
    let raw = raw_frame(AOT_MAIN, true, 0, 0, false);
    assert_eq!(decode_au(AOT_MAIN, &raw)?, b.channels[0]);
    let err = gold_max_abs(&b.channels[0], include_bytes!("goldens/profile_main.f32"));
    assert!(err < 1.0e-9, "main reference max abs {err}");
    Ok(())
}

#[test]
fn ltp_header_bits_land_on_the_ics() -> super::error::Result<()> {
    let raw = raw_frame(AOT_LTP, true, 512, 4, false);
    let mut br = super::bits::BitReader::new(&raw);
    let _ = br.read(3)?;
    let _ = br.read(4)?;
    let _ = br.read(8)?;
    let ics = super::ics::IcsInfo::parse_mode(&mut br, FS, false, AOT_LTP, 0)?;
    assert!(ics.predictor_present);
    assert!(ics.ltp_present);
    assert_eq!(ics.ltp_lag, 512);
    assert_eq!(ics.ltp_coef_idx, 4);
    assert_eq!(ics.ltp_used & 1, 1);
    Ok(())
}

#[test]
fn ltp_and_ssr_roundtrip_adts_and_raw_au() -> Result<(), AacError> {
    let ltp = stream(3, AOT_LTP, true, 512, 4, false);
    let ltp_pcm = decode_adts(&ltp)?;
    assert_eq!(ltp_pcm.channels[0].len(), FRAMES * 1024);
    let raw = raw_frame(AOT_LTP, true, 512, 4, false);
    assert_eq!(decode_au(AOT_LTP, &raw)?, ltp_pcm.channels[0]);
    // Lag 512 points at the previous frame, so a later frame leaves LC.
    let ltp_off = stream(3, AOT_LTP, false, 0, 0, false);
    let off = decode_adts(&ltp_off)?;
    assert_ne!(off.channels[0], ltp_pcm.channels[0]);
    let err = gold_max_abs(
        &ltp_pcm.channels[0],
        include_bytes!("goldens/profile_ltp.f32"),
    );
    assert!(err < 1.0e-9, "ltp reference max abs {err}");

    let ssr = stream(2, AOT_SSR, false, 0, 0, false);
    let ssr_pcm = decode_adts(&ssr)?;
    assert_eq!(ssr_pcm.channels[0].len(), FRAMES * 1024);
    let ssr_raw = raw_frame(AOT_SSR, false, 0, 0, false);
    assert_eq!(decode_au(AOT_SSR, &ssr_raw)?, ssr_pcm.channels[0]);
    // Same spectral line through the LC filterbank is a different signal.
    assert_ne!(off.channels[0], ssr_pcm.channels[0]);

    // Band 0 is never gain-controlled. A tone in band 1 must move.
    let plain = adts(2, &raw_band1(false));
    let boosted = adts(2, &raw_band1(true));
    let mut plain_s = Vec::new();
    let mut boost_s = Vec::new();
    for _ in 0..FRAMES {
        plain_s.extend(plain.iter().copied());
        boost_s.extend(boosted.iter().copied());
    }
    let plain_pcm = decode_adts(&plain_s)?;
    let boost_pcm = decode_adts(&boost_s)?;
    assert_ne!(plain_pcm.channels[0], boost_pcm.channels[0]);
    assert_eq!(decode_au(AOT_SSR, &raw_band1(true))?, boost_pcm.channels[0]);
    Ok(())
}
