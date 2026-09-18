//! Encode-only consumer: f32le mono PCM on stdin to LC ADTS.
use std::io::Read;
fn main() {
    let mut buf = Vec::new();
    std::io::stdin().read_to_end(&mut buf).unwrap();
    let pcm: Vec<f32> = buf.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
    let adts = syom::encode(&[pcm], 48_000).unwrap();
    println!("{}", adts.len());
}
