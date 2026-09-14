//! Channel mapping: PCE parse, default config slots, engine-level reorder.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::adts::AdtsHeader;
use super::asc::AudioSpecificConfig;
use super::bits::{BitReader, BitWriter};
use super::channel_map::{
    ElemKind, Element, PceChannelMap, PlaneMap, map_planes, mono_mix, parse_pce,
};
use super::decode::StreamDecoder;
use super::error::Error;
use super::huff_quad::{H1_CODE, H1_LEN};

/// Same gain as `golden_tests`: one quant-1 bin peaks around 4k LSB.
const AUDIBLE_GAIN: u8 = 188;

/// Book 1 index for a signed quad (w,x,y,z) each in −1..=1.
fn quad_idx(w: i32, x: i32, y: i32, z: i32) -> usize {
    ((w + 1) * 27 + (x + 1) * 9 + (y + 1) * 3 + (z + 1)) as usize
}

/// `single_channel_element()` body (after id+tag): one spectral line at
/// `line_sfb` (bin `swb_offset[line_sfb] + 3`), zeros elsewhere.
pub(super) fn write_ics_line(w: &mut BitWriter, line_sfb: u8) {
    let max_sfb = line_sfb + 1;
    w.write(u32::from(AUDIBLE_GAIN), 8);
    w.write_bit(false); // ics reserved
    w.write(0, 2); // ONLY_LONG
    w.write_bit(false); // window_shape
    w.write(u32::from(max_sfb), 6);
    w.write_bit(false); // predictor
    w.write(1, 4); // book 1
    w.write(u32::from(max_sfb), 5); // one section
    for _ in 0..max_sfb {
        w.write(0, 1); // sf dpcm 0 per band
    }
    w.write_bit(false); // pulse
    w.write_bit(false); // tns
    w.write_bit(false); // gain control
    for b in 0..max_sfb {
        let idx = if b == line_sfb {
            quad_idx(0, 0, 0, 1)
        } else {
            quad_idx(0, 0, 0, 0)
        };
        w.write(u32::from(H1_CODE[idx]), u32::from(H1_LEN[idx]));
    }
}

pub(super) fn write_sce(w: &mut BitWriter, tag: u8, line_sfb: u8) {
    w.write(0, 3); // ID_SCE
    w.write(u32::from(tag), 4);
    write_ics_line(w, line_sfb);
}

fn write_lfe(w: &mut BitWriter, tag: u8, line_sfb: u8) {
    w.write(3, 3); // ID_LFE
    w.write(u32::from(tag), 4);
    write_ics_line(w, line_sfb);
}

pub(super) fn write_cpe(w: &mut BitWriter, tag: u8, line_l: u8, line_r: u8) {
    w.write(1, 3); // ID_CPE
    w.write(u32::from(tag), 4);
    w.write_bit(false); // common_window
    write_ics_line(w, line_l);
    write_ics_line(w, line_r);
}

/// Minimal dependent CCE: one SCE target, silent ICS, FAAD2 Table 4.4.8 order.
fn write_cce_stub(w: &mut BitWriter) {
    w.write(2, 3); // ID_CCE
    w.write(0, 4); // element_instance_tag
    w.write_bit(false); // ind_sw
    w.write(0, 3); // 1 coupling target
    w.write_bit(false); // target is SCE
    w.write(0, 4); // target tag
    w.write_bit(false); // cc_domain
    w.write_bit(false); // sign
    w.write(0, 2); // scale
    w.write(128, 8); // coupling ICS global_gain
    w.write_bit(false); // ics reserved
    w.write(0, 2); // ONLY_LONG
    w.write_bit(false); // window_shape
    w.write(0, 6); // max_sfb 0 → no sections / sf / spectral
    w.write_bit(false); // predictor
    w.write_bit(false); // pulse
    w.write_bit(false); // tns
    w.write_bit(false); // gain control
}

