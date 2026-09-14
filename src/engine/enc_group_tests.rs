//! Grouping bits round-trip and energy-merge behaviour.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::Grouping;
use crate::engine::ics::{IcsInfo, WindowSequence, grouping_of};
use crate::engine::swb::LONG_WINDOW_LEN;

#[test]
fn ungrouped_is_zero_bits_eight_ones() {
    let g = Grouping::ungrouped();
    assert_eq!(g.n_groups, 8);
    assert_eq!(g.group_len, [1; 8]);
    assert_eq!(g.bits(), 0);
    assert_eq!(Grouping::from_bits(0), g);
}

#[test]
fn from_bits_matches_ics_grouping_of() {
    for bits in 0u8..=127 {
        let g = Grouping::from_bits(bits);
        let (_, n, lens) = grouping_of(WindowSequence::EightShort, Some(bits));
        assert_eq!(g.n_groups, n, "bits={bits:#04x}");
        assert_eq!(g.group_len, lens, "bits={bits:#04x}");
        // Reconstructing bits from lengths must parse back to the same groups.
        let again = Grouping::from_bits(g.bits());
        assert_eq!(again.n_groups, g.n_groups);
        assert_eq!(
            &again.group_len[..g.n_groups as usize],
            &g.group_len[..g.n_groups as usize]
        );
    }
}

#[test]
fn one_group_of_eight_is_all_merge_bits() {
    let g = Grouping::from_bits(0b111_1111);
    assert_eq!(g.n_groups, 1);
    assert_eq!(g.group_len[0], 8);
    assert_eq!(g.bits(), 0b111_1111);
}

#[test]
fn four_pairs() {
    // Merge 1-0, 3-2, 5-4, 7-6: bits 6,4,2,0 → 0b1010101
    let g = Grouping::from_bits(0b101_0101);
    assert_eq!(g.n_groups, 4);
    assert_eq!(&g.group_len[..4], &[2, 2, 2, 2]);
}

#[test]
fn decide_merges_equal_energy_and_splits_an_impulse() {
    let mut spec = [[0.0f32; LONG_WINDOW_LEN]; 1];
    // Windows 0–3: same quiet tone energy; window 4: impulse; 5–7 quiet.
    for w in 0..8 {
        let lo = w * 128;
        for i in 0..128 {
            spec[0][lo + i] = 0.01;
        }
    }
    for i in 0..8 {
        spec[0][4 * 128 + i] = 20.0;
    }
    let g = Grouping::decide(&spec, 1, &[]);
    assert!(g.n_groups >= 3, "impulse must open a new group: {g:?}");
    // The impulse window is not merged with both neighbors into one group.
    let mut w = 0u8;
    let mut impulse_len = 0u8;
    for g_i in 0..g.n_groups as usize {
        let len = g.group_len[g_i];
        if w <= 4 && 4 < w + len {
            impulse_len = len;
        }
        w += len;
    }
    assert!(impulse_len <= 2, "impulse group too wide: {g:?}");
}

#[test]
fn decide_all_quiet_is_one_group() {
    let spec = [[0.0f32; LONG_WINDOW_LEN]; 1];
    let g = Grouping::decide(&spec, 1, &[]);
    assert_eq!(g.n_groups, 1);
    assert_eq!(g.group_len[0], 8);
}

#[test]
fn ics_parse_reads_emitted_bits() {
    use crate::engine::bits::{BitReader, BitWriter};
    use crate::engine::enc_section::emit_ics_info;
    for &bits in &[0u8, 0x7f, 0x55, 0x01, 0x40] {
        let mut w = BitWriter::new();
        emit_ics_info(&mut w, WindowSequence::EightShort, 14, bits);
        let bytes = w.finish();
        let mut br = BitReader::new(&bytes);
        let ics = IcsInfo::parse(&mut br, 3, false).expect("ics");
        let g = Grouping::from_bits(bits);
        assert_eq!(ics.num_window_groups, g.n_groups);
        assert_eq!(
            &ics.window_group_length[..g.n_groups as usize],
            &g.group_len[..g.n_groups as usize]
        );
    }
}
