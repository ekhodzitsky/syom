//! TASK-23: independent budgets — exact-boundary, overflow, 32-bit, layouts.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::options::{DEFAULT_MAX_DURATION_SECS, DEFAULT_MAX_INPUT_BYTES, DecodeOptions};

#[test]
fn speech_defaults_unchanged() {
    let o = DecodeOptions::speech();
    assert_eq!(o.channel_mode, crate::ChannelMode::Mono);
    assert_eq!(o.max_duration_secs, DEFAULT_MAX_DURATION_SECS);
    assert_eq!(o.max_decode_sample_rate, 48_000);
    MemoryBudgets::default().validate().unwrap();
}

#[test]
fn finite_complete_uses_one_gib_streaming_feed_does_not() {
    assert!(input_cap_applies(InputScope::FiniteComplete));
    assert!(!input_cap_applies(InputScope::StreamingFeed));
    let b = MemoryBudgets::default();
    assert_eq!(b.max_input_bytes, DEFAULT_MAX_INPUT_BYTES);
    b.check_input_finite(DEFAULT_MAX_INPUT_BYTES).unwrap();
    assert_eq!(
        b.check_input_finite(DEFAULT_MAX_INPUT_BYTES + 1)
            .unwrap_err()
            .kind,
        BudgetKind::Input
    );
    // A lifetime sum above 1 GiB is not a streaming policy; only the
    // resident buffer is.
    b.check_buffered(DEFAULT_MAX_BUFFERED_INPUT_BYTES).unwrap();
    assert!(
        b.check_buffered(DEFAULT_MAX_BUFFERED_INPUT_BYTES + 1)
            .is_err()
    );
}

#[test]
fn output_exact_boundary_and_plus_one() {
    let b = MemoryBudgets::default();
    let samples = DEFAULT_MAX_OUTPUT_BYTES / 4;
    assert_eq!(pcm_bytes(1, samples), Some(DEFAULT_MAX_OUTPUT_BYTES));
    b.check_output(1, samples).unwrap();
    let err = b.check_output(1, samples + 1).unwrap_err();
    assert_eq!(err.kind, BudgetKind::Output);
    assert_eq!(err.max, b.effective_output_bytes());
}

#[test]
fn pcm_bytes_overflow_is_over_budget_not_wrap() {
    let b = MemoryBudgets::default();
    assert!(pcm_bytes(2, u64::MAX).is_none());
    let err = b.check_output(2, u64::MAX).unwrap_err();
    assert_eq!(err.kind, BudgetKind::Output);
    assert_eq!(err.observed, u64::MAX);
}

#[test]
fn channels_zero_and_plus_one() {
    let b = MemoryBudgets::default();
    b.check_channels(1).unwrap();
    b.check_channels(DEFAULT_MAX_CHANNELS).unwrap();
    assert!(b.check_channels(0).is_err());
    assert!(b.check_channels(DEFAULT_MAX_CHANNELS + 1).is_err());
    assert!(b.check_output(DEFAULT_MAX_CHANNELS + 1, 1024).is_err());
}

#[test]
fn lecture_layouts_versus_four_gib() {
    let b = MemoryBudgets::default();
    let samples = 7200u64 * 48_000;
    // speech() mono and stereo split fit; 5.1 split does not (F07).
    assert_eq!(pcm_bytes(1, samples), Some(1_382_400_000));
    assert_eq!(pcm_bytes(2, samples), Some(2_764_800_000));
    assert_eq!(pcm_bytes(6, samples), Some(8_294_400_000));
    b.check_output(1, samples).unwrap();
    b.check_output(2, samples).unwrap();
    assert!(b.check_output(6, samples).is_err());
}

#[test]
fn index_entries_and_metadata_bytes_both_bind() {
    let b = MemoryBudgets::default();
    b.check_index(u64::from(DEFAULT_MAX_INDEX_ENTRIES), 4)
        .unwrap();
    assert!(
        b.check_index(u64::from(DEFAULT_MAX_INDEX_ENTRIES) + 1, 4)
            .is_err()
    );
    // 16 MiB / 32 B = 524_288 entries, under the 1_048_576 entry cap, so
    // the byte budget binds.
    let err = b.check_index(524_289, 32).unwrap_err();
    assert_eq!(err.kind, BudgetKind::Metadata);
    assert!(b.check_index(u64::MAX, 8).is_err());
    b.check_box_depth(DEFAULT_MAX_BOX_DEPTH).unwrap();
    assert!(b.check_box_depth(DEFAULT_MAX_BOX_DEPTH + 1).is_err());
}

#[test]
fn workspace_plateau_is_independent_of_duration() {
    let b = MemoryBudgets::default();
    let lc_mono = workspace_lower_bound_bytes(1, false, false).unwrap();
    let he51 = workspace_lower_bound_bytes(6, true, true).unwrap();
    assert!(lc_mono > DEFAULT_MAX_BUFFERED_INPUT_BYTES);
    assert!(he51 < DEFAULT_MAX_WORKSPACE_BYTES);
    assert!(he51 > lc_mono);
    b.check_workspace(1, false, false).unwrap();
    b.check_workspace(6, true, true).unwrap();
}

#[test]
fn allocable_clips_at_isize_max() {
    assert_eq!(allocable_bytes(0), Some(0));
    assert!(allocable_bytes(isize::MAX as u64).is_some());
    assert!(allocable_bytes((isize::MAX as u64).saturating_add(1)).is_none());
    let err = check_planned(
        BudgetKind::Output,
        (isize::MAX as u64).saturating_add(1),
        u64::MAX,
    )
    .unwrap_err();
    assert_eq!(err.kind, BudgetKind::Output);
    assert_eq!(
        MemoryBudgets::default().effective_output_bytes(),
        DEFAULT_MAX_OUTPUT_BYTES.min(isize::MAX as u64)
    );
}

#[test]
fn zero_budget_is_invalid_not_unlimited() {
    let b = MemoryBudgets {
        max_output_bytes: 0,
        ..MemoryBudgets::default()
    };
    assert!(b.validate().is_err());
    let b = MemoryBudgets {
        max_channels: 0,
        ..MemoryBudgets::default()
    };
    assert!(b.validate().is_err());
}

#[test]
fn declared_au_and_adts_max() {
    let b = MemoryBudgets::default();
    b.check_declared_au(8191).unwrap();
    b.check_declared_au(u64::from(DEFAULT_MAX_DECLARED_AU_BYTES))
        .unwrap();
    assert!(
        b.check_declared_au(u64::from(DEFAULT_MAX_DECLARED_AU_BYTES) + 1)
            .is_err()
    );
}

#[test]
fn unbounded_collection_still_fences_channels_and_input() {
    let b = MemoryBudgets::unbounded_collection();
    b.validate().unwrap();
    b.check_output(6, 7200 * 48_000).unwrap();
    assert_eq!(b.max_input_bytes, DEFAULT_MAX_INPUT_BYTES);
    assert!(b.check_channels(DEFAULT_MAX_CHANNELS + 1).is_err());
}

#[test]
fn sine48_still_decodes_under_speech() {
    let pcm = crate::decode(include_bytes!("goldens/sine48.adts")).unwrap();
    assert_eq!(pcm.sample_rate, 48_000);
    assert_eq!(pcm.channels.len(), 1);
    assert!(!pcm.channels[0].is_empty());
    let samples = pcm.channels[0].len() as u64;
    MemoryBudgets::default().check_output(1, samples).unwrap();
}
