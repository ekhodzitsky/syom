//! Isolated mutational parser fuzz (TASK-48). Not invoked by `cargo test`.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const MAX_LEN: usize = 64 * 1024;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn harvest(data: &[u8]) {
    let opts = syom::DecodeOptions::speech();
    let _ = syom::sniff_aac(data);
    let _ = syom::sniff_is_adts(data);
    let _ = syom::sniff_is_latm(data);
    let _ = syom::sniff_is_isobmff(data);
    let _ = syom::decode_with(data, &opts);
}

fn next_u32(s: &mut u32) -> u32 {
    *s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    *s
}

fn mutate(src: &[u8], s: &mut u32) -> Vec<u8> {
    if src.is_empty() {
        return vec![next_u32(s) as u8];
    }
    let mut out = src.to_vec();
    match next_u32(s) % 6 {
        0 => {
            let i = (next_u32(s) as usize) % out.len();
            out[i] ^= 1 << (next_u32(s) % 8);
        }
        1 => {
            let i = (next_u32(s) as usize) % out.len();
            out[i] = next_u32(s) as u8;
        }
        2 => {
            let i = (next_u32(s) as usize) % (out.len() + 1);
            out.insert(i, next_u32(s) as u8);
        }
        3 if out.len() > 1 => {
            let i = (next_u32(s) as usize) % out.len();
            out.remove(i);
        }
        4 => {
            let n = 1 + (next_u32(s) as usize) % out.len().min(16);
            out.truncate(out.len().saturating_sub(n).max(1));
        }
        _ => {
            let n = 1 + (next_u32(s) as usize) % 8;
            out.extend(std::iter::repeat(next_u32(s) as u8).take(n));
        }
    }
    out.truncate(MAX_LEN);
    out
}

fn load_seeds(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let goldens = [
        "src/goldens/sine48.adts",
        "src/goldens/latm48.latm",
        "src/goldens/sine441.m4a",
        "src/goldens/he48.adts",
        "src/goldens/ps48.adts",
        "src/goldens/mc51.adts",
        "src/goldens/enc48.adts",
        "src/goldens/tns48.adts",
        "src/goldens/pns48.adts",
        "src/goldens/lecture.m4a",
        "src/goldens/he48.latm",
        "src/goldens/ps48.m4a",
        "src/goldens/fmp4_lc.mp4",
        "src/goldens/ld48.loas",
        "src/goldens/ld64m.loas",
        "src/goldens/ld64mus.loas",
    ];
    for rel in goldens {
        let p = root.join(rel);
        out.push((rel.to_string(), std::fs::read(&p).expect(rel)));
    }
    let dir = root.join("corpus/fuzz");
    for ent in std::fs::read_dir(&dir).expect("corpus/fuzz") {
        let p = ent.expect("ent").path();
        if p.extension().and_then(|s| s.to_str()) == Some("bin") {
            let name = format!("corpus/fuzz/{}", p.file_name().unwrap().to_string_lossy());
            out.push((name, std::fs::read(&p).expect("seed")));
        }
    }
    out
}

fn parse_args() -> (Duration, u64, u32, Option<PathBuf>) {
    let mut seconds = 30u64;
    let mut iters = 100_000u64;
    let mut seed = 0xC0FF_EE42u32;
    let mut crash_dir = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--seconds" => seconds = args.next().expect("--seconds").parse().expect("u64"),
            "--iters" => iters = args.next().expect("--iters").parse().expect("u64"),
            "--seed" => {
                let s = args.next().expect("--seed");
                seed = s.parse().unwrap_or(0xC0FF_EE42);
            }
            "--crash-dir" => crash_dir = Some(PathBuf::from(args.next().expect("--crash-dir"))),
            other => panic!("unknown arg {other}"),
        }
    }
    (Duration::from_secs(seconds), iters, seed, crash_dir)
}

fn main() {
    let root = repo_root();
    let seeds = load_seeds(&root);
    let (limit, max_iters, seed, crash_dir) = parse_args();
    if let Some(d) = &crash_dir {
        let _ = std::fs::create_dir_all(d);
    }
    let start = Instant::now();
    let mut rng = seed;
    let mut n = 0u64;
    let mut panics = 0u64;
    let mut last_panic: Option<(u64, String)> = None;
    while n < max_iters && start.elapsed() < limit {
        let idx = (next_u32(&mut rng) as usize) % seeds.len();
        let buf = mutate(&seeds[idx].1, &mut rng);
        let panicked = catch_unwind(AssertUnwindSafe(|| harvest(&buf))).is_err();
        if panicked {
            panics += 1;
            last_panic = Some((n, seeds[idx].0.clone()));
            if let Some(d) = &crash_dir {
                let p = d.join(format!("p-{seed}-{n}.bin"));
                let _ = std::fs::write(&p, &buf);
                eprintln!("crash {}", p.display());
            }
        }
        n += 1;
    }
    println!("syom-lab-fuzz TASK-103");
    println!("seed {seed}");
    println!("seeds {}", seeds.len());
    for (name, b) in &seeds {
        println!("  {name} {} B", b.len());
    }
    println!("iters {n}");
    println!("wall_ms {}", start.elapsed().as_millis());
    println!("panics {panics}");
    if let Some((i, name)) = last_panic {
        println!("last_panic_iter {i} seed {name}");
        std::process::exit(1);
    }
}
