//! TASK-75: noise_nrg mapping and scale-factor parse-back.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::super::bits::{BitReader, BitWriter};
use super::super::ics::WindowSequence;
use super::super::ics_body::parse_ics;
use super::super::pns::{self, Lcg};
use super::super::section::NOISE_HCB;
use super::super::swb::long_offsets;
use super::nrg_from_energy;
use crate::engine::enc_quant::QuantChannel;
use crate::engine::enc_section::{channel_body_bits, emit_channel_body, plan_books};
use crate::engine::enc_tns::EncTns;

#[test]
fn nrg_from_energy_matches_decoder_l2() {
    // Decoder: L2 = 2^(nrg/4). Encoder: nrg = round(2 log2 E) so L2 ≈ √E.
    for e in [1.0f32, 16.0, 256.0, 1.0e6] {
        let nrg = nrg_from_energy(e);
        let l2 = det_l2(nrg);
        let want = e.sqrt();
        let rel = (l2 - want).abs() / want.max(1e-6);
        assert!(rel < 0.2, "E={e} nrg={nrg} L2={l2} want={want} rel={rel}");
    }
    assert_eq!(nrg_from_energy(0.0), 0);
}

fn det_l2(nrg: i32) -> f32 {
    (0.25 * nrg as f32).exp2()
}

#[test]
fn pns_section_and_nrg_parse_back() {
    let offsets = long_offsets(3).unwrap();
    let n = offsets.len() - 1;
    let mut q = QuantChannel::new(n);
    for b in 0..n {
        q.coded[b] = b % 4 != 3;
        q.sf[b] = 100;
        if q.coded[b] {
            for (_cb, slot) in q.bits[b].iter_mut().enumerate().skip(1) {
                *slot = 40;
            }
        }
    }
    // Stamp a few mid-HF bands as PNS with a representable 9-bit nrg.
    q.pns[20] = true;
    q.pns[21] = true;
    q.noise_nrg[20] = 40;
    q.noise_nrg[21] = 44;
    let books = plan_books(&q);
    assert_eq!(books[20], NOISE_HCB);
    assert_eq!(books[21], NOISE_HCB);
    assert_ne!(books[19], NOISE_HCB);
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
    let _ = channel_body_bits(&books, &q, 100, true, &EncTns::off());
    let bytes = w.finish();
    let mut br = BitReader::new(&bytes);
    let body = parse_ics(&mut br, 3, 2, None).expect("parse PNS channel");
    assert_eq!(body.sections.sfb_cb[0][20], NOISE_HCB);
    assert_eq!(body.sections.sfb_cb[0][21], NOISE_HCB);
    assert_eq!(body.sf.noise_nrg[0][20], 40);
    assert_eq!(body.sf.noise_nrg[0][21], 44);
    // Independent decoder energy: L2 = 2^(nrg/4) on the PNS band.
    let mut spec = vec![0.0f32; 1024];
    let mut rng = Lcg::new();
    pns::apply(
        &mut spec,
        &body.ics,
        &body.sections,
        &body.sf,
        3,
        &mut rng,
        None,
    )
    .unwrap();
    let lo = usize::from(offsets[20]);
    let hi = usize::from(offsets[21]);
    let e: f32 = spec[lo..hi].iter().map(|x| x * x).sum();
    let want = det_l2(40);
    assert!(
        (e.sqrt() - want).abs() < 1e-3,
        "PNS L2 {} want {want}",
        e.sqrt()
    );
}
