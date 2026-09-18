//! Per-element SBR attachment: isolation, mix-after, layout reset.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::bits::{BitReader, BitWriter};
use super::channel_map_tests::{pce_frame, write_cpe, write_ics_line, write_sce};
use super::decode::StreamDecoder;
use super::error::Error;
use super::sbr_grid::FrameClass;
use super::sbr_header::SbrHeader;
use super::sbr_huffman::{SbrHuffContext, env_tables, noise_tables};
use crate::DecodeOptions;

const FS: u32 = 48_000;
const FS_SBR: u32 = 96_000;

type Planes = Vec<Vec<f32>>;

fn write_header(w: &mut BitWriter, amp_res: bool) {
    w.write_bit(amp_res);
    w.write(5, 4);
    w.write(0, 4);
    w.write(1, 3);
    w.write(0, 2);
    w.write_bit(true);
    w.write_bit(false);
    w.write(0, 2);
    w.write_bit(false);
    w.write(2, 2);
}

fn write_minimal_sce(w: &mut BitWriter, n_high: usize, n_q: usize) {
    w.write_bit(false);
    w.write(FrameClass::FixFix.to_bits(), 2);
    w.write(0, 2);
    w.write_bit(true);
    w.write_bit(false);
    w.write_bit(false);
    for _ in 0..n_q {
        w.write(1, 2);
    }
    let (_, (f_huff, f_lav)) = env_tables(SbrHuffContext {
        coupling: false,
        ch: false,
        amp_res: false,
    });
    w.write(33, 7);
    for i in 1..n_high {
        let (len, code) = f_huff[(i + f_lav as usize) % f_huff.len()];
        w.write(code, u32::from(len));
    }
    let (_, (nf, nfl)) = noise_tables(SbrHuffContext {
        coupling: false,
        ch: false,
        amp_res: false,
    });
    w.write(10, 5);
    for i in 1..n_q {
        let (len, code) = nf[(i + nfl as usize) % nf.len()];
        w.write(code, u32::from(len));
    }
    w.write_bit(false);
    w.write_bit(false);
}

fn sbr_fil_bytes() -> Vec<u8> {
    let mut hw = BitWriter::new();
    write_header(&mut hw, true);
    let bytes = hw.finish();
    let hdr = SbrHeader::parse(&mut BitReader::new(&bytes)).unwrap();
    let bands = hdr.derive_bands(FS_SBR).unwrap();
    let mut w = BitWriter::new();
    w.write(0b1101, 4);
    w.write_bit(true);
    write_header(&mut w, true);
    write_minimal_sce(&mut w, bands.n_high(), bands.n_q());
    w.finish()
}

fn write_fil_sbr(w: &mut BitWriter) {
    let payload = sbr_fil_bytes();
    let n = payload.len() as u32;
    w.write(6, 3);
    if n < 15 {
        w.write(n, 4);
    } else {
        w.write(15, 4);
        w.write(n - 14, 8);
    }
    for b in payload {
        w.write(u32::from(b), 8);
    }
}

fn write_lfe(w: &mut BitWriter, tag: u8, line_sfb: u8) {
    w.write(3, 3);
    w.write(u32::from(tag), 4);
    write_ics_line(w, line_sfb);
}

fn he_dec() -> Box<StreamDecoder> {
    let mut dec = StreamDecoder::new();
    dec.set_he_config(true, false, FS_SBR);
    dec
}

fn decode_he(dec: &mut StreamDecoder, payload: &[u8], cfg: u8) -> Result<Planes, Error> {
    let f = dec.decode_raw_data_block(2, 3, FS, cfg, 1, payload)?;
    assert_eq!(f.sample_rate, FS_SBR);
    Ok(f.planar)
}

fn close(a: &[f32], b: &[f32]) {
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(b.iter()) {
        assert!((x - y).abs() < 1e-3, "{x} vs {y}");
    }
}

