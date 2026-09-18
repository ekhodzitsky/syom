//! TASK-88: SBR writer — hand-authored bit patterns (Tables 4.63–4.73,
//! Huffman codes transcribed from the ISO tables), in-tree parser
//! round trips, fill/escape/alignment policy, unrepresentable errors.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::{
    EXT_SBR_DATA, FILL_MAX_BYTES, sbr_extension_payload, write_fill_element, write_grid,
    write_header, write_sbr_data, write_sbr_extension,
};
use crate::engine::bits::{BitReader, BitWriter};
use crate::engine::enc_sbr_est::{SLOTS, SbrEstimator, SbrFrameParams, he_header};
use crate::engine::enc_sbr_prep::SbrPrep;
use crate::engine::enc_sbr_qmf::{BANDS, EncSlot};
use crate::engine::raw_data_block::IdSynEle;
use crate::engine::sbr_element::SbrElement;
use crate::engine::sbr_envelope::{SbrEnvelopeData, SbrNoiseData};
use crate::engine::sbr_extension::SbrExtensionData;
use crate::engine::sbr_grid::{FrameClass, SbrDtdf, SbrGrid, SbrInvf};
use crate::engine::sbr_header::SbrHeader;
use crate::engine::skip::fill_count;

/// MSB-first bit string of the first `n` bits.
fn bits(bytes: &[u8], n: u64) -> String {
    (0..n)
        .map(|i| {
            let b = (bytes[(i / 8) as usize] >> (7 - (i % 8))) & 1;
            if b == 1 { '1' } else { '0' }
        })
        .collect()
}

fn header_with_extras() -> SbrHeader {
    SbrHeader {
        amp_res: false,
        start_freq: 5,
        stop_freq: 0,
        xover_band: 1,
        reserved: 0,
        header_extra_1: true,
        header_extra_2: true,
        freq_scale: 0,
        alter_scale: false,
        noise_bands: 2,
        limiter_bands: 1,
        limiter_gains: 3,
        interpol_freq: false,
        smoothing_mode: true,
    }
}

fn fixfix(num_env: usize, high: bool) -> SbrGrid {
    SbrGrid {
        frame_class: FrameClass::FixFix,
        num_env,
        num_noise: if num_env > 1 { 2 } else { 1 },
        freq_res: vec![high; num_env],
        var_bord_0: 0,
        var_bord_1: 0,
        rel_bord_0: vec![],
        rel_bord_1: vec![],
        pointer: 0,
        amp_res_override: num_env == 1,
    }
}

/// One-envelope SCE: start values and all-zero deltas, no harmonics.
pub(crate) fn minimal(
    n_high: usize,
    n_q: usize,
    env_start: i32,
    noise_start: i32,
) -> SbrFrameParams {
    let mut env = vec![0i32; n_high];
    env[0] = env_start;
    let mut noise = vec![0i32; n_q];
    noise[0] = noise_start;
    SbrFrameParams {
        grid: fixfix(1, true),
        dtdf: SbrDtdf {
            df_env: vec![false],
            df_noise: vec![false],
        },
        invf: SbrInvf {
            invf_mode: vec![0; n_q],
        },
        env_q: vec![env.clone()],
        noise_q: vec![noise.clone()],
        envelope: SbrEnvelopeData { data: vec![env] },
        noise: SbrNoiseData { data: vec![noise] },
        add_harmonic: vec![],
        est_bits: 0,
    }
}

#[test]
fn v1_header_is_sixteen_bits_table_4_63() {
    // amp_res 1 | start 10 | stop 8 | xover 0 | reserved 0 | extra1 0 | extra2 0
    let mut w = BitWriter::new();
    write_header(&mut w, &he_header(48_000, 24).unwrap()).unwrap();
    assert_eq!(w.bit_len(), 16);
    let bytes = w.finish();
    assert_eq!(bits(&bytes, 16), "1101010000000000");
    assert_eq!(bytes, vec![0xD4, 0x00]);
}

