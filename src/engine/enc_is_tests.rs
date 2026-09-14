//! TASK-76: is_pos parse-back and HCB stamp.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::super::bits::{BitReader, BitWriter};
use super::super::ics::WindowSequence;
use super::super::ics_body::parse_ics;
use super::super::section::{INTENSITY_HCB, INTENSITY_HCB2};
use super::super::swb::long_offsets;
use crate::engine::enc_quant::QuantChannel;
use crate::engine::enc_section::{emit_channel_body, plan_books};
use crate::engine::enc_tns::EncTns;

#[test]
fn intensity_section_and_pos_parse_back() {
    let offsets = long_offsets(3).unwrap();
    let n = offsets.len() - 1;
    let mut q = QuantChannel::new(n);
    for b in 0..n {
        q.coded[b] = true;
        q.sf[b] = 100;
        for slot in q.bits[b].iter_mut().skip(1) {
            *slot = 40;
        }
    }
    q.intensity[24] = true;
    q.intensity[25] = true;
    q.is_hcb[24] = INTENSITY_HCB;
    q.is_hcb[25] = INTENSITY_HCB2;
    q.is_pos[24] = 0;
    q.is_pos[25] = 4;
    let books = plan_books(&q);
    assert_eq!(books[24], INTENSITY_HCB);
    assert_eq!(books[25], INTENSITY_HCB2);
    let mut w = BitWriter::new();
    emit_channel_body(
        &mut w,
        offsets,
        WindowSequence::OnlyLong,
        &books,
        &q,
        100,
        true,
        &EncTns::off(),
    );
    let bytes = w.finish();
    let mut br = BitReader::new(&bytes);
    let body = parse_ics(&mut br, 3, 2, None).expect("parse IS channel");
    assert_eq!(body.sections.sfb_cb[0][24], INTENSITY_HCB);
    assert_eq!(body.sections.sfb_cb[0][25], INTENSITY_HCB2);
    assert_eq!(body.sf.is_pos[0][24], 0);
    assert_eq!(body.sf.is_pos[0][25], 4);
}