/// PCE (front/back lists + LFE tags; no side/assoc/cc), byte-aligned per
/// `byte_align()` counted from the element start (PCE is frame-initial here).
pub(super) fn write_pce(w: &mut BitWriter, front: &[(bool, u8)], back: &[(bool, u8)], lfe: &[u8]) {
    w.write(5, 3); // ID_PCE
    w.write(0, 4); // element_instance_tag
    w.write(1, 2); // object_type LC
    w.write(3, 4); // 48 kHz
    w.write(front.len() as u32, 4);
    w.write(0, 4); // side
    w.write(back.len() as u32, 4);
    w.write(lfe.len() as u32, 2);
    w.write(0, 3); // assoc
    w.write(0, 4); // cc
    w.write_bit(false); // mono_mixdown
    w.write_bit(false); // stereo_mixdown
    w.write_bit(false); // matrix_mixdown
    for &(is_cpe, tag) in front {
        w.write_bit(is_cpe);
        w.write(u32::from(tag), 4);
    }
    for &(is_cpe, tag) in back {
        w.write_bit(is_cpe);
        w.write(u32::from(tag), 4);
    }
    for &tag in lfe {
        w.write(u32::from(tag), 4);
    }
    let written =
        3 + 4 + 2 + 4 + 4 + 4 + 4 + 2 + 3 + 4 + 3 + 5 * (front.len() + back.len()) + 4 * lfe.len();
    let pad = (8 - written % 8) % 8;
    for _ in 0..pad {
        w.write_bit(false);
    }
    w.write(0, 8); // comment bytes
}

fn sce_payload(tag: u8, line_sfb: u8) -> Vec<u8> {
    let mut w = BitWriter::new();
    write_sce(&mut w, tag, line_sfb);
    w.write(7, 3); // ID_END
    w.finish()
}

fn cpe_payload(tag: u8, line_l: u8, line_r: u8) -> Vec<u8> {
    let mut w = BitWriter::new();
    write_cpe(&mut w, tag, line_l, line_r);
    w.write(7, 3);
    w.finish()
}

fn lfe_payload(tag: u8, line_sfb: u8) -> Vec<u8> {
    let mut w = BitWriter::new();
    write_lfe(&mut w, tag, line_sfb);
    w.write(7, 3);
    w.finish()
}

pub(super) fn decode_planes(payload: &[u8], config: u8) -> Result<Vec<Vec<f32>>, Error> {
    let mut dec = StreamDecoder::new();
    let frame = dec.decode_raw_data_block(2, 3, 48_000, config, 1, payload)?;
    Ok(frame.planar)
}

pub(super) fn wrap_adts(payload: &[u8]) -> Vec<u8> {
    let hdr = AdtsHeader {
        mpeg_version_mpeg2: false,
        protection_absent: true,
        profile: 1,
        sampling_frequency_index: 3,
        channel_configuration: 0,
        aac_frame_length: (7 + payload.len()) as u16,
        adts_buffer_fullness: 0x7FF,
        number_of_raw_data_blocks_in_frame: 1,
    };
    let mut out = hdr.write().to_vec();
    out.extend_from_slice(payload);
    out
}

fn pce_lc(front: Vec<(bool, u8)>) -> PceChannelMap {
    PceChannelMap {
        object_type: 1,
        sf_index: 3,
        front,
        ..Default::default()
    }
}

fn assert_fmt(got: Result<Vec<PlaneMap>, Error>, msg: &str) {
    match got {
        Err(Error::Format(m)) => assert_eq!(m, msg),
        other => panic!("expected Format({msg:?}), got {other:?}"),
    }
}

#[test]
fn parse_pce_reads_front_back_lfe_lists() -> Result<(), Error> {
    let mut w = BitWriter::new();
    write_pce(&mut w, &[(false, 5), (true, 2)], &[(true, 1)], &[0]);
    let bytes = w.finish();
    let mut br = BitReader::new(&bytes);
    let _id = br.read(3)?;
    let pce = parse_pce(&mut br)?;
    assert_eq!(pce.object_type, 1);
    assert_eq!(pce.sf_index, 3);
    assert_eq!(pce.front, vec![(false, 5), (true, 2)]);
    assert_eq!(pce.back, vec![(true, 1)]);
    assert_eq!(pce.lfe, vec![0]);
    Ok(())
}

