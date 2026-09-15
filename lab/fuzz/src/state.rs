//! Isolated stateful Decoder/Encoder fuzz (TASK-49). Not invoked by `cargo test`.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn next_u32(s: &mut u32) -> u32 {
    *s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    *s
}

fn load(rel: &str) -> Vec<u8> {
    std::fs::read(repo_root().join(rel)).unwrap_or_else(|_| panic!("{rel}"))
}

fn parse_args() -> (Duration, u64, u32) {
    let mut seconds = 30u64;
    let mut iters = 50_000u64;
    let mut seed = 0x49F4_9F49u32;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--seconds" => seconds = args.next().expect("s").parse().expect("u64"),
            "--iters" => iters = args.next().expect("n").parse().expect("u64"),
            "--seed" => seed = args.next().expect("seed").parse().expect("u32"),
            "--replay" => continue,
            other => panic!("unknown arg {other}"),
        }
    }
    (Duration::from_secs(seconds), iters, seed)
}

fn drop_frame(_: syom::Frame<'_>) -> syom::Result<()> {
    Ok(())
}

fn drop_au(_: syom::EncodedFrame<'_>) -> syom::Result<()> {
    Ok(())
}

fn fail_frame(_: syom::Frame<'_>) -> syom::Result<()> {
    Err(syom::AacError::decode("fuzz-cb"))
}

fn fail_au(_: syom::EncodedFrame<'_>) -> syom::Result<()> {
    Err(syom::AacError::encode("fuzz-cb"))
}

fn decode_step(dec: &mut syom::Decoder, src: &[u8], rng: &mut u32) {
    match next_u32(rng) % 6 {
        0 | 1 => {
            let n = 1 + (next_u32(rng) as usize) % src.len().max(1).min(64);
            let off = (next_u32(rng) as usize) % src.len().max(1);
            let end = (off + n).min(src.len());
            let _ = dec.feed(&src[off..end], drop_frame);
        }
        2 => {
            let mut bad = src.to_vec();
            if !bad.is_empty() {
                let i = (next_u32(rng) as usize) % bad.len();
                bad[i] ^= 0xFF;
            }
            let _ = dec.feed(&bad, drop_frame);
        }
        3 => {
            let _ = dec.feed(src, fail_frame);
        }
        4 => {
            let _ = dec.finish(drop_frame);
        }
        _ => dec.reset(),
    }
}

fn encode_step(enc: &mut syom::Encoder, rng: &mut u32) {
    match next_u32(rng) % 7 {
        0 | 1 => {
            let n = 1 + (next_u32(rng) as usize) % 1500;
            let pcm: Vec<f32> = (0..n)
                .map(|i| ((i as u32).wrapping_mul(*rng) as f32 / u32::MAX as f32) * 1.8 - 0.9)
                .collect();
            let _ = enc.feed(&[&pcm], drop_au);
        }
        2 => {
            let p = [f32::NAN];
            let _ = enc.feed(&[&p], drop_au);
        }
        3 => {
            let p = [2.5f32];
            let _ = enc.feed(&[&p], drop_au);
        }
        4 => {
            let p = [0.1f32; 2048];
            let _ = enc.feed(&[&p], fail_au);
        }
        5 => {
            let _ = enc.finish(drop_au);
        }
        _ => {
            let _ = enc.reset();
        }
    }
}

fn main() {
    let sine = load("src/goldens/sine48.adts");
    let he = load("src/goldens/he48.adts");
    let latm = load("src/goldens/latm48.latm");
    let seeds = [sine.as_slice(), he.as_slice(), latm.as_slice()];
    let (limit, max_iters, seed) = parse_args();
    let start = Instant::now();
    let mut rng = seed;
    let mut n = 0u64;
    let mut panics = 0u64;
    let mut dec = syom::Decoder::new(syom::DecodeOptions::speech());
    let mut enc = syom::Encoder::new(48_000, 1, &syom::EncodeOptions::adts()).expect("enc");
    while n < max_iters && start.elapsed() < limit {
        let src = seeds[(next_u32(&mut rng) as usize) % seeds.len()];
        let panicked = catch_unwind(AssertUnwindSafe(|| {
            decode_step(&mut dec, src, &mut rng);
            encode_step(&mut enc, &mut rng);
        }))
        .is_err();
        if panicked {
            panics += 1;
            dec = syom::Decoder::new(syom::DecodeOptions::speech());
            enc = syom::Encoder::new(48_000, 1, &syom::EncodeOptions::adts()).expect("enc");
        }
        n += 1;
    }
    println!("syom-lab-fuzz-state TASK-103");
    println!("seed {seed}");
    println!("iters {n}");
    println!("wall_ms {}", start.elapsed().as_millis());
    println!("panics {panics}");
    if panics > 0 {
        std::process::exit(1);
    }
}
