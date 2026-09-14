//! CCE Table 4.4.8 parse and unsupported fence.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::bits::{BitReader, BitWriter};
use super::channel_map_tests::wrap_adts;
use super::error::Error;
use super::skip::parse_cce;

fn silent_ics(w: &mut BitWriter) {
    w.write(128, 8);
    w.write_bit(false);
    w.write(0, 2);
    w.write_bit(false);
    w.write(0, 6);
    w.write_bit(false); // predictor
    w.write_bit(false); // pulse
    w.write_bit(false); // tns
    w.write_bit(false); // gain
}

fn parse_ok(bytes: &[u8]) -> super::skip::CceSyntax {
    let mut br = BitReader::new(bytes);
    parse_cce(&mut br, 3, 2).expect("parse_cce")
}

#[test]
fn independent_one_sce_consumes_header_ics_and_no_gain() {
    let mut w = BitWriter::new();
    w.write(7, 4); // tag
    w.write_bit(true); // independent
    w.write(0, 3); // 1 target
    w.write_bit(false); // SCE
    w.write(3, 4); // target tag
    w.write_bit(true); // cc_domain
    w.write_bit(true); // sign
    w.write(2, 2); // scale
    silent_ics(&mut w);
    let bits = 4 + 1 + 3 + 1 + 4 + 1 + 1 + 2 + 22;
    let bytes = w.finish();
    let mut br = BitReader::new(&bytes);
    let cce = parse_cce(&mut br, 3, 2).unwrap();
    assert_eq!(cce.tag, 7);
    assert!(cce.independent);
    assert_eq!(cce.n_coupled, 1);
    assert_eq!(cce.n_gain_lists, 1);
    assert_eq!(br.bit_position(), bits);
}

#[test]
fn independent_two_targets_reads_one_sf_vlc() {
    let mut w = BitWriter::new();
    w.write(0, 4);
    w.write_bit(true);
    w.write(1, 3); // 2 targets
    w.write_bit(false);
    w.write(0, 4);
    w.write_bit(false);
    w.write(1, 4);
    w.write_bit(false);
    w.write_bit(false);
    w.write(0, 2);
    silent_ics(&mut w);
    w.write_bit(false); // sf delta 0 (Table 4.A.1 index 60)
    let bytes = w.finish();
    let mut br = BitReader::new(&bytes);
    let cce = parse_cce(&mut br, 3, 2).unwrap();
    assert_eq!(cce.n_coupled, 2);
    assert_eq!(cce.n_gain_lists, 2);
    assert_eq!(br.bit_position(), 4 + 1 + 3 + 5 + 5 + 4 + 22 + 1);
}

#[test]
fn dependent_second_list_common_gain() {
    let mut w = BitWriter::new();
    w.write(0, 4);
    w.write_bit(false);
    w.write(1, 3); // 2 SCE targets
    w.write_bit(false);
    w.write(0, 4);
    w.write_bit(false);
    w.write(1, 4);
    w.write_bit(false);
    w.write_bit(false);
    w.write(0, 2);
    silent_ics(&mut w);
    w.write_bit(true); // common_gain_element_present
    w.write_bit(false); // sf 0
    let bytes = w.finish();
    let mut br = BitReader::new(&bytes);
    let cce = parse_cce(&mut br, 3, 2).unwrap();
    assert!(!cce.independent);
    assert_eq!(cce.n_gain_lists, 2);
}

#[test]
fn cpe_both_channels_adds_a_gain_list() {
    let mut w = BitWriter::new();
    w.write(0, 4);
    w.write_bit(false);
    w.write(0, 3); // 1 target
    w.write_bit(true); // CPE
    w.write(2, 4);
    w.write_bit(true); // cc_l
    w.write_bit(true); // cc_r → extra gain list
    w.write_bit(false);
    w.write_bit(false);
    w.write(0, 2);
    silent_ics(&mut w);
    w.write_bit(true);
    w.write_bit(false);
    let cce = parse_ok(&w.finish());
    assert_eq!(cce.n_coupled, 1);
    assert_eq!(cce.n_gain_lists, 2);
}

#[test]
fn truncated_gain_is_unexpected_end() {
    let mut w = BitWriter::new();
    w.write(0, 4);
    w.write_bit(true);
    w.write(5, 3); // 6 targets → 5 extra SF VLCs
    for t in 0..6u8 {
        w.write_bit(false);
        w.write(u32::from(t), 4);
    }
    w.write_bit(false);
    w.write_bit(false);
    w.write(0, 2);
    silent_ics(&mut w);
    // 4+1+3+6×5+4+22 = 64: byte-aligned, no pad that could look like SF 0.
    let bytes = w.finish();
    assert_eq!(bytes.len(), 8);
    let mut br = BitReader::new(&bytes);
    let err = parse_cce(&mut br, 3, 2).expect_err("truncated gain");
    assert!(matches!(err, Error::UnexpectedEnd | Error::HuffmanInvalid));
}

#[test]
fn public_decode_fences_cce() {
    let mut w = BitWriter::new();
    w.write(2, 3); // ID_CCE
    w.write(0, 4);
    w.write_bit(false);
    w.write(0, 3);
    w.write_bit(false);
    w.write(0, 4);
    w.write_bit(false);
    w.write_bit(false);
    w.write(0, 2);
    silent_ics(&mut w);
    w.write(7, 3); // ID_END
    let err = crate::decode(&wrap_adts(&w.finish())).expect_err("CCE");
    let msg = err.to_string();
    assert!(
        msg.contains("UnsupportedCce") || msg.contains("not implemented"),
        "{msg}"
    );
}
