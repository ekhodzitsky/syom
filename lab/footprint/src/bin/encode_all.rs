//! Encode consumer that reaches every encoder: LC, HE v1, HE v2, surround.
use std::io::Read;
use syom::{encode_with, EncodeOptions};
fn main() {
    let mut buf = Vec::new();
    std::io::stdin().read_to_end(&mut buf).unwrap();
    let pcm: Vec<f32> = buf.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
    let mode = std::env::args().nth(1).unwrap_or_default();
    let planes = |n: usize| vec![pcm.clone(); n];
    let out = match mode.as_str() {
        "he" => encode_with(&planes(1), 48_000, &EncodeOptions::low_rate()),
        "he2" => encode_with(&planes(2), 48_000, &EncodeOptions::adts().with_he_v2(true).with_bitrate_bps(32_000)),
        "51" => encode_with(&planes(6), 48_000, &EncodeOptions::adts().with_bitrate_bps(256_000)),
        _ => encode_with(&planes(2), 48_000, &EncodeOptions::m4a()),
    };
    println!("{}", out.unwrap().len());
}
