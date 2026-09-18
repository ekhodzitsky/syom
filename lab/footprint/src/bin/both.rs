//! Common usage: decode a stream, re-encode it as LC ADTS.
use std::io::Read;
fn main() {
    let mut buf = Vec::new();
    std::io::stdin().read_to_end(&mut buf).unwrap();
    let pcm = syom::decode_with(&buf, &syom::DecodeOptions::unbounded()).unwrap();
    let planes: Vec<Vec<f32>> = pcm.channels.into_iter().take(2).collect();
    let adts = syom::encode(&planes, pcm.sample_rate).unwrap();
    println!("{}", adts.len());
}
