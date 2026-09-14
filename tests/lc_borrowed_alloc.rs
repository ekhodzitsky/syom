//! TASK-78: streaming encode still matches one-shot (borrowed AU path).
//! Heap counts live in `lab/baseline/ALLOC.md` (thin-LTO probe).

#![allow(clippy::unwrap_used)]

use syom::{EncodeOptions, Encoder};

#[test]
fn encode_bytes_still_match_one_shot() {
    let n = 8 * 1024;
    let pcm: Vec<Vec<f32>> = vec![
        (0..n)
            .map(|i| 0.5 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 48_000.0).sin())
            .collect(),
    ];
    let want = syom::encode(&pcm, 48_000).unwrap();
    let mut enc = Encoder::new(48_000, 1, &EncodeOptions::adts()).unwrap();
    let mut out = Vec::new();
    let planes: Vec<&[f32]> = pcm.iter().map(Vec::as_slice).collect();
    enc.feed(&planes, |f| {
        out.extend_from_slice(f.au);
        Ok(())
    })
    .unwrap();
    enc.finish(|f| {
        out.extend_from_slice(f.au);
        Ok(())
    })
    .unwrap();
    assert_eq!(out, want);
}
