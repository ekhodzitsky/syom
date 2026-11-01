//! Compare exact integer costs, including overflow, missing books and ties.
use super::*;

#[test]
fn prefix_matches_original_range_walks_and_book_selection() {
    let mut state = 0x5a7b_7719u32;
    for n in [0, 1, 15, 31, 49, MAX_BANDS] {
        for _ in 0..16 {
            let mut coded = [false; MAX_BANDS];
            let mut bits = [[UNREPRESENTABLE; BOOKS]; MAX_BANDS];
            for b in 0..n {
                state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                coded[b] = !state.is_multiple_of(3);
                for cost in &mut bits[b][1..] {
                    state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                    *cost = match state % 8 {
                        0 => UNREPRESENTABLE,
                        1 => UNREPRESENTABLE - 1,
                        _ => state % 7,
                    };
                }
            }
            let prefix = CostPrefix::new(&coded, &bits, n);
            for lo in 0..=n {
                for hi in lo..=n {
                    assert_eq!(
                        prefix.best(lo, hi),
                        super::super::best_book(&coded, &bits, lo, hi)
                    );
                    for cb in 1..BOOKS {
                        assert_eq!(
                            prefix.range(lo, hi, cb),
                            super::super::range_book_bits(&coded, &bits, lo, hi, cb)
                        );
                    }
                }
            }
        }
    }
}