#[test]
fn two_sce_sbr_history_is_isolated() -> Result<(), Error> {
    let a = pce_frame(&[(false, 0), (false, 1)], &[], &[], |w| {
        write_sce(w, 0, 1);
        write_sce(w, 1, 3);
    });
    let b = pce_frame(&[(false, 1)], &[], &[], |w| write_sce(w, 1, 5));
    let mut both = he_dec();
    decode_he(&mut both, &a, 0)?;
    let got = decode_he(&mut both, &b, 0)?;
    let mut cold = he_dec();
    decode_he(
        &mut cold,
        &pce_frame(&[(false, 1)], &[], &[], |w| write_sce(w, 1, 3)),
        0,
    )?;
    let want = decode_he(&mut cold, &b, 0)?;
    assert_eq!(got, want, "SCE1 reused SCE0 SBR state");
    Ok(())
}

#[test]
fn interleaved_order_keeps_per_element_sbr() -> Result<(), Error> {
    let a = pce_frame(&[(false, 5), (false, 2)], &[], &[], |w| {
        write_sce(w, 5, 1);
        write_sce(w, 2, 3);
    });
    let b = pce_frame(&[(false, 5), (false, 2)], &[], &[], |w| {
        write_sce(w, 2, 5);
        write_sce(w, 5, 2);
    });
    let mut both = he_dec();
    decode_he(&mut both, &a, 0)?;
    let got = decode_he(&mut both, &b, 0)?;
    let mut s5 = he_dec();
    decode_he(
        &mut s5,
        &pce_frame(&[(false, 5)], &[], &[], |w| write_sce(w, 5, 1)),
        0,
    )?;
    let w5 = decode_he(
        &mut s5,
        &pce_frame(&[(false, 5)], &[], &[], |w| write_sce(w, 5, 2)),
        0,
    )?;
    let mut s2 = he_dec();
    decode_he(
        &mut s2,
        &pce_frame(&[(false, 2)], &[], &[], |w| write_sce(w, 2, 3)),
        0,
    )?;
    let w2 = decode_he(
        &mut s2,
        &pce_frame(&[(false, 2)], &[], &[], |w| write_sce(w, 2, 5)),
        0,
    )?;
    assert_eq!(got.len(), 2);
    assert_eq!(got[0], w5[0], "SCE5 history followed encounter order");
    assert_eq!(got[1], w2[0], "SCE2 history followed encounter order");
    Ok(())
}

#[test]
fn layout_change_does_not_reuse_sbr_state() -> Result<(), Error> {
    let sce = pce_frame(&[(false, 0)], &[], &[], |w| write_sce(w, 0, 1));
    let cpe = pce_frame(&[(true, 0)], &[], &[], |w| write_cpe(w, 0, 3, 5));
    let mut dec = he_dec();
    decode_he(&mut dec, &sce, 0)?;
    let got = decode_he(&mut dec, &cpe, 0)?;
    let mut cold = he_dec();
    let want = decode_he(&mut cold, &cpe, 0)?;
    assert_eq!(got, want, "SCE SBR leaked into CPE tag 0");
    Ok(())
}

#[test]
fn fil_attaches_to_preceding_element_only() -> Result<(), Error> {
    let both = pce_frame(&[(false, 0), (false, 1)], &[], &[], |w| {
        write_sce(w, 0, 1);
        write_fil_sbr(w);
        write_sce(w, 1, 3);
    });
    let mut got_d = he_dec();
    let got = decode_he(&mut got_d, &both, 0)?;
    let mut s0 = he_dec();
    let w0 = decode_he(
        &mut s0,
        &pce_frame(&[(false, 0)], &[], &[], |w| {
            write_sce(w, 0, 1);
            write_fil_sbr(w);
        }),
        0,
    )?;
    let mut s1 = he_dec();
    let w1 = decode_he(
        &mut s1,
        &pce_frame(&[(false, 1)], &[], &[], |w| write_sce(w, 1, 3)),
        0,
    )?;
    assert_eq!(got.len(), 2);
    close(&got[0], &w0[0]);
    close(&got[1], &w1[0]);
    assert_ne!(got[0], got[1], "shared SBR payload across elements");
    Ok(())
}

