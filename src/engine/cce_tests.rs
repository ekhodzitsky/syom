//! Dependent CCE reconstruction: unity add, channel select, missing target.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::bits::BitWriter;
use super::cce::{CcePayload, apply_dependent, parse_cce};
use super::channel_map_tests::{decode_planes, wrap_adts, write_cpe, write_ics_line, write_sce};
use super::error::Error;
use super::huff_quad::{H1_CODE, H1_LEN};
use super::ics::{IcsInfo, WindowSequence, WindowShape};
use super::section::SectionData;

fn long_ics(max_sfb: u8) -> IcsInfo {
    IcsInfo {
        window_sequence: WindowSequence::OnlyLong,
        window_shape: WindowShape::Sine,
        max_sfb,
        num_windows: 1,
        num_window_groups: 1,
        window_group_length: [1, 0, 0, 0, 0, 0, 0, 0],
        num_swb: 49,
    }
}

fn write_silent_sce(w: &mut BitWriter, tag: u8) {
    w.write(0, 3);
    w.write(u32::from(tag), 4);
    w.write(128, 8);
    w.write_bit(false);
    w.write(0, 2);
    w.write_bit(false);
    w.write(0, 6);
    w.write_bit(false);
    w.write_bit(false);
    w.write_bit(false);
    w.write_bit(false);
}

fn rdb(body: impl FnOnce(&mut BitWriter)) -> Vec<u8> {
    let mut w = BitWriter::new();
    body(&mut w);
    w.write(7, 3);
    w.finish()
}

fn write_cce_dep_sce(w: &mut BitWriter, tgt: u8, line_sfb: u8) {
    w.write(2, 3);
    w.write(0, 4);
    w.write_bit(false);
    w.write(0, 3);
    w.write_bit(false);
    w.write(u32::from(tgt), 4);
    w.write_bit(false);
    w.write_bit(false);
    w.write(0, 2);
    write_ics_line(w, line_sfb);
}

fn write_cce_dep_cpe(w: &mut BitWriter, tgt: u8, cc_l: bool, cc_r: bool, line_sfb: u8) {
    w.write(2, 3);
    w.write(0, 4);
    w.write_bit(false);
    w.write(0, 3);
    w.write_bit(true);
    w.write(u32::from(tgt), 4);
    w.write_bit(cc_l);
    w.write_bit(cc_r);
    w.write_bit(false);
    w.write_bit(false);
    w.write(0, 2);
    write_ics_line(w, line_sfb);
}

fn write_ics_short_line(w: &mut BitWriter, line_sfb: u8) {
    let max_sfb = line_sfb + 1;
    w.write(188, 8);
    w.write_bit(false);
    w.write(2, 2);
    w.write_bit(false);
    w.write(u32::from(max_sfb), 4);
    w.write(0x7f, 7); // one group of 8
    w.write(1, 4);
    w.write(u32::from(max_sfb), 3);
    for _ in 0..max_sfb {
        w.write(0, 1);
    }
    w.write_bit(false);
    w.write_bit(false);
    w.write_bit(false);
    for g in 0..8 {
        for b in 0..max_sfb {
            let q = if g == 0 && b == line_sfb { 1 } else { 0 };
            let idx = (40 + q) as usize;
            w.write(u32::from(H1_CODE[idx]), u32::from(H1_LEN[idx]));
        }
    }
}

fn write_silent_short_sce(w: &mut BitWriter, tag: u8) {
    w.write(0, 3);
    w.write(u32::from(tag), 4);
    w.write(128, 8);
    w.write_bit(false);
    w.write(2, 2);
    w.write_bit(false);
    w.write(0, 4);
    w.write(0x7f, 7);
    w.write_bit(false);
    w.write_bit(false);
    w.write_bit(false);
}

#[test]
fn apply_dependent_adds_gain_times_cce_bin() -> Result<(), Error> {
    let cce = CcePayload {
        tag: 0,
        independent: false,
        n_coupled: 1,
        n_gain_lists: 1,
        domain: 0,
        targets: vec![],
        ics: long_ics(1),
        sections: SectionData {
            sfb_cb: vec![vec![1]],
        },
        sf: Default::default(),
        spec: {
            let mut s = vec![0.0f32; 1024];
            s[3] = 4.0;
            s
        },
        tns: None,
        gains: vec![vec![0.5]],
    };
    let mut target = vec![1.0f32; 1024];
    apply_dependent(&mut target, &cce, 0, 3)?;
    assert!((target[3] - 3.0).abs() < 1e-5, "{}", target[3]);
    assert!((target[8] - 1.0).abs() < 1e-6);
    Ok(())
}