#[test]
fn header_extras_are_written_in_table_order() {
    // 0 0101 0000 001 00 1 1 | 00 0 10 | 01 11 0 1
    let mut w = BitWriter::new();
    write_header(&mut w, &header_with_extras()).unwrap();
    assert_eq!(w.bit_len(), 27);
    let bytes = w.finish();
    assert_eq!(bits(&bytes, 27), "001010000001001100010011101");
    let parsed = SbrHeader::parse(&mut BitReader::new(&bytes)).unwrap();
    assert_eq!(parsed, header_with_extras());
}

#[test]
fn header_with_non_default_values_and_clear_extra_flag_is_unrepresentable() {
    let mut h = he_header(48_000, 24).unwrap();
    h.noise_bands = 1; // extra_1 stays false
    assert!(write_header(&mut BitWriter::new(), &h).is_err());
    let mut h = he_header(48_000, 24).unwrap();
    h.limiter_gains = 0;
    assert!(write_header(&mut BitWriter::new(), &h).is_err());
    let mut h = he_header(48_000, 24).unwrap();
    h.start_freq = 16;
    assert!(write_header(&mut BitWriter::new(), &h).is_err());
}

#[test]
fn fixfix_grids_are_five_bits() {
    for (num_env, high, expect) in [(1, true, "00001"), (2, false, "00010"), (4, false, "00100")] {
        let mut w = BitWriter::new();
        write_grid(&mut w, &fixfix(num_env, high)).unwrap();
        assert_eq!(w.bit_len(), 5);
        assert_eq!(bits(&w.finish(), 5), expect, "{num_env} envelopes");
    }
    let mut bad = fixfix(2, true);
    bad.freq_res[1] = false; // FIXFIX shares one flag
    assert!(write_grid(&mut BitWriter::new(), &bad).is_err());
    assert!(write_grid(&mut BitWriter::new(), &fixfix(3, true)).is_err());
}

#[test]
fn variable_grids_match_the_parser_vectors() {
    // Same vectors as the sbr_grid parser tests, hand-assembled here:
    // FIXVAR: 01 | 01 | 10 | 00 11 | 01 | freq_res reversed 1 0 1
    let fixvar = SbrGrid {
        frame_class: FrameClass::FixVar,
        num_env: 3,
        num_noise: 2,
        freq_res: vec![true, false, true],
        var_bord_0: 0,
        var_bord_1: 1,
        rel_bord_0: vec![],
        rel_bord_1: vec![0, 3],
        pointer: 1,
        amp_res_override: false,
    };
    // VARVAR: 11 | 01 | 10 | 01 | 01 | 00 | 11 | 10 | 1 0 1
    let varvar = SbrGrid {
        frame_class: FrameClass::VarVar,
        num_env: 3,
        num_noise: 2,
        freq_res: vec![true, false, true],
        var_bord_0: 1,
        var_bord_1: 2,
        rel_bord_0: vec![0],
        rel_bord_1: vec![3],
        pointer: 2,
        amp_res_override: false,
    };
    // VARFIX: 10 | 10 | 01 | 11 | 00 | 0 1
    let varfix = SbrGrid {
        frame_class: FrameClass::VarFix,
        num_env: 2,
        num_noise: 2,
        freq_res: vec![false, true],
        var_bord_0: 2,
        var_bord_1: 0,
        rel_bord_0: vec![3],
        rel_bord_1: vec![],
        pointer: 0,
        amp_res_override: false,
    };
    for (g, expect) in [
        (fixvar, "010110001101101"),
        (varvar, "1101100101001110101"),
        (varfix, "101001110001"),
    ] {
        let mut w = BitWriter::new();
        write_grid(&mut w, &g).unwrap();
        assert_eq!(w.bit_len(), expect.len() as u64);
        let bytes = w.finish();
        assert_eq!(bits(&bytes, expect.len() as u64), expect);
        assert_eq!(SbrGrid::parse(&mut BitReader::new(&bytes)).unwrap(), g);
    }
}

