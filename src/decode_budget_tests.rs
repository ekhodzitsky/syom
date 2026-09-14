//! TASK-24: output and metadata budgets fire before allocation.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::budgets::{BudgetKind, MemoryBudgets, pcm_bytes};
use crate::isomp4::parse_aac_track_with;
use crate::{AacError, DecodeOptions, decode_with};

const SINE: &[u8] = include_bytes!("goldens/sine48.adts");
const LECTURE: &[u8] = include_bytes!("goldens/lecture.m4a");

fn opts_with(mem: MemoryBudgets) -> DecodeOptions {
    DecodeOptions::speech().with_memory(mem)
}

#[test]
fn exact_output_limit_still_decodes_sine() {
    let pcm = crate::decode(SINE).unwrap();
    let samples = pcm.channels[0].len() as u64;
    let bytes = pcm_bytes(1, samples).unwrap();
    let mem = MemoryBudgets {
        max_output_bytes: bytes,
        ..MemoryBudgets::default()
    };
    let got = decode_with(SINE, &opts_with(mem)).unwrap();
    assert_eq!(got.sample_rate, 48_000);
    assert_eq!(got.channels[0].len() as u64, samples);
    assert!(got.channels[0].iter().all(|x| x.is_finite()));
}

#[test]
fn output_plus_one_is_limit_not_partial_success() {
    let pcm = crate::decode(SINE).unwrap();
    let samples = pcm.channels[0].len() as u64;
    let bytes = pcm_bytes(1, samples).unwrap();
    let mem = MemoryBudgets {
        max_output_bytes: bytes.saturating_sub(4),
        ..MemoryBudgets::default()
    };
    let err = decode_with(SINE, &opts_with(mem)).unwrap_err();
    assert!(
        matches!(
            err,
            AacError::Limit {
                kind: BudgetKind::Output,
                ..
            }
        ),
        "{err:?}"
    );
}

#[test]
fn collected_pcm_stays_inside_accounting() {
    let mem = MemoryBudgets::default();
    let pcm = decode_with(SINE, &opts_with(mem)).unwrap();
    let bytes = pcm_bytes(1, pcm.channels[0].len() as u64).unwrap();
    assert!(bytes <= mem.effective_output_bytes());
    assert!(bytes <= mem.max_output_bytes);
}

#[test]
fn tiny_index_budget_rejects_lecture_m4a() {
    let mem = MemoryBudgets {
        max_index_entries: 1,
        ..MemoryBudgets::default()
    };
    let err = parse_aac_track_with(LECTURE, &mem).unwrap_err();
    assert!(
        matches!(
            err,
            AacError::Limit {
                kind: BudgetKind::Metadata,
                ..
            }
        ),
        "{err:?}"
    );
}

#[test]
fn tiny_box_depth_rejects_lecture_m4a() {
    let mem = MemoryBudgets {
        max_box_depth: 1,
        ..MemoryBudgets::default()
    };
    let err = parse_aac_track_with(LECTURE, &mem).unwrap_err();
    assert!(
        matches!(
            err,
            AacError::Limit {
                kind: BudgetKind::Metadata,
                ..
            }
        ),
        "{err:?}"
    );
}

#[test]
fn overflow_pcm_count_is_limit() {
    let mem = MemoryBudgets::default();
    let err = mem.check_output(2, u64::MAX).unwrap_err();
    assert_eq!(err.kind, BudgetKind::Output);
    assert_eq!(err.observed, u64::MAX);
}

#[test]
fn zero_output_budget_is_invalid_limits() {
    let mem = MemoryBudgets {
        max_output_bytes: 0,
        ..MemoryBudgets::default()
    };
    let err = decode_with(SINE, &opts_with(mem)).unwrap_err();
    assert!(matches!(err, AacError::InvalidLimits(_)), "{err:?}");
}

#[test]
fn speech_sine_unchanged() {
    let a = crate::decode(SINE).unwrap();
    let b = decode_with(SINE, &DecodeOptions::speech()).unwrap();
    assert_eq!(a.sample_rate, b.sample_rate);
    assert_eq!(a.channels[0].len(), b.channels[0].len());
}
