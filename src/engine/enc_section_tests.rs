//! Section planning + emission, parsed back by the shipped section/sf/sf_tab
//! machinery.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::super::bits::{BitReader, BitWriter};
use super::super::ics::{IcsInfo, WindowSequence};
use super::super::section::SectionData;
use super::super::sf::{self, ScaleFactors};
use super::super::swb::long_offsets;
use super::{
    channel_body_bits, emit_channel_body, emit_ics_info, emit_section_data, plan_books,
    plan_books_dp, plan_books_into, range_book_bits, section_header_bits, section_spectral_cost,
};
use crate::engine::enc_quant::QuantChannel;
use crate::engine::enc_quant::{BOOKS, UNREPRESENTABLE};

/// A channel with alternating loud/quiet bands so planning is non-trivial.
fn sample_channel(n_bands: usize) -> QuantChannel {
    let mut q = QuantChannel::new(n_bands);
    for b in 0..n_bands {
        q.coded[b] = b % 3 != 2;
        q.sf[b] = 100 + (b as i32 % 7) * 8;
        if q.coded[b] {
            for (cb, slot) in q.bits[b].iter_mut().enumerate().skip(1) {
                *slot = (20 + b * 3 + cb * cb) as u32;
            }
        }
    }
    q
}

#[test]
fn plan_books_never_picks_unrepresentable() {
    let q = sample_channel(49);
    let books = plan_books(&q);
    for (b, (&book, &coded)) in books.iter().zip(q.coded.iter()).enumerate().take(49) {
        if coded {
            let cb = usize::from(book);
            assert!((1..=11).contains(&cb));
            assert_ne!(
                q.bits[b][cb],
                crate::engine::enc_quant::UNREPRESENTABLE,
                "band {b} planned on an unrepresentable book"
            );
        } else {
            assert_eq!(book, 0);
        }
    }
}

#[test]
fn section_data_parses_back() {
    let q = sample_channel(49);
    let books = plan_books(&q);
    let mut w = BitWriter::new();
    emit_ics_info(&mut w, WindowSequence::OnlyLong, 49, 0);
    emit_section_data(&mut w, &books, 49);
    let bytes = w.finish();
    let mut br = BitReader::new(&bytes);
    let ics = IcsInfo::parse(&mut br, 3, false).expect("ics_info");
    assert_eq!(ics.max_sfb, 49);
    assert!(!ics.window_sequence.is_eight_short());
    let sections = SectionData::parse(&mut br, &ics).expect("section_data");
    assert_eq!(sections.sfb_cb.len(), 1, "long window: one group");
    assert_eq!(&sections.sfb_cb[0], &books[..49]);
}

#[test]
fn long_run_escapes_at_31() {
    // All bands one book: a single 49-band run must use the 31 escape.
    let mut q = QuantChannel::new(49);
    for b in 0..49 {
        q.coded[b] = true;
        q.sf[b] = 120;
        q.bits[b] = [crate::engine::enc_quant::UNREPRESENTABLE; 12];
        q.bits[b][3] = 10; // book 3 cheapest everywhere
    }
    let books = plan_books(&q);
    assert!(books[..49].iter().all(|&b| b == 3));
    let mut w = BitWriter::new();
    emit_section_data(&mut w, &books, 49);
    let bytes = w.finish();
    let mut w2 = BitWriter::new();
    emit_ics_info(&mut w2, WindowSequence::OnlyLong, 49, 0);
    let ics_bytes = w2.finish();
    let mut br2 = BitReader::new(&ics_bytes);
    let ics = IcsInfo::parse(&mut br2, 3, false).expect("ics");
    let mut br3 = BitReader::new(&bytes);
    let sections = SectionData::parse(&mut br3, &ics).expect("sections");
    assert_eq!(sections.sfb_cb[0], vec![3u8; 49]);
}