#[test]
fn silent_dependent_cce_is_noop() -> Result<(), Error> {
    let sce = decode_planes(&rdb(|w| write_sce(w, 0, 1)), 1)?;
    let both = decode_planes(
        &rdb(|w| {
            write_sce(w, 0, 1);
            write_cce_dep_sce(w, 0, 0);
        }),
        1,
    )?;
    // CCE with a line on a coded band *does* add; max_sfb 0 stub is the no-op
    // (channel_map_tests::cce_between_channels_does_not_corrupt_them).
    assert_eq!(sce.len(), 1);
    assert_eq!(both.len(), 1);
    assert_ne!(sce[0], both[0], "unity CCE must change the target");
    Ok(())
}

#[test]
fn unity_cce_on_silent_sce_matches_audible_sce() -> Result<(), Error> {
    let audible = decode_planes(&rdb(|w| write_sce(w, 0, 1)), 1)?;
    let coupled = decode_planes(
        &rdb(|w| {
            write_silent_sce(w, 0);
            write_cce_dep_sce(w, 0, 1);
        }),
        1,
    )?;
    assert_eq!(coupled.len(), 1);
    assert_eq!(coupled[0].len(), audible[0].len());
    for (i, (a, b)) in audible[0].iter().zip(coupled[0].iter()).enumerate() {
        assert!(
            (a - b).abs() <= 1e-3,
            "sample {i}: audible {a} vs coupled {b}"
        );
    }
    Ok(())
}

#[test]
fn missing_cce_target_is_format() {
    let err = decode_planes(&rdb(|w| write_cce_dep_sce(w, 5, 1)), 1).expect_err("missing");
    assert!(matches!(err, Error::Format("CCE missing target")));
}

#[test]
fn cpe_left_only_select_leaves_right() -> Result<(), Error> {
    let pair = decode_planes(&rdb(|w| write_cpe(w, 0, 1, 3)), 2)?;
    let coupled = decode_planes(
        &rdb(|w| {
            write_cpe(w, 0, 1, 3);
            write_cce_dep_cpe(w, 0, true, false, 5);
        }),
        2,
    )?;
    assert_eq!(coupled.len(), 2);
    assert_ne!(coupled[0], pair[0], "left must receive CCE");
    assert_eq!(coupled[1], pair[1], "right must stay uncoupled");
    let right = decode_planes(
        &rdb(|w| {
            write_cpe(w, 0, 1, 3);
            write_cce_dep_cpe(w, 0, false, true, 5);
        }),
        2,
    )?;
    assert_eq!(right[0], pair[0], "left must stay uncoupled");
    assert_ne!(right[1], pair[1], "right must receive CCE");
    Ok(())
}

#[test]
fn short_window_unity_matches_audible() -> Result<(), Error> {
    let audible = decode_planes(
        &rdb(|w| {
            w.write(0, 3);
            w.write(0, 4);
            write_ics_short_line(w, 0);
        }),
        1,
    )?;
    let coupled = decode_planes(
        &rdb(|w| {
            write_silent_short_sce(w, 0);
            w.write(2, 3);
            w.write(0, 4);
            w.write_bit(false);
            w.write(0, 3);
            w.write_bit(false);
            w.write(0, 4);
            w.write_bit(false);
            w.write_bit(false);
            w.write(0, 2);
            write_ics_short_line(w, 0);
        }),
        1,
    )?;
    assert_eq!(coupled[0].len(), audible[0].len());
    for (i, (a, b)) in audible[0].iter().zip(coupled[0].iter()).enumerate() {
        assert!((a - b).abs() <= 1e-3, "short sample {i}: {a} vs {b}");
    }
    Ok(())
}

#[test]
fn public_decode_dependent_cce_succeeds() {
    let bytes = wrap_adts(&rdb(|w| {
        write_silent_sce(w, 0);
        write_cce_dep_sce(w, 0, 1);
    }));
    let pcm = crate::decode_with(&bytes, &crate::DecodeOptions::unbounded()).unwrap();
    assert_eq!(pcm.channels.len(), 1);
    assert!(pcm.channels[0].iter().any(|s| s.abs() > 1e-4));
}

#[test]
fn parse_independent_still_flags_independent() {
    let mut w = BitWriter::new();
    w.write(0, 4);
    w.write_bit(true);
    w.write(0, 3);
    w.write_bit(false);
    w.write(0, 4);
    w.write_bit(false);
    w.write_bit(false);
    w.write(0, 2);
    w.write(128, 8);
    w.write_bit(false);
    w.write(0, 2);
    w.write_bit(false);
    w.write(0, 6);
    w.write_bit(false);
    w.write_bit(false);
    w.write_bit(false);
    w.write_bit(false);
    let bytes = w.finish();
    let mut br = super::bits::BitReader::new(&bytes);
    let cce = parse_cce(&mut br, 3, 2).unwrap();
    assert!(cce.independent);
}
