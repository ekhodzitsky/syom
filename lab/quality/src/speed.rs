//! TASK-83: LC encode wall-clock harness (isolated; not linked by cargo test).
//!
//! Each cell encodes one clip with one option set `WARM + reps` times and
//! prints the sorted per-rep milliseconds so `speed_ci.py` can bootstrap a
//! confidence interval on the median ratio between two builds. The FNV-1a
//! hash of the elementary stream pins byte identity across builds.
//!
//!   cargo run --release --bin syom_speed -- [reps] > run.txt

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use syom::{decode_with, encode_with, DecodeOptions, EncodeOptions};

const RATE: u32 = 48_000;
const WARM: usize = 5;

struct Clip {
    name: &'static str,
    pcm: Vec<Vec<f32>>,
}

fn main() {
    let reps: usize = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(31);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo = root.parent().unwrap().parent().unwrap();
    let clips = clips(repo);
    println!("# syom_speed reps={reps} warm={WARM} (ms per rep, sorted; xrt = audio s / median s)");
    println!("cell\tch\tsecs\tbps\tmedian_ms\tp95_ms\txrt\tbytes\tfnv1a\treps");
    for clip in &clips {
        let ch = clip.pcm.len();
        let secs = clip.pcm[0].len() as f64 / f64::from(RATE);
        let cells: Vec<(&str, EncodeOptions)> = vec![
            ("lc128", EncodeOptions::adts()),
            ("lc64", EncodeOptions::adts().with_bitrate_bps(64_000)),
            ("q5", EncodeOptions::adts().with_quality(5)),
        ];
        for (tag, opts) in cells {
            let mut ms = Vec::with_capacity(reps);
            let mut bytes = Vec::new();
            for i in 0..WARM + reps {
                let t = Instant::now();
                bytes = encode_with(&clip.pcm, RATE, &opts).expect("encode");
                let dt = t.elapsed().as_secs_f64() * 1e3;
                if i >= WARM {
                    ms.push(dt);
                }
            }
            ms.sort_by(|a, b| a.total_cmp(b));
            let med = ms[ms.len() / 2];
            let p95 = ms[(ms.len() * 95 / 100).min(ms.len() - 1)];
            let list: Vec<String> = ms.iter().map(|v| format!("{v:.3}")).collect();
            println!(
                "{}-{tag}\t{ch}\t{secs:.2}\t{}\t{med:.3}\t{p95:.3}\t{:.1}\t{}\t{:016x}\t{}",
                clip.name,
                opts.bitrate_bps,
                secs * 1e3 / med,
                bytes.len(),
                fnv1a(&bytes),
                list.join(",")
            );
        }
    }
}

fn fnv1a(b: &[u8]) -> u64 {
    b.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, &x| {
        (h ^ u64::from(x)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn clips(repo: &Path) -> Vec<Clip> {
    let mut v = Vec::new();
    if let Some(l) = lecture(repo) {
        // 0.25 s natural speech, tiled to 2 s (speed only; content repeats).
        let tiled: Vec<Vec<f32>> = l.iter().map(|p| p.repeat(8)).collect();
        v.push(Clip {
            name: "lecture-st",
            pcm: tiled.clone(),
        });
        v.push(Clip {
            name: "lecture-mono",
            pcm: vec![tiled[0].clone()],
        });
    }
    v.push(Clip {
        name: "music-st",
        pcm: music(2.0),
    });
    v.push(Clip {
        name: "noise-st",
        pcm: vec![noise(0.3, 5, 2.0), noise(0.3, 6, 2.0)],
    });
    v.push(Clip {
        name: "click-mono",
        pcm: vec![clicks(2.0)],
    });
    v
}

fn lecture(repo: &Path) -> Option<Vec<Vec<f32>>> {
    let bytes = fs::read(repo.join("src/goldens/lecture.m4a")).ok()?;
    let dec = decode_with(&bytes, &DecodeOptions::unbounded()).ok()?;
    // The golden is quiet; normalize to a 0.5 peak so the rate loop works.
    let peak = dec
        .channels
        .iter()
        .flatten()
        .fold(0.0f32, |m, &x| m.max(x.abs()));
    let g = if peak > 0.0 { 0.5 / peak } else { 1.0 };
    Some(
        dec.channels
            .iter()
            .map(|p| p.iter().map(|&x| x * g).collect())
            .collect(),
    )
}

fn noise(amp: f32, seed: u32, secs: f64) -> Vec<f32> {
    let n = (secs * f64::from(RATE)) as usize;
    let mut s = 0x0BAD_F00Du32 ^ seed.wrapping_mul(0x9E37_79B9);
    (0..n)
        .map(|_| {
            s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            amp * (((s >> 9) as f32 / (1u32 << 23) as f32) * 2.0 - 1.0)
        })
        .collect()
}

fn clicks(secs: f64) -> Vec<f32> {
    let n = (secs * f64::from(RATE)) as usize;
    let mut v = vec![0.0f32; n];
    let mut i = 0;
    while i < n {
        v[i] = 0.9;
        i += 2048;
    }
    v
}

/// Harmonic tones with vibrato and a note change every 0.5 s over a −40 dB
/// noise floor (stereo: right is the left detuned and attenuated).
fn music(secs: f64) -> Vec<Vec<f32>> {
    let n = (secs * f64::from(RATE)) as usize;
    let floor = noise(0.01, 9, secs);
    let mut l = Vec::with_capacity(n);
    let mut r = Vec::with_capacity(n);
    let mut ph = [0.0f64; 6];
    let mut ph_r = [0.0f64; 6];
    for i in 0..n {
        let t = i as f64 / f64::from(RATE);
        let note = [220.0, 261.63, 329.63, 392.0][((t * 2.0) as usize) % 4];
        let vib = 1.0 + 0.004 * (2.0 * std::f64::consts::PI * 5.5 * t).sin();
        let (mut sl, mut sr) = (0.0f64, 0.0f64);
        for (h, (p, pr)) in ph.iter_mut().zip(ph_r.iter_mut()).enumerate() {
            let f = note * (h as f64 + 1.0) * vib;
            *p = (*p + f / f64::from(RATE)).fract();
            *pr = (*pr + f * 1.002 / f64::from(RATE)).fract();
            let a = 0.25 / (h as f64 + 1.0);
            sl += a * (2.0 * std::f64::consts::PI * *p).sin();
            sr += 0.8 * a * (2.0 * std::f64::consts::PI * *pr).sin();
        }
        l.push(sl as f32 + floor[i]);
        r.push(sr as f32 + floor[(i + 777) % n]);
    }
    vec![l, r]
}
