//! TASK-62: `channel_configuration` 7 (7.1) slots, PCE 7.1 equivalence,
//! missing / duplicate elements, unsupported configurations 8–15.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::bits::BitWriter;
use super::channel_map::{ElemKind, Element, PceChannelMap, mono_mix, reorder};
use super::channel_map_tests::{decode_planes, pce_frame, write_cpe, write_lfe, write_sce};
use super::error::Error;
use crate::layout::{Channel, mpeg_channels};

fn el(kind: ElemKind, tag: u8, plane: usize) -> Element {
    Element { kind, tag, plane }
}

/// Bitstream element order for cfg 7: SCE(C), CPE(front), CPE(outside
/// front), CPE(back), LFE.
fn elems_7() -> [Element; 5] {
    [
        el(ElemKind::Sce, 0, 0),
        el(ElemKind::Cpe, 0, 1),
        el(ElemKind::Cpe, 1, 3),
        el(ElemKind::Cpe, 2, 5),
        el(ElemKind::Lfe, 0, 7),
    ]
}

/// PCE that declares the same elements the ISO way: front C, L/R, outside
/// L/R; back L/R; LFE.
fn pce_7_1() -> PceChannelMap {
    PceChannelMap {
        tag: 0,
        object_type: 1,
        sf_index: 3,
        front: vec![(false, 0), (true, 0), (true, 1)],
        side: vec![],
        back: vec![(true, 2)],
        lfe: vec![0],
    }
}

#[test]
fn default_config_7_maps_to_lavc_7_1_order_with_labels() {
    let order = super::channel_map::map_planes(&elems_7(), None, 7, 3).unwrap();
    let srcs: Vec<Option<usize>> = order.iter().map(|p| p.src).collect();
    // FL FR FC LFE BL BR SL SR: the second CPE (outside front) is the side pair.
    assert_eq!(
        srcs,
        vec![
            Some(1),
            Some(2),
            Some(0),
            Some(7),
            Some(5),
            Some(6),
            Some(3),
            Some(4)
        ]
    );
    let labels: Vec<Channel> = order.iter().map(|p| p.label).collect();
    assert_eq!(labels, mpeg_channels(7));
    let lfe: Vec<bool> = order.iter().map(|p| p.lfe).collect();
    assert_eq!(
        lfe,
        vec![false, false, false, true, false, false, false, false]
    );
}

#[test]
fn pce_7_1_and_config_7_share_planes_lfe_and_speech_mix() {
    let by_cfg = super::channel_map::map_planes(&elems_7(), None, 7, 3).unwrap();
    let by_pce = super::channel_map::map_planes(&elems_7(), Some(&pce_7_1()), 0, 3).unwrap();
    assert_eq!(by_pce.len(), 8);
    // PCE order is declaration order: C, L/R, outside L/R, back L/R, LFE.
    let labels: Vec<Channel> = by_pce.iter().map(|p| p.label).collect();
    assert_eq!(
        labels,
        vec![
            Channel::FrontCenter,
            Channel::FrontLeft,
            Channel::FrontRight,
            Channel::SideLeft,
            Channel::SideRight,
            Channel::BackLeft,
            Channel::BackRight,
            Channel::Lfe,
        ]
    );
    // Same (src, lfe, label) triples, only permuted.
    let mut a: Vec<(Option<usize>, bool, Channel)> =
        by_cfg.iter().map(|p| (p.src, p.lfe, p.label)).collect();
    let mut b: Vec<(Option<usize>, bool, Channel)> =
        by_pce.iter().map(|p| (p.src, p.lfe, p.label)).collect();
    a.sort_by_key(|t| t.0);
    b.sort_by_key(|t| t.0);
    assert_eq!(a, b);
    // Speech mono excludes exactly the LFE element in both maps.
    let planes: Vec<Vec<f32>> = (0..8).map(|i| vec![i as f32; 4]).collect();
    for order in [&by_cfg, &by_pce] {
        let mut p = planes.clone();
        reorder(&mut p, order);
        let mono = mono_mix(&p, order);
        assert!(
            mono.iter().all(|&m| (m - 3.0).abs() < 1e-5),
            "mean of planes 0..=6 (LFE is plane 7): {mono:?}"
        );
    }
}

fn block_7(skip_back: bool, dup_tag: bool) -> Vec<u8> {
    let mut w = BitWriter::new();
    write_sce(&mut w, 0, 1);
    write_cpe(&mut w, 0, 2, 3);
    write_cpe(&mut w, if dup_tag { 0 } else { 1 }, 4, 5);
    if !skip_back {
        write_cpe(&mut w, 2, 6, 7);
    }
    write_lfe(&mut w, 0, 0);
    w.write(7, 3); // END
    w.finish()
}

fn energy(p: &[f32]) -> f32 {
    p.iter().map(|v| v * v).sum()
}

#[test]
fn config_7_decodes_eight_planes_missing_back_is_silence_duplicate_is_error() {
    let planes = decode_planes(&block_7(false, false), 7).unwrap();
    assert_eq!(planes.len(), 8);
    assert!(
        planes.iter().all(|p| energy(p) > 0.0),
        "every plane carries audio"
    );
    let planes = decode_planes(&block_7(true, false), 7).unwrap();
    assert_eq!(
        planes.len(),
        8,
        "missing CPE still yields 8 labelled planes"
    );
    let silent: Vec<usize> = (0..8).filter(|&i| energy(&planes[i]) == 0.0).collect();
    assert_eq!(silent, vec![4, 5], "back pair (lavc slots 4, 5) is silence");
    assert!(matches!(
        decode_planes(&block_7(false, true), 7),
        Err(Error::Format(_))
    ));
}

#[test]
fn pce_bitstream_7_1_decodes_in_declaration_order() {
    let pce = pce_7_1();
    let block = pce_frame(&pce.front, &pce.back, &pce.lfe, |w| {
        write_sce(w, 0, 1);
        write_cpe(w, 0, 2, 3);
        write_cpe(w, 1, 4, 5);
        write_cpe(w, 2, 6, 7);
        write_lfe(w, 0, 0);
    });
    let planes = decode_planes(&block, 0).unwrap();
    assert_eq!(planes.len(), 8);
    assert!(planes.iter().all(|p| energy(p) > 0.0));
}

#[test]
fn configurations_8_to_15_are_an_explicit_unsupported_error() {
    for cfg in 8u8..=15 {
        assert!(
            matches!(
                decode_planes(&block_7(false, false), cfg),
                Err(Error::UnsupportedChannelConfiguration(c)) if c == cfg
            ),
            "cfg {cfg}"
        );
    }
}
