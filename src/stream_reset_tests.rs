//! TASK-77: reset reuses workspace and matches a fresh Decoder.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::stream_tests::{assert_eq_collected, collect_into};
use super::{DecodeOptions, Decoder, Layout, Result, decode_with};

const LC: &[u8] = include_bytes!("goldens/sine48.adts");
const HE: &[u8] = include_bytes!("goldens/he48.adts");
const PS: &[u8] = include_bytes!("goldens/ps48.adts");
const MC: &[u8] = include_bytes!("goldens/mc51.adts");

#[test]
fn reset_lc_matches_fresh() -> Result<()> {
    let opts = DecodeOptions::unbounded();
    let mut dec = Decoder::new(opts.clone());
    let first = collect_into(&mut dec, LC)?;
    dec.reset();
    let second = collect_into(&mut dec, LC)?;
    let fresh = decode_with(LC, &opts)?;
    assert_eq_collected(&fresh, &first, "first");
    assert_eq_collected(&fresh, &second, "reset");
    Ok(())
}

#[test]
fn reset_after_he_does_not_leak_into_lc() -> Result<()> {
    let opts = DecodeOptions::unbounded();
    let mut dec = Decoder::new(opts.clone());
    let _ = collect_into(&mut dec, HE)?;
    dec.reset();
    let got = collect_into(&mut dec, LC)?;
    let fresh = decode_with(LC, &opts)?;
    assert_eq_collected(&fresh, &got, "he-then-lc");
    Ok(())
}

#[test]
fn reset_after_ps_does_not_leak_into_lc() -> Result<()> {
    let opts = DecodeOptions::unbounded();
    let mut dec = Decoder::new(opts.clone());
    let _ = collect_into(&mut dec, PS)?;
    dec.reset();
    let got = collect_into(&mut dec, LC)?;
    let fresh = decode_with(LC, &opts)?;
    assert_eq_collected(&fresh, &got, "ps-then-lc");
    Ok(())
}

#[test]
fn reset_after_mc_does_not_leak_layout() -> Result<()> {
    let opts = DecodeOptions::unbounded();
    let mut dec = Decoder::new(opts.clone());
    let _ = collect_into(&mut dec, MC)?;
    dec.reset();
    let mut layout = None;
    dec.feed(LC, |f| {
        if layout.is_none() {
            layout = Some(f.meta.layout);
        }
        Ok(())
    })?;
    dec.finish(|_| Ok(()))?;
    assert_eq!(layout, Some(Layout::Mpeg(1)));
    Ok(())
}

#[test]
fn reset_keeps_pcm_and_buf_capacity() -> Result<()> {
    let opts = DecodeOptions::unbounded();
    let mut dec = Decoder::new(opts);
    let _ = collect_into(&mut dec, LC)?;
    let pcm = dec.test_pcm_cap();
    let buf = dec.test_buf_cap();
    assert!(pcm > 0, "first session must grow pcm workspace");
    dec.reset();
    assert!(
        dec.test_pcm_cap() >= pcm,
        "reset must not drop pcm capacity ({pcm})"
    );
    assert!(
        dec.test_buf_cap() >= buf,
        "reset must not drop buf capacity ({buf})"
    );
    Ok(())
}

#[test]
fn two_decoders_reset_are_reentrant() -> Result<()> {
    let opts = DecodeOptions::unbounded();
    let mut a = Decoder::new(opts.clone());
    let mut b = Decoder::new(opts.clone());
    let _ = collect_into(&mut a, HE)?;
    let _ = collect_into(&mut b, MC)?;
    a.reset();
    b.reset();
    let ga = collect_into(&mut a, LC)?;
    let gb = collect_into(&mut b, LC)?;
    let fresh = decode_with(LC, &opts)?;
    assert_eq_collected(&fresh, &ga, "a");
    assert_eq_collected(&fresh, &gb, "b");
    Ok(())
}