#[test]
fn asc_channel_configuration_zero_parses() -> Result<(), Error> {
    let (asc, _) = AudioSpecificConfig::parse(&unhex_pce_asc())?;
    assert_eq!(asc.channel_configuration, 0);
    assert_eq!(asc.sample_rate, 48_000);
    let pce = asc.pce.expect("embedded PCE");
    assert_eq!(pce.object_type, 1);
    assert_eq!(pce.sf_index, 3);
    assert_eq!(pce.front, vec![(false, 0)]);
    Ok(())
}

fn unhex_pce_asc() -> Vec<u8> {
    // TASK-17 `asc-lc-ch0-embedded-pce`
    hex_bytes("118004c400000000")
}

fn hex_bytes(s: &str) -> Vec<u8> {
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap())
        .collect()
}

#[test]
fn asc_embedded_pce_seeds_decoder_without_in_band_pce() -> Result<(), Error> {
    let (asc, _) = AudioSpecificConfig::parse(&unhex_pce_asc())?;
    let mut dec = StreamDecoder::new();
    dec.set_config_pce(asc.pce.expect("embedded PCE"));
    let frame = dec.decode_raw_data_block(2, 3, 48_000, 0, 1, &sce_payload(0, 8))?;
    assert_eq!(frame.planar.len(), 1);
    Ok(())
}

#[test]
fn embedded_and_in_band_pce_lists_agree() -> Result<(), Error> {
    let (asc, _) = AudioSpecificConfig::parse(&unhex_pce_asc())?;
    let embedded = asc.pce.expect("embedded PCE");
    let pce_bits = hex_bytes("04c400000000");
    let mut br = BitReader::new(&pce_bits);
    let in_band = parse_pce(&mut br)?;
    assert_eq!(embedded.front, in_band.front);
    assert_eq!(embedded.object_type, in_band.object_type);
    assert_eq!(embedded.sf_index, in_band.sf_index);
    Ok(())
}

fn el(kind: ElemKind, tag: u8, plane: usize) -> Element {
    Element { kind, tag, plane }
}

fn srcs(order: &[PlaneMap]) -> Vec<Option<usize>> {
    order.iter().map(|p| p.src).collect()
}

#[test]
fn default_config_6_maps_to_lavc_5_1_order() {
    // Bitstream element order for cfg 6: SCE(C), CPE(front), CPE(back), LFE.
    let elems = [
        el(ElemKind::Sce, 0, 0),
        el(ElemKind::Cpe, 0, 1),
        el(ElemKind::Cpe, 1, 3),
        el(ElemKind::Lfe, 0, 5),
    ];
    let order = map_planes(&elems, None, 6, 3).unwrap();
    // lavc 5.1: FL FR FC LFE BL BR.
    assert_eq!(
        srcs(&order),
        vec![Some(1), Some(2), Some(0), Some(5), Some(3), Some(4)]
    );
    let lfe: Vec<bool> = order.iter().map(|p| p.lfe).collect();
    assert_eq!(lfe, vec![false, false, false, true, false, false]);
}

#[test]
fn default_configs_3_to_5_map_to_lavc_layouts() {
    let elems = [
        el(ElemKind::Sce, 0, 0),
        el(ElemKind::Cpe, 0, 1),
        el(ElemKind::Sce, 1, 3),
    ];
    // cfg 3 = FL FR FC (second SCE absent).
    assert_eq!(
        srcs(&map_planes(&elems[..2], None, 3, 3).unwrap()),
        vec![Some(1), Some(2), Some(0)]
    );
    // cfg 4 = FL FR FC BC.
    assert_eq!(
        srcs(&map_planes(&elems, None, 4, 3).unwrap()),
        vec![Some(1), Some(2), Some(0), Some(3)]
    );
    // cfg 5 = FL FR FC BL BR (second element is a CPE, not SCE).
    let elems5 = [
        el(ElemKind::Sce, 0, 0),
        el(ElemKind::Cpe, 0, 1),
        el(ElemKind::Cpe, 1, 3),
    ];
    assert_eq!(
        srcs(&map_planes(&elems5, None, 5, 3).unwrap()),
        vec![Some(1), Some(2), Some(0), Some(3), Some(4)]
    );
}