#[test]
fn minimal_sce_matches_hand_authored_bits() {
    // 48 kHz v1 bands: NHigh 12, NQ 2. amp_res forced to 1.5 dB by the
    // single-envelope grid: 7-bit start, f_huffman_env_1_5dB(0) = "00";
    // noise: 5-bit start, f_huffman_env_3_0dB(0) = "0".
    let est = SbrEstimator::new(48_000, 24).unwrap();
    let bands = est.bands();
    assert_eq!((bands.n_high(), bands.n_q()), (12, 2));
    let p = minimal(12, 2, 33, 10);
    let mut w = BitWriter::new();
    write_sbr_data(&mut w, true, bands, &[&p]).unwrap();
    let expect = String::new()
        + "0" // bs_data_extra
        + "00001" // FIXFIX, 1 env, high
        + "0" + "0" // df_env, df_noise
        + "0000" // invf × 2
        + "0100001" + &"00".repeat(11) // env start 33 + 11 zero deltas
        + "01010" + "0" // noise start 10 + 1 zero delta
        + "0" + "0"; // add_harmonic, extended_data
    assert_eq!(w.bit_len(), expect.len() as u64);
    assert_eq!(w.bit_len(), 1 + 5 + 2 + 4 + 7 + 22 + 5 + 1 + 2);
    let bytes = w.finish();
    assert_eq!(bits(&bytes, expect.len() as u64), expect);
    let el = SbrElement::parse_single(&mut BitReader::new(&bytes), bands, true).unwrap();
    assert_eq!(el.channels[0].envelope, p.envelope);
    assert_eq!(el.channels[0].noise, p.noise);
    assert_eq!(el.channels[0].invf, p.invf);
    assert!(el.channels[0].add_harmonic.is_empty() && el.extension.is_none() && !el.coupling);
}

#[test]
fn nonzero_deltas_use_the_transcribed_codes() {
    // 3.0 dB (two envelopes, low res, NLow 6): t_huffman_env_3_0dB:
    // −1 "10", 0 "0", +1 "110", +2 "11110"; f_huffman_env_3_0dB: −2 "1110".
    let est = SbrEstimator::new(48_000, 24).unwrap();
    let bands = est.bands();
    let mut p = minimal(12, 2, 0, 0);
    p.grid = fixfix(2, false);
    p.dtdf = SbrDtdf {
        df_env: vec![false, true],
        df_noise: vec![false, true],
    };
    p.envelope = SbrEnvelopeData {
        data: vec![vec![20, -2, 0, 1, 0, -2], vec![1, 0, -1, 2, 0, 0]],
    };
    p.noise = SbrNoiseData {
        data: vec![vec![7, 1], vec![-1, 2]],
    };
    let mut w = BitWriter::new();
    write_sbr_data(&mut w, true, bands, &[&p]).unwrap();
    let expect = String::new()
        + "0" + "00010" + "01" + "01" + "0000"
        + "010100" + "1110" + "0" + "110" + "0" + "1110" // env 0: 6-bit start, f codes
        + "110" + "0" + "10" + "11110" + "0" + "0" // env 1: t codes
        + "00111" + "110" // noise 0: 5-bit start, f_huffman_env_3_0dB(+1)
        + "110" + "11110" // noise 1: t_huffman_noise_3_0dB(−1), (+2)
        + "0" + "0";
    assert_eq!(bits(&w.finish(), expect.len() as u64), expect);
}

#[test]
fn deltas_outside_the_codebook_are_errors() {
    let est = SbrEstimator::new(48_000, 24).unwrap();
    let bands = est.bands();
    let mut p = minimal(12, 2, 33, 10);
    p.envelope.data[0][3] = 61; // LAV 60 at 1.5 dB
    assert!(write_sbr_data(&mut BitWriter::new(), true, bands, &[&p]).is_err());
    let mut p = minimal(12, 2, 128, 10); // 7-bit start value
    p.env_q[0][0] = 128;
    assert!(write_sbr_data(&mut BitWriter::new(), true, bands, &[&p]).is_err());
    let mut p = minimal(12, 2, 33, 10);
    p.envelope.data[0].pop(); // NHigh mismatch
    assert!(write_sbr_data(&mut BitWriter::new(), true, bands, &[&p]).is_err());
    let mut p = minimal(12, 2, 33, 10);
    p.invf.invf_mode[0] = 4;
    assert!(write_sbr_data(&mut BitWriter::new(), true, bands, &[&p]).is_err());
    let mut p = minimal(12, 2, 33, 10);
    p.add_harmonic = vec![true; 5];
    assert!(write_sbr_data(&mut BitWriter::new(), true, bands, &[&p]).is_err());
    let p = minimal(12, 2, 33, 10);
    assert!(write_sbr_data(&mut BitWriter::new(), true, bands, &[]).is_err());
    assert!(write_sbr_data(&mut BitWriter::new(), true, bands, &[&p, &p, &p]).is_err());
}

