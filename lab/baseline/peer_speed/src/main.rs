//! One-core in-process syom timers. Not invoked by `cargo test`.
//!
//! The clock wraps `encode_with` / `decode_with` only. Process start and
//! PCM generation sit outside that region and are not subtracted from
//! anything.

use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::time::Instant;

use syom::{decode_with, encode_with, DecodeOptions, EncodeOptions};

const RATE: u32 = 48_000;
const SECS: usize = 2;
const N: usize = RATE as usize * SECS;

fn main() {
    let mut args = std::env::args().skip(1);
    let cmd = args.next().unwrap_or_default();
    let code = match cmd.as_str() {
        "encode-voice" => {
            let warm = parse(&args.next(), 4);
            let reps = parse(&args.next(), 21);
            let pcm_out = args.next();
            encode_voice(warm, reps, pcm_out.as_deref())
        }
        "decode" => {
            let path = args.next().expect("decode FILE WARM REPS");
            let warm = parse(&args.next(), 4);
            let reps = parse(&args.next(), 21);
            decode_file(&path, warm, reps)
        }
        _ => {
            eprintln!(
                "usage:\n  peer_speed encode-voice [WARM] [REPS] [OUT.f32]\n  peer_speed decode FILE [WARM] [REPS]"
            );
            2
        }
    };
    std::process::exit(code);
}

fn parse(s: &Option<String>, default: usize) -> usize {
    s.as_ref()
        .and_then(|v| v.parse().ok())
        .filter(|n| *n > 0)
        .unwrap_or(default)
}

fn encode_voice(warm: usize, reps: usize, pcm_out: Option<&str>) -> i32 {
    let pcm = vec![voice(3, 150.0), voice(4, 113.0)];
    if let Some(path) = pcm_out {
        if let Err(err) = write_planar(path, &pcm) {
            eprintln!("write {path}: {err}");
            return 2;
        }
    }
    print_host();
    let cells = [
        ("lc128", EncodeOptions::adts().with_bitrate_bps(128_000)),
        ("lc24", EncodeOptions::adts().with_bitrate_bps(24_000)),
    ];
    for (name, opts) in cells {
        let mut ms = Vec::with_capacity(reps);
        let mut last_len = 0usize;
        for i in 0..warm + reps {
            let t0 = Instant::now();
            let bytes = encode_with(&pcm, RATE, &opts).expect("encode");
            last_len = bytes.len();
            std::hint::black_box(bytes.len());
            drop(bytes);
            let dt = t0.elapsed().as_secs_f64() * 1e3;
            if i >= warm {
                ms.push(dt);
            }
        }
        print_encode(name, opts.bitrate_bps, warm, &ms, last_len);
    }
    0
}

fn decode_file(path: &str, warm: usize, reps: usize) -> i32 {
    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(err) => {
            eprintln!("read {path}: {err}");
            return 2;
        }
    };
    let mut data = Vec::new();
    if file.read_to_end(&mut data).is_err() {
        eprintln!("read {path}");
        return 2;
    }
    let opts = DecodeOptions::unbounded();
    print_host();
    let mut ms = Vec::with_capacity(reps);
    let mut last_rate = 0u32;
    let mut last_ch = 0usize;
    let mut last_n = 0usize;
    for i in 0..warm + reps {
        let t0 = Instant::now();
        let dec = decode_with(&data, &opts).expect("decode");
        last_rate = dec.sample_rate;
        last_ch = dec.channels.len();
        last_n = dec.channels.first().map(|p| p.len()).unwrap_or(0);
        std::hint::black_box(last_n);
        drop(dec);
        let dt = t0.elapsed().as_secs_f64() * 1e3;
        if i >= warm {
            ms.push(dt);
        }
    }
    ms.sort_by(|a, b| a.total_cmp(b));
    let med = ms[ms.len() / 2];
    println!(
        "{{\"engine\":\"syom\",\"op\":\"decode\",\"file\":\"{path}\",\"warm\":{warm},\"reps\":{},\"median_ms\":{med:.4},\"min_ms\":{:.4},\"max_ms\":{:.4},\"rate\":{last_rate},\"channels\":{last_ch},\"samples\":{last_n},\"lane\":\"unbounded_split\",\"samples_ms\":{}}}",
        ms.len(),
        ms[0],
        ms[ms.len() - 1],
        fmt_reps(&ms)
    );
    0
}

fn print_encode(name: &str, bps: u32, warm: usize, ms: &[f64], adts_bytes: usize) {
    let mut sorted = ms.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let med = sorted[sorted.len() / 2];
    println!(
        "{{\"engine\":\"syom\",\"op\":\"encode\",\"cell\":\"{name}\",\"profile\":\"release-thin-lto-codegen-units-1\",\"bitrate_bps\":{bps},\"warm\":{warm},\"reps\":{},\"median_ms\":{med:.4},\"min_ms\":{:.4},\"max_ms\":{:.4},\"rate\":{RATE},\"channels\":2,\"samples\":{N},\"adts_bytes\":{adts_bytes},\"pcm\":\"voice-like seed 3/4 f0 150/113 Hz {SECS}s\",\"samples_ms\":{}}}",
        sorted.len(),
        sorted[0],
        sorted[sorted.len() - 1],
        fmt_reps(&sorted)
    );
}

fn print_host() {
    let load = std::fs::read_to_string("/proc/loadavg").unwrap_or_default();
    let khz = std::fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq")
        .unwrap_or_default();
    println!(
        "{{\"op\":\"host\",\"loadavg\":\"{}\",\"cpu0_khz\":\"{}\"}}",
        load.trim(),
        khz.trim()
    );
}

fn fmt_reps(ms: &[f64]) -> String {
    let body: Vec<String> = ms.iter().map(|v| format!("{v:.4}")).collect();
    format!("[{}]", body.join(","))
}

fn write_planar(path: &str, pcm: &[Vec<f32>]) -> std::io::Result<()> {
    let mut f = File::create(Path::new(path))?;
    for plane in pcm {
        let bytes =
            unsafe { std::slice::from_raw_parts(plane.as_ptr() as *const u8, plane.len() * 4) };
        f.write_all(bytes)?;
    }
    Ok(())
}

/// Same construction as `voice()` in `lab/quality/src/he_qual.rs` (the
/// voice-like / speech-like stereo clip in CORE_TUNE.md).
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
