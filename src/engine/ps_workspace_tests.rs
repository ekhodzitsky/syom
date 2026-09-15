//! TASK-80: PS analysis/decorrelation/output workspace reuse.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::{DecodeOptions, Decoder, decode_with};

const PS: &[u8] = include_bytes!("../goldens/ps48.adts");
const LC: &[u8] = include_bytes!("../goldens/sine48.adts");

fn push_planes(dst: &mut Vec<Vec<f32>>, planes: &[&[f32]]) {
    if dst.len() < planes.len() {
        dst.resize(planes.len(), Vec::new());
    }
    for (d, src) in dst.iter_mut().zip(planes.iter()) {
        d.extend_from_slice(src);
    }
}

fn collect(bytes: &[u8], opts: DecodeOptions) -> crate::Result<(u32, Vec<Vec<f32>>)> {
    let mut planes: Vec<Vec<f32>> = Vec::new();
    let mut dec = Decoder::new(opts);
    dec.feed(bytes, |f| {
        push_planes(&mut planes, f.planar);
        Ok(())
    })?;
    let info = dec.finish(|f| {
        push_planes(&mut planes, f.planar);
        Ok(())
    })?;
    Ok((info.sample_rate, planes))
}

#[test]
fn ps48_streaming_matches_oneshot_split_and_speech() -> crate::Result<()> {
    for opts in [DecodeOptions::unbounded(), DecodeOptions::speech()] {
        let owned = decode_with(PS, &opts)?;
        let (rate, planes) = collect(PS, opts)?;
        assert_eq!(rate, owned.sample_rate);
        assert_eq!(planes.len(), owned.channels.len());
        for (a, b) in planes.iter().zip(owned.channels.iter()) {
            assert_eq!(a, b);
        }
        assert!(planes.iter().all(|p| p.iter().all(|x| x.is_finite())));
    }
    Ok(())
}

#[test]
fn ps48_reset_matches_fresh_and_does_not_leak_into_lc() -> crate::Result<()> {
    let opts = DecodeOptions::unbounded();
    let mut dec = Decoder::new(DecodeOptions::unbounded());
    dec.feed(PS, |_| Ok(()))?;
    let _ = dec.finish(|_| Ok(()))?;
    dec.reset();
    let mut second = Vec::new();
    dec.feed(PS, |f| {
        push_planes(&mut second, f.planar);
        Ok(())
    })?;
    let _ = dec.finish(|f| {
        push_planes(&mut second, f.planar);
        Ok(())
    })?;
    let owned = decode_with(PS, &opts)?;
    assert_eq!(second.len(), owned.channels.len());
    for (a, b) in second.iter().zip(owned.channels.iter()) {
        assert_eq!(a, b, "reset PS session must match a fresh decoder");
    }

    dec.reset();
    let mut after = Vec::new();
    dec.feed(LC, |f| {
        push_planes(&mut after, f.planar);
        Ok(())
    })?;
    let info = dec.finish(|f| {
        push_planes(&mut after, f.planar);
        Ok(())
    })?;
    assert_eq!(info.sample_rate, 48_000);
    assert_eq!(after.len(), 1, "PS state must not leak into LC");
    Ok(())
}

#[test]
fn two_ps_decoders_do_not_alias() -> crate::Result<()> {
    let mut a = Decoder::new(DecodeOptions::unbounded());
    let mut b = Decoder::new(DecodeOptions::unbounded());
    let mut pa = Vec::new();
    let mut pb = Vec::new();
    a.feed(PS, |f| {
        push_planes(&mut pa, f.planar);
        Ok(())
    })?;
    b.feed(PS, |f| {
        push_planes(&mut pb, f.planar);
        Ok(())
    })?;
    let _ = a.finish(|f| {
        push_planes(&mut pa, f.planar);
        Ok(())
    })?;
    let _ = b.finish(|f| {
        push_planes(&mut pb, f.planar);
        Ok(())
    })?;
    assert_eq!(pa, pb);
    assert_eq!(pa.len(), 2);
    Ok(())
}
