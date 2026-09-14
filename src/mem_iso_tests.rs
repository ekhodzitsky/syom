//! Heap tracker semantics (TASK-12). Does not spawn peer processes.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::mem_iso::{HeapTracker, MemReport};

#[test]
fn live_and_peak_track_alloc_and_free() {
    let mut t = HeapTracker::default();
    t.alloc(1000);
    t.alloc(500);
    assert_eq!(t.live, 1500);
    assert_eq!(t.peak_live, 1500);
    assert_eq!(t.allocs, 2);
    t.free(500);
    assert_eq!(t.live, 1000);
    assert_eq!(t.peak_live, 1500, "peak is not lowered by free");
    t.alloc(2000);
    assert_eq!(t.peak_live, 3000);
    assert_eq!(t.live, 3000);
}

#[test]
fn report_json_separates_rss_live_and_retained() {
    let r = MemReport {
        peer: "syom".into(),
        case: "lc_adts".into(),
        phase: "one_shot".into(),
        baseline_rss: 10,
        peak_rss: 40,
        peak_rss_delta: 30,
        peak_live: 20,
        allocs: 7,
        alloc_bytes: 99,
        retained: 4,
    };
    let j = r.to_json();
    assert!(j.contains("\"peer\":\"syom\""));
    assert!(j.contains("baseline_rss"));
    assert!(j.contains("peak_rss_delta"));
    assert!(j.contains("peak_live"));
    assert!(j.contains("retained"));
    assert!(j.contains("one_shot"));
}

#[test]
fn reverse_order_does_not_change_tracker_math() {
    let seq = [100u64, 200, 50];
    let mut a = HeapTracker::default();
    for s in seq {
        a.alloc(s);
    }
    for s in seq.iter().rev() {
        a.free(*s);
    }
    let mut b = HeapTracker::default();
    for s in seq.iter().rev().copied() {
        b.alloc(s);
    }
    for s in seq {
        b.free(s);
    }
    assert_eq!(a.live, 0);
    assert_eq!(b.live, 0);
    assert_eq!(a.peak_live, b.peak_live);
    assert_eq!(a.alloc_bytes, b.alloc_bytes);
}