#[test]
fn uncoupled_cpe_round_trips_in_table_4_66_order() {
    let est = SbrEstimator::new(48_000, 24).unwrap();
    let bands = est.bands();
    let a = minimal(12, 2, 40, 12);
    let mut b = minimal(12, 2, 35, 8);
    b.invf.invf_mode = vec![2, 3];
    b.add_harmonic = vec![false; 12];
    b.add_harmonic[4] = true;
    let mut w = BitWriter::new();
    write_sbr_data(&mut w, true, bands, &[&a, &b]).unwrap();
    // data_extra 0, coupling 0, then both grids before both dtdf.
    let bytes = w.finish();
    assert_eq!(&bits(&bytes, 12), "000000100001");
    let el = SbrElement::parse_pair(&mut BitReader::new(&bytes), bands, true).unwrap();
    assert!(!el.coupling);
    assert_eq!(el.channels.len(), 2);
    assert_eq!(el.channels[0].envelope, a.envelope);
    assert_eq!(el.channels[1].envelope, b.envelope);
    assert_eq!(el.channels[1].invf, b.invf);
    assert_eq!(el.channels[1].add_harmonic, b.add_harmonic);
    assert_eq!(el.channels[0].add_harmonic, Vec::<bool>::new());
}

#[test]
fn extension_round_trips_with_header_and_with_reuse() {
    let est = SbrEstimator::new(48_000, 24).unwrap();
    let (h, bands) = (est.header(), est.bands());
    let p = minimal(12, 2, 33, 10);
    let mut w = BitWriter::new();
    let n = write_sbr_extension(&mut w, h, true, bands, &[&p], None).unwrap();
    assert_eq!(n, 1 + 16 + 49);
    let bytes = w.finish();
    let ext = SbrExtensionData::parse(
        &mut BitReader::new(&bytes),
        IdSynEle::Sce,
        false,
        48_000,
        None,
        None,
    )
    .unwrap();
    assert!(ext.header_present && ext.crc.is_none());
    assert_eq!(ext.header, *h);
    assert_eq!(ext.num_sbr_bits, n);
    assert_eq!(ext.element.channels[0].envelope, p.envelope);
    let mut w = BitWriter::new();
    let n2 = write_sbr_extension(&mut w, h, false, bands, &[&p], None).unwrap();
    assert_eq!(n2, 1 + 49);
    let bytes = w.finish();
    assert!(
        SbrExtensionData::parse(
            &mut BitReader::new(&bytes),
            IdSynEle::Sce,
            false,
            48_000,
            None,
            None
        )
        .is_err(),
        "reuse without a prior header is ill-formed"
    );
    let ext = SbrExtensionData::parse(
        &mut BitReader::new(&bytes),
        IdSynEle::Sce,
        false,
        48_000,
        None,
        Some(*h),
    )
    .unwrap();
    assert!(!ext.header_present);
    assert_eq!(ext.element.channels[0].noise, p.noise);
}

#[test]
fn payload_is_type_nibble_plus_zero_fill_to_the_byte() {
    let est = SbrEstimator::new(48_000, 24).unwrap();
    let (h, bands) = (est.header(), est.bands());
    let p = minimal(12, 2, 33, 10);
    let payload = sbr_extension_payload(h, true, bands, &[&p], None).unwrap();
    // 4 + 66 = 70 bits → 9 bytes, 2 fill bits.
    assert_eq!(payload.len(), 9);
    assert_eq!(payload[0] >> 4, EXT_SBR_DATA as u8);
    assert_eq!(payload[8] & 0b11, 0, "bs_fill_bits are zero");
    let mut r = BitReader::new(&payload);
    assert_eq!(r.read(4).unwrap(), EXT_SBR_DATA);
    let ext = SbrExtensionData::parse(&mut r, IdSynEle::Sce, false, 48_000, Some(9), None).unwrap();
    assert_eq!(ext.num_sbr_bits, 66);
    assert_eq!(r.bit_position(), 72, "fill consumed to the payload end");
}

