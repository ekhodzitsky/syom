//! Decode-only consumer: any supported stream on stdin to planar PCM.
use std::io::Read;
fn main() {
    let mut buf = Vec::new();
    std::io::stdin().read_to_end(&mut buf).unwrap();
    let pcm = syom::decode_with(&buf, &syom::DecodeOptions::unbounded()).unwrap();
    println!("{} {}", pcm.channels.len(), pcm.sample_rate);
}