#[test]
fn channel_body_bits_match_emit() {
    let offsets = long_offsets(3).expect("offsets");
    let mut q = sample_channel(49);
    // Real quant values so the spectral walk is exercised.
    for (i, v) in q.quant.iter_mut().enumerate() {
        *v = ((i * 7) % 5) as i32 - 2;
    }
    for b in 0..49 {
        if q.coded[b] {
            // Recompute the cost table against the actual tuples.
            for cb in 1..12usize {
                let step = if cb <= 4 { 4 } else { 2 };
                let (lo, hi) = (usize::from(offsets[b]), usize::from(offsets[b + 1]));
                let mut total = 0u32;
                let mut i = lo;
                let mut ok = true;
                while i < hi {
                    match crate::engine::enc_huff::spectral_bits(cb as u8, &q.quant[i..i + step]) {
                        Some(bits) => total += bits as u32,
                        None => {
                            ok = false;
                            break;
                        }
                    }
                    i += step;
                }
                q.bits[b][cb] = if ok {
                    total
                } else {
                    crate::engine::enc_quant::UNREPRESENTABLE
                };
            }
        }
    }
    let books = plan_books(&q);
    let mut w = BitWriter::new();
    emit_channel_body(
        &mut w,
        offsets,
        WindowSequence::OnlyLong,
        &books,
        &q,
        100,
        true,
        &crate::engine::enc_tns::EncTns::off(),
    );
    let counted = channel_body_bits(
        &books,
        &q,
        100,
        true,
        &crate::engine::enc_tns::EncTns::off(),
    );
    assert_eq!(
        w.bit_len() as usize,
        counted,
        "counted bits must match emitted bits exactly"
    );
    // And the emitted body must parse: sf DPCM over the same plan.
    let bytes = w.finish();
    let mut br = BitReader::new(&bytes);
    let gg = br.read(8).expect("gg") as u8;
    assert_eq!(gg, 100);
    let ics = IcsInfo::parse(&mut br, 3, false).expect("ics");
    let sections = SectionData::parse(&mut br, &ics).expect("sections");
    let mut sf = ScaleFactors::default();
    sf::parse_into(&mut br, &ics, &sections.sfb_cb, gg, &mut sf).expect("sf");
    for b in 0..49 {
        if q.coded[b] {
            assert_eq!(sf.sf[0][b], q.sf[b], "band {b} scalefactor");
        }
    }
}

/// Independent exhaustive min cost of a coded stretch (same recurrence as
/// production DP, not calling `plan_stretch_dp`).
fn oracle_stretch_cost(
    coded: &[bool],
    bits: &[[u32; BOOKS]],
    lo: usize,
    hi: usize,
    header_bits: fn(usize) -> usize,
) -> u32 {
    let n = hi - lo;
    let mut dp = vec![u32::MAX; n + 1];
    dp[0] = 0;
    for i in 1..=n {
        for j in 0..i {
            for cb in 1..BOOKS {
                let spec = range_book_bits(coded, bits, lo + j, lo + i, cb);
                if spec == UNREPRESENTABLE {
                    continue;
                }
                let c = dp[j]
                    .saturating_add(spec)
                    .saturating_add(header_bits(i - j) as u32);
                if c < dp[i] {
                    dp[i] = c;
                }
            }
        }
    }
    dp[n]
}

fn lcg_channel(n: usize, seed: u32) -> QuantChannel {
    let mut q = QuantChannel::new(n);
    let mut s = seed;
    for b in 0..n {
        s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        q.coded[b] = !s.is_multiple_of(5);
        q.sf[b] = 80 + (s % 40) as i32;
        if q.coded[b] {
            for cb in 1..BOOKS {
                s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                q.bits[b][cb] = 8 + (s % 80);
            }
            // Make one book clearly cheap, another expensive.
            q.bits[b][1 + (b % 10)] = 4;
        }
    }
    q
}