#[test]
fn missing_element_maps_to_silence_extra_keeps_audio() {
    // cfg 6 but no LFE element, plus an unexpected extra SCE.
    let elems = [
        el(ElemKind::Sce, 0, 0),
        el(ElemKind::Cpe, 0, 1),
        el(ElemKind::Cpe, 1, 3),
        el(ElemKind::Sce, 7, 5),
    ];
    let order = map_planes(&elems, None, 6, 3).unwrap();
    assert_eq!(
        srcs(&order),
        vec![Some(1), Some(2), Some(0), None, Some(3), Some(4), Some(5)]
    );
}

#[test]
fn mono_mix_is_mean_of_non_lfe_planes() {
    let planes = vec![vec![1.0f32; 4], vec![3.0; 4], vec![5.0; 4]];
    let order = [
        PlaneMap {
            src: Some(0),
            lfe: false,
        },
        PlaneMap {
            src: Some(1),
            lfe: false,
        },
        PlaneMap {
            src: Some(2),
            lfe: true,
        },
    ];
    assert_eq!(mono_mix(&planes, &order), vec![2.0; 4]);
    // All-LFE pathological stream: fall back to the mean of everything.
    let all_lfe = [
        PlaneMap {
            src: Some(0),
            lfe: true,
        },
        PlaneMap {
            src: Some(1),
            lfe: true,
        },
    ];
    assert_eq!(mono_mix(&planes[..2], &all_lfe), vec![2.0; 4]);
}

#[test]
fn pce_declaration_order_overrides_element_order() -> Result<(), Error> {
    let ref_sce5 = decode_planes(&sce_payload(5, 1), 1)?;
    let ref_cpe2 = decode_planes(&cpe_payload(2, 3, 5), 2)?;
    let ref_cpe1 = decode_planes(&cpe_payload(1, 6, 8), 2)?;
    let ref_lfe0 = decode_planes(&lfe_payload(0, 2), 1)?;

    // PCE declares front SCE tag5, front CPE tag2, back CPE tag1, LFE tag0,
    // while the bitstream carries CPE1, LFE0, SCE5, CPE2 in that order.
    let mut w = BitWriter::new();
    write_pce(&mut w, &[(false, 5), (true, 2)], &[(true, 1)], &[0]);
    write_cpe(&mut w, 1, 6, 8);
    write_lfe(&mut w, 0, 2);
    write_sce(&mut w, 5, 1);
    write_cpe(&mut w, 2, 3, 5);
    w.write(7, 3); // ID_END
    let planar = decode_planes(&w.finish(), 0)?;

    let expect = [
        &ref_sce5[0],
        &ref_cpe2[0],
        &ref_cpe2[1],
        &ref_cpe1[0],
        &ref_cpe1[1],
        &ref_lfe0[0],
    ];
    assert_eq!(planar.len(), expect.len(), "PCE plane count");
    for (i, (got, want)) in planar.iter().zip(expect.iter()).enumerate() {
        assert_eq!(got, *want, "plane {i} must follow PCE declaration order");
    }
    Ok(())
}

#[test]
fn cce_between_channels_does_not_corrupt_them() -> Result<(), Error> {
    let ref_a = decode_planes(&sce_payload(0, 1), 1)?;
    let ref_b = decode_planes(&sce_payload(1, 3), 1)?;
    let mut w = BitWriter::new();
    write_sce(&mut w, 0, 1);
    write_cce_stub(&mut w);
    write_sce(&mut w, 1, 3);
    w.write(7, 3);
    let planar = decode_planes(&w.finish(), 0)?;
    assert_eq!(planar.len(), 2, "silent CCE must not emit a plane");
    assert_eq!(planar[0], ref_a[0], "channel before CCE corrupted");
    assert_eq!(planar[1], ref_b[0], "channel after CCE corrupted");
    Ok(())
}

