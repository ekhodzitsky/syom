//! Native reference for `run.mjs`: the same summaries, as JSON lines.
use std::time::Instant;
use syom_lab_wasm::{decode_pcm_bytes, decode_summary, encode_bytes, test_pcm, FNV_SEED};

fn main() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../src/goldens");
    for name in std::env::args().skip(1) {
        if let Some(mode) = name.strip_prefix("enc:") {
            let mode: u32 = mode.parse().unwrap();
            let planes = if mode == 1 { 1 } else { 2 };
            let pcm = test_pcm(planes, 96_000);
            let t = Instant::now();
            let bytes = encode_bytes(&pcm, planes, 48_000, mode).unwrap();
            let ms = t.elapsed().as_secs_f64() * 1e3;
            let h = bytes.iter().fold(FNV_SEED, |h, &b| (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3));
            println!("{{\"case\":\"{name}\",\"bytes\":{},\"hash\":\"{h:016x}\",\"ms\":{ms:.2}}}", bytes.len());
            continue;
        }
        let data = std::fs::read(root.join(&name)).unwrap();
        if let Ok(dir) = std::env::var("SYOM_DUMP") {
            std::fs::write(std::path::Path::new(&dir).join(format!("{name}.f32")), decode_pcm_bytes(&data)).unwrap();
        }
        let t = Instant::now();
        let s = decode_summary(&data);
        let ms = t.elapsed().as_secs_f64() * 1e3;
        println!(
            "{{\"case\":\"{name}\",\"status\":{},\"channels\":{},\"rate\":{},\"frames\":{},\"samples\":{},\"hash\":\"{:016x}\",\"ms\":{ms:.2}}}",
            s[0], s[1], s[2], s[3], s[4], s[5]
        );
    }
}