#[test]
fn dp_matches_independent_oracle_on_small_stretches() {
    for seed in [1u32, 7, 99, 12345] {
        let q = lcg_channel(8, seed);
        let mut dp = [0u8; 8];
        plan_books_dp(&q.coded, &q.bits, 8, &mut dp, section_header_bits);
        let got = section_spectral_cost(&dp, &q.coded, &q.bits, 8, section_header_bits);
        // Oracle over each coded stretch, summed with ZERO headers.
        let mut lo = 0usize;
        let mut oracle = 0u64;
        while lo < 8 {
            if !q.coded[lo] {
                // ZERO section run
                let mut hi = lo + 1;
                while hi < 8 && !q.coded[hi] {
                    hi += 1;
                }
                oracle += section_header_bits(hi - lo) as u64;
                lo = hi;
                continue;
            }
            let mut hi = lo + 1;
            while hi < 8 && q.coded[hi] {
                hi += 1;
            }
            oracle += u64::from(oracle_stretch_cost(
                &q.coded,
                &q.bits,
                lo,
                hi,
                section_header_bits,
            ));
            lo = hi;
        }
        assert_eq!(got, oracle, "seed {seed}");
        let short_hdr = |len: usize| 4 + 3 * (len / 7 + 1);
        let mut dp_s = [0u8; 8];
        plan_books_dp(&q.coded, &q.bits, 8, &mut dp_s, short_hdr);
        let got_s = section_spectral_cost(&dp_s, &q.coded, &q.bits, 8, short_hdr);
        let mut lo = 0usize;
        let mut oracle_s = 0u64;
        while lo < 8 {
            if !q.coded[lo] {
                let mut hi = lo + 1;
                while hi < 8 && !q.coded[hi] {
                    hi += 1;
                }
                oracle_s += short_hdr(hi - lo) as u64;
                lo = hi;
                continue;
            }
            let mut hi = lo + 1;
            while hi < 8 && q.coded[hi] {
                hi += 1;
            }
            oracle_s += u64::from(oracle_stretch_cost(&q.coded, &q.bits, lo, hi, short_hdr));
            lo = hi;
        }
        assert_eq!(got_s, oracle_s, "short-hdr seed {seed}");
    }
}

#[test]
fn dp_never_more_expensive_than_greedy() {
    let mut saved = 0u64;
    let mut base = 0u64;
    for seed in 0u32..64 {
        let n = 49;
        let q = lcg_channel(n, 0xC0FF_EE00 ^ seed);
        let mut dp = [0u8; 51];
        let mut gr = [0u8; 51];
        plan_books_dp(&q.coded, &q.bits, n, &mut dp, section_header_bits);
        plan_books_into(&q.coded, &q.bits, n, &mut gr, section_header_bits);
        let c_dp = section_spectral_cost(&dp, &q.coded, &q.bits, n, section_header_bits);
        let c_gr = section_spectral_cost(&gr, &q.coded, &q.bits, n, section_header_bits);
        assert!(c_dp <= c_gr, "seed {seed}: dp {c_dp} > greedy {c_gr}");
        saved += c_gr - c_dp;
        base += c_gr;
    }
    let pct = if base == 0 {
        0.0
    } else {
        100.0 * saved as f64 / base as f64
    };
    eprintln!("TASK-73 DP vs greedy on 64 LCG long channels: saved {saved}/{base} ({pct:.2}%)");
    let q = sample_channel(49);
    let mut dp = [0u8; 51];
    let mut gr = [0u8; 51];
    plan_books_dp(&q.coded, &q.bits, 49, &mut dp, section_header_bits);
    plan_books_into(&q.coded, &q.bits, 49, &mut gr, section_header_bits);
    let c_dp = section_spectral_cost(&dp, &q.coded, &q.bits, 49, section_header_bits);
    let c_gr = section_spectral_cost(&gr, &q.coded, &q.bits, 49, section_header_bits);
    eprintln!("TASK-73 sample_channel 49: greedy {c_gr} dp {c_dp}");
    assert!(c_dp <= c_gr);
}

#[test]
fn escape_book_and_zero_holes_parse() {
    let mut q = QuantChannel::new(12);
    for b in 0..12 {
        q.coded[b] = b % 4 != 3; // holes
        q.sf[b] = 110;
        q.bits[b] = [UNREPRESENTABLE; BOOKS];
        if q.coded[b] {
            q.bits[b][11] = 30; // only book 11
            if b % 2 == 0 {
                q.bits[b][5] = 40;
            }
        }
    }
    let books = plan_books(&q);
    for (b, (&book, &coded)) in books.iter().zip(q.coded.iter()).enumerate().take(12) {
        if coded {
            assert!(book == 11 || book == 5, "band {b} book {book}");
            assert_ne!(q.bits[b][usize::from(book)], UNREPRESENTABLE);
        } else {
            assert_eq!(book, 0);
        }
    }
    let mut w = BitWriter::new();
    emit_section_data(&mut w, &books, 12);
    let mut w2 = BitWriter::new();
    emit_ics_info(&mut w2, WindowSequence::OnlyLong, 12, 0);
    let ics = IcsInfo::parse(&mut BitReader::new(&w2.finish()), 3, false).expect("ics");
    let sections = SectionData::parse(&mut BitReader::new(&w.finish()), &ics).expect("sec");
    assert_eq!(&sections.sfb_cb[0][..12], &books[..12]);
}
