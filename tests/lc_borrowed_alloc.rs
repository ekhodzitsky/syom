//! TASK-78: streaming encode still matches one-shot (borrowed AU path).
//! Heap counts live in `lab/baseline/ALLOC.md` (thin-LTO probe).
//!
//! One-shot ADTS prepends an iTunSMPB tag. Push frames stay raw access
//! units, so the match is the bytes after that tag.

#![allow(clippy::unwrap_used)]

use syom::{EncodeOptions, Encoder};

/// Drop one leading ID3v2.3/2.4 tag. The size is syncsafe and excludes
/// the ten-byte header; the footer flag adds ten bytes.
fn after_leading_id3(data: &[u8]) -> &[u8] {
    assert!(
        data.len() >= 10 && data.starts_with(b"ID3"),
        "one-shot ADTS starts with an ID3 tag"
    );
    assert!(data[3] == 3 || data[3] == 4, "ID3v2.3 or v2.4");
    assert!(data[6..10].iter().all(|b| b & 0x80 == 0), "syncsafe size");
    let body = (usize::from(data[6]) << 21)
        | (usize::from(data[7]) << 14)
        | (usize::from(data[8]) << 7)
        | usize::from(data[9]);
    let mut total = 10 + body;
    if data[5] & 0x10 != 0 {
        total += 10;
    }
    assert!(
        total < data.len() && data[total] == 0xff,
        "ADTS sync after the tag"
    );
    &data[total..]
}

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
    assert_eq!(out.as_slice(), after_leading_id3(&want));
}