#[test]
fn fill_element_count_and_escape_follow_4_4_2_7() {
    let short = vec![0xA5u8; 9];
    let mut w = BitWriter::new();
    w.write(0b101, 3); // unaligned prefix, as inside an AU
    write_fill_element(&mut w, &short).unwrap();
    let bytes = w.finish();
    assert_eq!(bits(&bytes, 3 + 3 + 4 + 8), "101110100110100101");
    let mut r = BitReader::new(&bytes);
    r.read(3).unwrap();
    assert_eq!(r.read(3).unwrap(), 6, "ID_FIL");
    assert_eq!(fill_count(&mut r).unwrap(), 9);
    let long = vec![0x5Au8; 40];
    let mut w = BitWriter::new();
    write_fill_element(&mut w, &long).unwrap();
    let bytes = w.finish();
    assert_eq!(bits(&bytes, 3 + 4 + 8), "110111100011010"); // esc = 40 − 14 = 26
    let mut r = BitReader::new(&bytes);
    r.read(3).unwrap();
    assert_eq!(fill_count(&mut r).unwrap(), 40);
    assert!(write_fill_element(&mut BitWriter::new(), &[]).is_err());
    assert!(write_fill_element(&mut BitWriter::new(), &vec![0u8; FILL_MAX_BYTES]).is_ok());
    assert!(write_fill_element(&mut BitWriter::new(), &vec![0u8; FILL_MAX_BYTES + 1]).is_err());
}

/// Estimator output for a synthetic frame stream (no decoder involved).
fn estimated_frames() -> (SbrEstimator, Vec<SbrFrameParams>) {
    let n = 2048 * 6;
    let mut seed = 0x9e37_79b9u32;
    let pcm: Vec<f32> = (0..n)
        .map(|i| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let noise = (seed as f32 / u32::MAX as f32) - 0.5;
            let t = i as f32 / 48_000.0;
            0.2 * noise
                + 0.3 * (2.0 * std::f32::consts::PI * 700.0 * t).sin()
                + if i > n / 2 {
                    0.3 * (2.0 * std::f32::consts::PI * 9_100.0 * t).sin()
                } else {
                    0.0
                }
        })
        .collect();
    let mut prep = SbrPrep::new();
    let mut core = Vec::new();
    let mut slots: Vec<EncSlot> = Vec::new();
    prep.push(&pcm, &mut core, |s| slots.push(*s)).unwrap();
    let mut est = SbrEstimator::new(48_000, 24).unwrap();
    let frames = slots
        .chunks_exact(SLOTS)
        .map(|f| est.estimate(f).unwrap())
        .collect();
    assert_eq!(BANDS, 64);
    (est, frames)
}

#[test]
fn estimator_frames_serialize_to_their_bit_estimate_and_decode_back() {
    let (est, frames) = estimated_frames();
    let (h, bands) = (est.header(), est.bands());
    let mut prev_bytes: Option<Vec<u8>> = None;
    for (i, p) in frames.iter().enumerate() {
        let mut w = BitWriter::new();
        let n = write_sbr_extension(&mut w, h, i == 0, bands, &[p], None).unwrap();
        let header_bits = if i == 0 { 16 } else { 0 };
        assert_eq!(n, 1 + header_bits + u64::from(p.est_bits), "frame {i}");
        let bytes = w.finish();
        let ext = SbrExtensionData::parse(
            &mut BitReader::new(&bytes),
            IdSynEle::Sce,
            false,
            48_000,
            None,
            Some(*h),
        )
        .unwrap();
        let ch = &ext.element.channels[0];
        assert_eq!((&ch.grid, &ch.dtdf, &ch.invf), (&p.grid, &p.dtdf, &p.invf));
        assert_eq!((&ch.envelope, &ch.noise), (&p.envelope, &p.noise));
        // Byte-identical on repeat.
        let mut w2 = BitWriter::new();
        write_sbr_extension(&mut w2, h, i == 0, bands, &[p], None).unwrap();
        assert_eq!(w2.finish(), bytes);
        prev_bytes = Some(bytes);
    }
    assert!(prev_bytes.is_some());
    assert!(
        frames.iter().any(|p| p.grid.num_env > 1),
        "the 9.1 kHz onset codes a multi-envelope frame"
    );
}
