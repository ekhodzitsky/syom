//! Native-rate sine / noise LC goldens vs the shipped decoder.
//!
//! Expected PCM is the naive §4.6.11 IMDCT + sine window (first-frame
//! overlap with zeros). Runtime does not shell to ffmpeg.

use super::adts::{ADTS_HEADER_BYTES_NO_CRC, AdtsHeader};
use super::bits::BitWriter;
use super::decode::StreamDecoder;
use super::error::Error;
use super::huff_quad::{H1_CODE, H1_LEN};
use super::imdct::imdct_naive;
use super::spectrum::invquant;

const FRAME: usize = 1024;

fn adts_wrap(payload: &[u8], ch: u8, fs_index: u8) -> Vec<u8> {
    let frame_len = ADTS_HEADER_BYTES_NO_CRC + payload.len();
    let mut w = BitWriter::new();
    w.write(0xFFF, 12);
    w.write_bit(false);
    w.write(0, 2);
    w.write_bit(true);
    w.write(1, 2); // LC
    w.write(u32::from(fs_index), 4);
    w.write_bit(false);
    w.write(u32::from(ch), 3);
    w.write(0, 4);
    w.write(frame_len as u32, 13);
    w.write(0x7FF, 11);
    w.write(0, 2);
    let mut out = w.finish();
    out.extend_from_slice(payload);
    out
}

fn sce_book1(indices: &[usize], max_sfb: u8) -> Vec<u8> {
    let mut w = BitWriter::new();
    w.write(0, 3);
    w.write(0, 4);
    w.write(100, 8);
    w.write_bit(false);
    w.write(0, 2);
    w.write_bit(false);
    w.write(u32::from(max_sfb), 6);
    w.write_bit(false);
    w.write(1, 4); // book 1
    w.write(u32::from(max_sfb), 5);
    for _ in 0..max_sfb {
        w.write(0, 1); // sf dpcm 0 per band (Table 4.A.1 index 60)
    }
    w.write_bit(false);
    w.write_bit(false);
    w.write_bit(false);
    for &idx in indices {
        w.write(u32::from(H1_CODE[idx]), u32::from(H1_LEN[idx]));
    }
    w.write(7, 3);
    w.finish()
}

fn snr_db(signal: &[f64], other: &[f64]) -> f64 {
    let mut ps = 0.0f64;
    let mut pe = 0.0f64;
    for (&s, &o) in signal.iter().zip(other.iter()) {
        ps += s * s;
        let e = s - o;
        pe += e * e;
    }
    if pe == 0.0 {
        return 200.0;
    }
    10.0 * (ps / pe).log10()
}

fn max_abs(a: &[f64], b: &[f64]) -> f64 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).abs())
        .fold(0.0, f64::max)
}

fn naive_sine_window_long(spec: &[f64]) -> Vec<f64> {
    let x = imdct_naive(spec, 2048);
    let n = 2048.0;
    (0..FRAME)
        .map(|i| {
            let w = (std::f64::consts::PI / n * (i as f64 + 0.5)).sin();
            x[i] * w
        })
        .collect()
}

fn to_s16(v: f64) -> i16 {
    let r = v.round();
    if r > 32767.0 {
        32767
    } else if r < -32768.0 {
        -32768
    } else {
        r as i16
    }
}

/// Book 1 index for signed quad (w,x,y,z) each in −1..=1.
fn quad_idx(w: i32, x: i32, y: i32, z: i32) -> usize {
    ((w + 1) * 27 + (x + 1) * 9 + (y + 1) * 3 + (z + 1)) as usize
}

#[test]
fn sine_lc_matches_naive_imdct_within_1lsb() -> Result<(), Error> {
    // One spectral line in sfb 0 (4 bins): (0,0,0,1) → bin 3 = 1.
    let idx = quad_idx(0, 0, 0, 1);
    let payload = sce_book1(&[idx], 1);
    let mut spec = vec![0.0f64; FRAME];
    spec[3] = invquant(1);
    let expected = naive_sine_window_long(&spec);

    let mut dec = StreamDecoder::new();
    let frame = dec.decode_raw_data_block(2, 3, 48_000, 1, 1, &payload)?;
    let got = &frame.planar[0];
    assert_eq!(got.len(), FRAME);
    let err = max_abs(got, &expected);
    assert!(err <= 1.0, "sine max abs {err} > 1 LSB of s16");
    let snr = snr_db(&expected, got);
    assert!(snr >= 90.0, "sine SNR {snr} dB");

    let gold_s16: Vec<i16> = expected.iter().copied().map(to_s16).collect();
    let got_s16: Vec<i16> = got.iter().copied().map(to_s16).collect();
    let max_lsb = gold_s16
        .iter()
        .zip(got_s16.iter())
        .map(|(a, b)| (*a as i32 - *b as i32).unsigned_abs())
        .max()
        .unwrap_or(0);
    assert!(max_lsb <= 1, "sine s16 max lsb {max_lsb}");
    Ok(())
}

