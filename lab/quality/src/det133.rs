//! TASK-133 attack-detector parameter sweep (offline simulation, no codec):
//! candidate detectors scored on the he_qual dev clips — tremolo must stay
//! quiet, clicks/lecture must still fire. Isolated; never run by tests.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use syom::{decode_with, DecodeOptions};

const RATE: u32 = 48_000;
const SECS: usize = 2;
const N: usize = RATE as usize * SECS;
const SUB: usize = 128; // 8 sub-blocks per 1024-sample frame

/// Current syom detector: e > 8 * mean(prev 16 sub-blocks), e > 1e-6.
fn det_current(ch: &[f32]) -> Vec<bool> {
    let mut hist = [0.0f32; 16];
    let (mut pos, mut len) = (0usize, 0usize);
    let mut prev = 0.0f32;
    let mut out = Vec::new();
    for frame in ch.chunks_exact(1024) {
        let mut attack = false;
        for sb in frame.chunks_exact(SUB) {
            let mut e = 0.0f32;
            for &x in sb {
                let d = x - prev;
                e += d * d;
                prev = x;
            }
            if len > 0 && e > 1e-6 {
                let mean: f32 = hist.iter().take(len).sum::<f32>() / len as f32;
                if e > 8.0 * mean {
                    attack = true;
                }
            }
            hist[pos] = e;
            pos = (pos + 1) % 16;
            len = (len + 1).min(16);
        }
        out.push(attack);
    }
    out
}

/// FDK-style: high-passed sub-block energy vs a leaky accumulator fed the
/// previous sub-block: attack if e > ratio * acc; acc = alpha*acc + beta*e_prev.
/// Absolute floor on the frame's max sub-block energy.
fn det_leaky(ch: &[f32], alpha: f32, beta: f32, ratio: f32, floor: f32) -> Vec<bool> {
    let mut acc = 0.0f32;
    let mut prev_e = 0.0f32;
    let mut prev = 0.0f32;
    let mut out = Vec::new();
    for frame in ch.chunks_exact(1024) {
        let mut attack = false;
        let mut emax = 0.0f32;
        let mut flags = [false; 8];
        for (i, sb) in frame.chunks_exact(SUB).enumerate() {
            let mut e = 0.0f32;
            for &x in sb {
                let d = x - prev;
                e += d * d;
                prev = x;
            }
            acc = alpha * acc + beta * prev_e;
            if e > ratio * acc {
                flags[i] = true;
            }
            emax = emax.max(e);
            prev_e = e;
        }
        if emax > floor {
            attack = flags.iter().any(|&f| f);
        }
        out.push(attack);
    }
    out
}

fn summarize(name: &str, flags: &[bool]) {
    let n = flags.iter().filter(|&&f| f).count();
    let idx: Vec<String> = flags
        .iter()
        .enumerate()
        .filter(|(_, &f)| f)
        .map(|(i, _)| i.to_string())
        .collect();
    println!(
        "  {name}: {}/{} frames fire: {}",
        n,
        flags.len(),
        idx.join(",")
    );
}

fn main() -> ExitCode {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo = root.parent().unwrap().parent().unwrap();
    let clips: Vec<(&str, Vec<f32>)> = vec![
        ("tremolo", tremolo()),
        ("click-st", clicks(21)),
        ("voice-like", voice(3, 150.0)),
        ("mix-st", mix(1)),
    ];
    let mut clips = clips;
    if let Some(lec) = lecture(repo) {
        clips.push(("lecture", lec));
    }
    println!("current detector (8x mean of 16):");
    for (name, pcm) in &clips {
        summarize(name, &det_current(pcm));
    }
    for &(a, b, r, f) in &[
        (0.65f32, 0.35f32, 8.0f32, 1e-6f32),
        (0.65, 0.35, 10.0, 1e-6),
        (0.5, 0.5, 8.0, 1e-6),
        (0.65, 0.35, 8.0, 1e-4),
        (0.65, 0.35, 6.0, 1e-4),
        (0.8, 0.2, 8.0, 1e-4),
    ] {
        println!("leaky alpha={a} beta={b} ratio={r} floor={f}:");
        for (name, pcm) in &clips {
            summarize(name, &det_leaky(pcm, a, b, r, f));
        }
    }
    ExitCode::SUCCESS
}

fn tremolo() -> Vec<f32> {
    (0..N)
        .map(|i| {
            let t = i as f64 / RATE as f64;
            let env = 0.5 + 0.5 * (2.0 * std::f64::consts::PI * 8.0 * t).sin();
            (0.4 * env * (2.0 * std::f64::consts::PI * 880.0 * t).sin()) as f32
        })
        .collect()
}

fn noise(amp: f32, seed: u32) -> Vec<f32> {
    let mut s = 0x1234_5678u32.wrapping_mul(seed.max(1));
    (0..N)
        .map(|_| {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            2.0 * amp * ((s as f32 / u32::MAX as f32) - 0.5)
        })
        .collect()
}

fn mix(seed: u32) -> Vec<f32> {
    let t: Vec<f32> = (0..N)
        .map(|i| {
            0.3 * (2.0 * std::f64::consts::PI * (440.0 + 100.0 * seed as f64) * i as f64
                / RATE as f64)
                .sin() as f32
        })
        .collect();
    let n = noise(0.05, seed);
    t.iter().zip(&n).map(|(a, b)| a + b).collect()
}

fn clicks(seed: u32) -> Vec<f32> {
    let mut v = vec![0.0f32; N];
    let mut s = seed;
    for i in 0..N {
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        v[i] = 0.02 * ((s as f32 / u32::MAX as f32) - 0.5);
    }
    for c in 0..(SECS * 4) {
        let at = c * RATE as usize / 4 + 300 + seed as usize;
        for k in 0..64 {
            v[at + k] = 0.8 * (1.0 - k as f32 / 64.0) * if k % 2 == 0 { 1.0 } else { -1.0 };
        }
    }
    v
}

fn voice(seed: u32, f0: f64) -> Vec<f32> {
    let mut out = vec![0.0f32; N];
    for h in 1..=40 {
        let f = f0 * h as f64;
        if f > 20_000.0 {
            break;
        }
        for (i, v) in out.iter_mut().enumerate() {
            let vib =
                1.0 + 0.01 * (2.0 * std::f64::consts::PI * 5.0 * i as f64 / RATE as f64).sin();
            *v += 0.012
                * (2.0 * std::f64::consts::PI * f * vib * i as f64 / RATE as f64).sin() as f32;
        }
    }
    let mut s = 7u32.wrapping_mul(seed);
    let (mut y1, mut y2) = (0.0f32, 0.0f32);
    for (i, v) in out.iter_mut().enumerate() {
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        let x = (s as f32 / u32::MAX as f32) - 0.5;
        y1 = 0.55 * (y1 + x - y2);
        y2 = x;
        let duck = if (i / 19_200) % 2 == 0 { 1.0 } else { 0.25 };
        *v += 0.36 * duck * y1 + 0.02 * x;
    }
    out
}

fn lecture(repo: &Path) -> Option<Vec<f32>> {
    let bytes = fs::read(repo.join("src/goldens/lecture.m4a")).ok()?;
    let dec = decode_with(&bytes, &DecodeOptions::unbounded()).ok()?;
    Some(dec.channels[0].clone())
}
