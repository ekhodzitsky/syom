//! TASK-79: SBR conversion/reconstruction workspace reuse.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::engine::sbr_freq_bands::HiLoTables;
use crate::engine::sbr_hf_gen::{Patches, generate_hf, generate_hf_into};
use crate::engine::sbr_qmf::Complex;
use crate::{DecodeOptions, Decoder, decode_with};

const HE: &[u8] = include_bytes!("../goldens/he48.adts");
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
fn he48_streaming_matches_oneshot_split_and_speech() -> crate::Result<()> {
    for opts in [DecodeOptions::unbounded(), DecodeOptions::speech()] {
        let owned = decode_with(HE, &opts)?;
        let (rate, planes) = collect(HE, opts)?;
        assert_eq!(rate, owned.sample_rate);
        assert_eq!(planes.len(), owned.channels.len());
        for (a, b) in planes.iter().zip(owned.channels.iter()) {
            assert_eq!(a, b);
        }
    }
    Ok(())
}

#[test]
fn he48_reset_matches_fresh_and_does_not_leak_into_lc() -> crate::Result<()> {
    let opts = DecodeOptions::unbounded();
    let mut dec = Decoder::new(DecodeOptions::unbounded());
    let mut first = Vec::new();
    dec.feed(HE, |_| Ok(()))?;
    let _ = dec.finish(|_| Ok(()))?;
    dec.reset();
    dec.feed(HE, |f| {
        push_planes(&mut first, f.planar);
        Ok(())
    })?;
    let _ = dec.finish(|f| {
        push_planes(&mut first, f.planar);
        Ok(())
    })?;
    let owned = decode_with(HE, &opts)?;
    assert_eq!(first.len(), owned.channels.len());
    for (a, b) in first.iter().zip(owned.channels.iter()) {
        assert_eq!(a, b, "reset HE session must match a fresh decoder");
    }

    dec.reset();
    let lc = decode_with(LC, &DecodeOptions::speech())?;
    let mut after = Vec::new();
    dec.feed(LC, |f| {
        push_planes(&mut after, f.planar);
        Ok(())
    })?;
    let info = dec.finish(|f| {
        push_planes(&mut after, f.planar);
        Ok(())
    })?;
    assert_eq!(info.sample_rate, lc.sample_rate);
    assert_eq!(after.len(), lc.channels.len());
    assert_eq!(info.sample_rate, 48_000);
    assert_eq!(after.len(), 1, "HE SBR state must not leak into LC");
    Ok(())
}

#[test]
fn two_he_decoders_do_not_alias() -> crate::Result<()> {
    let mut a = Decoder::new(DecodeOptions::unbounded());
    let mut b = Decoder::new(DecodeOptions::unbounded());
    let mut pa = Vec::new();
    let mut pb = Vec::new();
    a.feed(HE, |f| {
        push_planes(&mut pa, f.planar);
        Ok(())
    })?;
    b.feed(HE, |f| {
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
    Ok(())
}

#[test]
fn generate_hf_into_matches_allocating_wrapper() {
    let a1 = Complex::new(0.8, 0.2);
    let a2 = Complex::new(-0.4, 0.0);
    let mut x = vec![[Complex::default(); 32]; 40];
    x[0][2] = Complex::new(1.0, 0.3);
    x[1][2] = Complex::new(0.2, -0.5);
    for n in 2..40 {
        x[n][2] = a1 * x[n - 1][2] + a2 * x[n - 2][2];
    }
    let patches = Patches {
        start: vec![2],
        num: vec![8],
    };
    let bands = HiLoTables {
        f_table_high: vec![8, 12, 16],
        f_table_low: vec![8, 16],
        f_table_noise: vec![8, 16],
        m: 8,
        k_x: 8,
    };
    let want = generate_hf(&x, &patches, &[0.0], &bands, 0..32, 32).unwrap();
    let mut got = vec![[Complex::new(99.0, 99.0); 64]; 40];
    generate_hf_into(&x, &patches, &[0.0], &bands, 0..32, 32, &mut got).unwrap();
    assert_eq!(got, want);
}