#[test]
fn pce_mismatch_is_format_not_silence() {
    let s0 = [el(ElemKind::Sce, 0, 0)];
    let mut p = pce_lc(vec![(false, 0)]);
    p.object_type = 0;
    assert_fmt(map_planes(&s0, Some(&p), 0, 3), "PCE object_type is not LC");
    p = pce_lc(vec![(false, 0)]);
    p.sf_index = 4;
    assert_fmt(
        map_planes(&s0, Some(&p), 0, 3),
        "PCE sf_index does not match stream",
    );
    assert_fmt(
        map_planes(&[], Some(&pce_lc(vec![(false, 5)])), 0, 3),
        "PCE missing channel element",
    );
    let two = [el(ElemKind::Sce, 0, 0), el(ElemKind::Sce, 1, 1)];
    assert_fmt(
        map_planes(&two, Some(&pce_lc(vec![(false, 0)])), 0, 3),
        "PCE extra channel element",
    );
    assert_fmt(
        map_planes(&s0, Some(&pce_lc(vec![(true, 0)])), 0, 3),
        "PCE missing channel element",
    );
    let seven: Vec<_> = (0..7).map(|t| el(ElemKind::Sce, t, t as usize)).collect();
    let front: Vec<_> = (0..7).map(|t| (false, t)).collect();
    assert_fmt(
        map_planes(&seven, Some(&pce_lc(front)), 0, 3),
        "PCE layout exceeds 5.1",
    );
}

pub(super) fn pce_frame(
    front: &[(bool, u8)],
    back: &[(bool, u8)],
    lfe: &[u8],
    body: impl FnOnce(&mut BitWriter),
) -> Vec<u8> {
    let mut w = BitWriter::new();
    write_pce(&mut w, front, back, lfe);
    body(&mut w);
    w.write(7, 3);
    w.finish()
}

#[test]
fn pce_decode_rejects_inconsistent_elements() {
    let err = |p: Vec<u8>| decode_planes(&p, 0).unwrap_err();
    assert!(matches!(
        err(pce_frame(&[(false, 5)], &[], &[], |_| {})),
        Error::Format("PCE missing channel element")
    ));
    let extra = pce_frame(&[(false, 0)], &[], &[], |w| {
        write_sce(w, 0, 1);
        write_sce(w, 1, 3);
    });
    assert!(matches!(
        err(extra.clone()),
        Error::Format("PCE extra channel element")
    ));
    assert!(matches!(
        err(pce_frame(&[(true, 0)], &[], &[], |w| write_sce(w, 0, 1))),
        Error::Format("PCE missing channel element")
    ));
    let msg = crate::decode(&wrap_adts(&extra)).unwrap_err().to_string();
    assert!(msg.contains("PCE extra channel element"), "{msg}");
}

#[test]
fn pce_speech_mono_excludes_declared_lfe() -> Result<(), Error> {
    let payload = pce_frame(&[(false, 5), (true, 2)], &[(true, 1)], &[0], |w| {
        write_cpe(w, 1, 6, 8);
        write_lfe(w, 0, 2);
        write_sce(w, 5, 1);
        write_cpe(w, 2, 3, 5);
    });
    let split = decode_planes(&payload, 0)?;
    let mut dec = StreamDecoder::new();
    dec.mix_down_mono = true;
    let mono = dec.decode_raw_data_block(2, 3, 48_000, 0, 1, &payload)?;
    assert_eq!(mono.planar.len(), 1);
    let inv = 1.0 / 5.0;
    for (i, &mv) in mono.planar[0].iter().enumerate() {
        let mean = (0..5).map(|c| split[c][i]).sum::<f32>() * inv;
        assert!((mv - mean).abs() <= 1e-3, "sample {i} mixed LFE");
    }
    Ok(())
}