#[test]
fn noise_lc_scattered_quads_within_1lsb() -> Result<(), Error> {
    // Five sfb of book 1, mixed non-zero quads — a noise-like spectrum.
    let idxs = [
        quad_idx(1, 0, -1, 0),
        quad_idx(0, 1, 0, -1),
        quad_idx(-1, 0, 1, 0),
        quad_idx(0, -1, 0, 1),
        quad_idx(1, -1, 1, -1),
    ];
    let payload = sce_book1(&idxs, 5);
    let quads = [
        [1, 0, -1, 0],
        [0, 1, 0, -1],
        [-1, 0, 1, 0],
        [0, -1, 0, 1],
        [1, -1, 1, -1],
    ];
    let mut spec = vec![0.0f64; FRAME];
    for (b, q) in quads.iter().enumerate() {
        for (i, &c) in q.iter().enumerate() {
            spec[b * 4 + i] = invquant(c);
        }
    }
    let expected = naive_sine_window_long(&spec);
    let mut dec = StreamDecoder::new();
    let frame = dec.decode_raw_data_block(2, 3, 48_000, 1, 1, &payload)?;
    let got = &frame.planar[0];
    let err = max_abs(got, &expected);
    assert!(err <= 1.0, "noise max abs {err} > 1 LSB");
    let snr = snr_db(&expected, got);
    assert!(snr >= 90.0, "noise SNR {snr} dB");
    Ok(())
}

#[test]
fn committed_sine_and_noise_goldens() -> Result<(), Error> {
    let sine_adts = include_bytes!("goldens/sine.adts");
    let sine_pcm = include_bytes!("goldens/sine.s16");
    let noise_adts = include_bytes!("goldens/noise.adts");
    let noise_pcm = include_bytes!("goldens/noise.s16");
    check_golden(sine_adts, sine_pcm)?;
    check_golden(noise_adts, noise_pcm)?;
    Ok(())
}

fn check_golden(adts: &[u8], gold: &[u8]) -> Result<(), Error> {
    let (hdr, off) = AdtsHeader::parse(adts)?;
    let mut dec = StreamDecoder::new();
    let frame = dec.decode_frame(&hdr, &adts[off..])?;
    let got: Vec<u8> = frame.planar[0]
        .iter()
        .flat_map(|&v| to_s16(v).to_le_bytes())
        .collect();
    assert_eq!(got.len(), gold.len());
    let mut max_lsb = 0u32;
    let mut ps = 0.0f64;
    let mut pe = 0.0f64;
    for (g, o) in gold.chunks_exact(2).zip(got.chunks_exact(2)) {
        let gv = i16::from_le_bytes([g[0], g[1]]);
        let ov = i16::from_le_bytes([o[0], o[1]]);
        max_lsb = max_lsb.max((i32::from(gv) - i32::from(ov)).unsigned_abs());
        let gs = f64::from(gv);
        let es = gs - f64::from(ov);
        ps += gs * gs;
        pe += es * es;
    }
    assert!(max_lsb <= 1, "golden max lsb {max_lsb}");
    let snr = if pe == 0.0 {
        200.0
    } else {
        10.0 * (ps / pe).log10()
    };
    assert!(snr >= 90.0, "golden SNR {snr} dB");
    Ok(())
}

#[test]
fn adts_sine_roundtrip_via_header() -> Result<(), Error> {
    let idx = quad_idx(0, 0, 0, 1);
    let payload = sce_book1(&[idx], 1);
    let adts = adts_wrap(&payload, 1, 3);
    let (hdr, off) = AdtsHeader::parse(&adts)?;
    let mut dec = StreamDecoder::new();
    let frame = dec.decode_frame(&hdr, &adts[off..])?;
    assert_eq!(frame.sample_rate, 48_000);
    assert_eq!(frame.channels, 1);
    let energy: f64 = frame.planar[0].iter().map(|x| x * x).sum();
    assert!(energy > 0.0, "ADTS sine was silent");
    Ok(())
}
