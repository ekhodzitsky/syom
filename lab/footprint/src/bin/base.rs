//! Baseline: reads stdin, prints a length. No syom call.
use std::io::Read;
fn main() {
    let mut buf = Vec::new();
    std::io::stdin().read_to_end(&mut buf).unwrap();
    println!("{}", std::hint::black_box(buf.len()));
}
