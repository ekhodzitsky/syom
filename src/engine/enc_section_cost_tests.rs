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

#[test]
fn uncoded_bands_do_not_require_cost_rows() {
    let coded = [false; MAX_BANDS];
    let prefix = CostPrefix::new(&coded, &[], MAX_BANDS);
    assert_eq!(prefix.best(0, MAX_BANDS), Some((1, 0)));
    for cb in 0..BOOKS {
        assert_eq!(prefix.range(0, MAX_BANDS, cb), 0);
    }
    let mut books = [99; MAX_BANDS];
    super::super::plan_books_into(
        &coded,
        &[],
        MAX_BANDS,
        &mut books,
        super::super::section_header_bits,
    );
    assert_eq!(books, [0; MAX_BANDS]);
}

#[test]
fn omitted_trailing_uncoded_rows_preserve_previous_prefix_totals() {
    let coded = [true, false, false];
    let mut row = [UNREPRESENTABLE; BOOKS];
    row[2] = 9;
    row[3] = 9;
    row[11] = 12;
    let bits = [row];
    let prefix = CostPrefix::new(&coded, &bits, coded.len());
    for lo in 0..=coded.len() {
        for hi in lo..=coded.len() {
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
    let mut books = [99; 3];
    super::super::plan_books_into(
        &coded,
        &bits,
        coded.len(),
        &mut books,
        super::super::section_header_bits,
    );
    assert_eq!(books, [2, 0, 0]);
}

#[test]
#[should_panic]
fn missing_coded_cost_rows_remain_invalid() {
    let _ = CostPrefix::new(&[false, true], &[], 2);
}
