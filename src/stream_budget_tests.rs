//! TASK-25: streaming buffer is resident, not a lifetime compressed cap.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::budgets::{BudgetKind, MemoryBudgets};
use crate::{AacError, DecodeOptions, Decoder, decode_with};

const SINE: &[u8] = include_bytes!("goldens/sine48.adts");

fn collect(dec: &mut Decoder, bytes: &[u8]) -> crate::Result<u64> {
    let mut samples = 0u64;
    dec.feed(bytes, |f| {
        samples += f.samples as u64;
        Ok(())
    })?;
    Ok(samples)
}

#[test]
fn feed_does_not_inherit_one_gib_as_lifetime_cap() {
    // max_input_bytes = 10 would reject one-shot SINE; streaming feed
    // must still accept repeated chunks (F07).
    let mem = MemoryBudgets {
        max_input_bytes: 10,
        ..MemoryBudgets::default()
    };
    assert!(decode_with(SINE, &DecodeOptions::speech().with_memory(mem)).is_err());
    let mut dec = Decoder::new(DecodeOptions::speech().with_memory(mem));
    let mut total = 0u64;
    for _ in 0..4 {
        for chunk in SINE.chunks(7) {
            total += collect(&mut dec, chunk).unwrap();
        }
    }
    assert!(total > 0);
}

#[test]
fn one_byte_and_whole_slice_feeds_match() {
    let mut a = Decoder::new(DecodeOptions::speech());
    let mut b = Decoder::new(DecodeOptions::speech());
    let mut sa = 0u64;
    let mut sb = 0u64;
    for chunk in SINE.chunks(1) {
        sa += collect(&mut a, chunk).unwrap();
    }
    sb += collect(&mut b, SINE).unwrap();
    let ia = a
        .finish(|f| {
            sa += f.samples as u64;
            Ok(())
        })
        .unwrap();
    let ib = b
        .finish(|f| {
            sb += f.samples as u64;
            Ok(())
        })
        .unwrap();
    assert_eq!(ia.sample_rate, ib.sample_rate);
    assert_eq!(ia.samples, ib.samples);
    assert_eq!(sa, ia.samples);
    assert_eq!(sb, ib.samples);
}

#[test]
fn tiny_buffered_cap_rejects_oversized_resident_prefix() {
    let mem = MemoryBudgets {
        max_buffered_input_bytes: 8,
        ..MemoryBudgets::default()
    };
    let mut dec = Decoder::new(DecodeOptions::speech().with_memory(mem));
    let err = dec.feed(SINE, |_| Ok(())).unwrap_err();
    assert!(
        matches!(
            err,
            AacError::Limit {
                kind: BudgetKind::Input,
                ..
            }
        ),
        "{err:?}"
    );
}

#[test]
fn declared_au_cap_rejects_before_waiting_for_body() {
    let mem = MemoryBudgets {
        max_declared_au_bytes: 16,
        ..MemoryBudgets::default()
    };
    let mut dec = Decoder::new(DecodeOptions::speech().with_memory(mem));
    let err = dec.feed(SINE, |_| Ok(())).unwrap_err();
    assert!(
        matches!(
            err,
            AacError::Limit {
                kind: BudgetKind::AccessUnit,
                ..
            }
        ),
        "{err:?}"
    );
}

#[test]
fn tiny_workspace_rejects_on_first_frame() {
    let mem = MemoryBudgets {
        max_workspace_bytes: 8,
        ..MemoryBudgets::default()
    };
    let mut dec = Decoder::new(DecodeOptions::speech().with_memory(mem));
    let err = dec.feed(SINE, |_| Ok(())).unwrap_err();
    assert!(
        matches!(
            err,
            AacError::Limit {
                kind: BudgetKind::Workspace,
                ..
            }
        ),
        "{err:?}"
    );
}

#[test]
fn repeated_feeds_keep_decoding_without_lifetime_input_error() {
    let mut dec = Decoder::new(DecodeOptions::speech());
    let mut samples = 0u64;
    for _ in 0..32 {
        samples += collect(&mut dec, SINE).unwrap();
    }
    let info = dec.finish(|_| Ok(())).unwrap();
    assert_eq!(info.sample_rate, 48_000);
    assert_eq!(samples, info.samples);
}

#[test]
fn he_chunked_feed_matches_oneshot() {
    const HE: &[u8] = include_bytes!("goldens/he48.adts");
    let oneshot = decode_with(HE, &DecodeOptions::unbounded()).unwrap();
    let mut dec = Decoder::new(DecodeOptions::unbounded());
    let mut samples = 0u64;
    for chunk in HE.chunks(9) {
        samples += collect(&mut dec, chunk).unwrap();
    }
    let info = dec
        .finish(|f| {
            samples += f.samples as u64;
            Ok(())
        })
        .unwrap();
    assert_eq!(info.sample_rate, oneshot.sample_rate);
    assert_eq!(info.samples as usize, oneshot.channels[0].len());
    assert_eq!(samples, info.samples);
}