#[test]
fn he_mix_is_non_lfe_mean_after_sbr() -> Result<(), Error> {
    let payload = pce_frame(&[(false, 0)], &[], &[1], |w| {
        write_sce(w, 0, 1);
        write_lfe(w, 1, 3);
    });
    let mut split = he_dec();
    let sp = decode_he(&mut split, &payload, 0)?;
    let mut mixd = he_dec();
    mixd.mix_down_mono = true;
    let mp = decode_he(&mut mixd, &payload, 0)?;
    assert_eq!(sp.len(), 2);
    assert_eq!(mp.len(), 1);
    assert_eq!(mp[0].len(), 2048);
    close(&mp[0], &sp[0]);
    Ok(())
}

#[test]
fn two_sce_mix_matches_split_mean() -> Result<(), Error> {
    let payload = pce_frame(&[(false, 0), (false, 1)], &[], &[], |w| {
        write_sce(w, 0, 1);
        write_sce(w, 1, 3);
    });
    let mut split = he_dec();
    let sp = decode_he(&mut split, &payload, 0)?;
    let mut mixd = he_dec();
    mixd.mix_down_mono = true;
    let mp = decode_he(&mut mixd, &payload, 0)?;
    let want: Vec<f32> = sp[0]
        .iter()
        .zip(sp[1].iter())
        .map(|(a, b)| 0.5 * (a + b))
        .collect();
    close(&mp[0], &want);
    Ok(())
}

#[test]
fn cfg3_declared_2x_keeps_three_planes() -> Result<(), Error> {
    let payload = {
        let mut w = BitWriter::new();
        write_cpe(&mut w, 0, 1, 3);
        write_sce(&mut w, 0, 2);
        w.write(7, 3);
        w.finish()
    };
    let mut dec = he_dec();
    let p = decode_he(&mut dec, &payload, 3)?;
    assert_eq!(p.len(), 3);
    assert!(p.iter().all(|ch| ch.len() == 2048));
    Ok(())
}

#[test]
fn sbr_without_channel_element_is_format() {
    let bytes = {
        let mut w = BitWriter::new();
        write_fil_sbr(&mut w);
        write_sce(&mut w, 0, 1);
        w.write(7, 3);
        w.finish()
    };
    let err = he_dec()
        .decode_raw_data_block(2, 3, FS, 1, 1, &bytes)
        .expect_err("SBR first");
    assert!(matches!(err, Error::Format("SBR without channel element")));
}

#[test]
fn duplicate_sbr_extension_is_format() {
    let bytes = pce_frame(&[(false, 0)], &[], &[], |w| {
        write_sce(w, 0, 1);
        write_fil_sbr(w);
        write_fil_sbr(w);
    });
    let err = he_dec()
        .decode_raw_data_block(2, 3, FS, 0, 1, &bytes)
        .expect_err("dup SBR");
    assert!(matches!(err, Error::Format("duplicate SBR extension")));
}

#[test]
fn he48_speech_mix_matches_unbounded_mean() {
    let bytes = include_bytes!("../goldens/he48.adts");
    let split = crate::decode_with(bytes, &DecodeOptions::unbounded()).unwrap();
    let mixed = crate::decode_with(bytes, &DecodeOptions::speech()).unwrap();
    assert_eq!(split.channels.len(), 2);
    assert_eq!(mixed.channels.len(), 1);
    let want: Vec<f32> = split.channels[0]
        .iter()
        .zip(split.channels[1].iter())
        .map(|(a, b)| 0.5 * (a + b))
        .collect();
    close(&mixed.channels[0], &want);
}
