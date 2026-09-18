//! TASK-105: smallest thread stack that completes an operation.
//! `stack <mode> <kib>` runs the operation on a thread with that stack and
//! exits 0; an overflow kills the process, which the driver detects.
use syom::{decode_with, encode_with, DecodeOptions, EncodeOptions};

fn pcm(planes: usize) -> Vec<Vec<f32>> {
    (0..planes)
        .map(|p| (0..16_384).map(|i| 0.3 * ((i * (p + 3)) as f32 * 0.01).sin()).collect())
        .collect()
}

fn main() {
    let mode = std::env::args().nth(1).unwrap();
    let kib: usize = std::env::args().nth(2).unwrap().parse().unwrap();
    // Inputs are prepared on the main thread so only the codec call counts.
    let lc = encode_with(&pcm(2), 48_000, &EncodeOptions::adts()).unwrap();
    let he2 = encode_with(&pcm(2), 48_000, &EncodeOptions::adts().with_he_v2(true).with_bitrate_bps(32_000)).unwrap();
    let src = pcm(2);
    let six = pcm(6);
    let t = std::thread::Builder::new().stack_size(kib * 1024).spawn(move || match mode.as_str() {
        "dec-lc" => decode_with(&lc, &DecodeOptions::unbounded()).unwrap().channels.len(),
        "dec-he2" => decode_with(&he2, &DecodeOptions::unbounded()).unwrap().channels.len(),
        "enc-lc" => encode_with(&src, 48_000, &EncodeOptions::adts()).unwrap().len(),
        "enc-he2" => encode_with(&src, 48_000, &EncodeOptions::adts().with_he_v2(true).with_bitrate_bps(32_000)).unwrap().len(),
        _ => encode_with(&six, 48_000, &EncodeOptions::adts().with_bitrate_bps(256_000)).unwrap().len(),
    });
    println!("{}", t.unwrap().join().unwrap());
}
