//! TASK-108 development curves: rate / quality / speed / delay for the
//! accepted LC (causal, lookahead) and HE v1 modes. Isolated; not run by
//! `cargo test`. Decoder: syom `decode_with` unbounded. SNR at the
//! declared priming is **not** PEAQ; HE adds a spectral band-level error
//! (SBR is not a waveform coder).
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;
use syom::{decode_with, encode_with, DecodeOptions, EncodeOptions};

const RATE: u32 = 48_000;
const SECS: usize = 2;
const N: usize = RATE as usize * SECS;
const HE_PRIMING: usize = 3018;
const LC_PRIMING: usize = 1024;
/// SBR crossover region used for the HE band split (6.75 kHz at 24 kbps/ch).
const SPLIT_HZ: f64 = 6_750.0;
/// v1 SBR ceiling at 48 kHz (k2 = 41): neither mode codes above it at
/// these rates, so the HF error stops here.
const TOP_HZ: f64 = 15_375.0;

struct Clip {
    name: &'static str,
    class: &'static str,
    pcm: Vec<Vec<f32>>,
}

#[derive(Clone, Copy)]
enum Mode {
    Lc,
    LcLookahead,
    He,
}

impl Mode {
    fn tag(self) -> &'static str {
        match self {
            Mode::Lc => "LC",
            Mode::LcLookahead => "LC+la",
            Mode::He => "HE",
        }
    }
    fn opts(self, bps: u32) -> EncodeOptions {
        let o = EncodeOptions::adts().with_bitrate_bps(bps);
        match self {
            Mode::Lc => o,
            Mode::LcLookahead => o.with_lookahead(true),
            Mode::He => o.with_he(true),
        }
    }
    fn priming(self) -> usize {
        match self {
            Mode::He => HE_PRIMING,
            _ => LC_PRIMING,
        }
    }
}

fn main() -> ExitCode {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo = root.parent().unwrap().parent().unwrap();
    println!("# TASK-108 development curves (48 kHz, {SECS} s synth + lecture)");
    println!();
    println!("Decoder: syom `decode_with` unbounded. `SNR` = waveform SNR after the declared priming (LC 1024, HE 3018) — not PEAQ. `LF SNR` / `HF err` from 2048-point block spectra: SNR of the spectrum below {SPLIT_HZ:.0} Hz, and mean |level error| (dB) over 375 Hz bands between {SPLIT_HZ:.0} and {TOP_HZ:.0} Hz (HE reconstructs that range parametrically; neither mode codes above {TOP_HZ:.0} Hz at these rates). `bps` = coded bytes over source seconds.");
    println!();
    println!("| clip | class | mode | req kbps | actual kbps | SNR dB | LF SNR dB | HF err dB |");
    println!("|---|---|---|---:|---:|---:|---:|---:|");
    for clip in clips(repo) {
        for (mode, rates) in [
            (Mode::Lc, &[32_000u32, 48_000, 64_000, 96_000, 128_000, 192_000][..]),
            (Mode::LcLookahead, &[64_000, 128_000][..]),
            (Mode::He, &[24_000, 32_000, 48_000, 64_000][..]),
        ] {
            for &bps in rates {
                row(&clip, mode, bps);
            }
        }
    }
    cpu_and_delay();
    ExitCode::SUCCESS
}

fn row(clip: &Clip, mode: Mode, bps: u32) {
    let ch = clip.pcm.len();
    let secs = clip.pcm[0].len() as f64 / RATE as f64;
    let stream = match encode_with(&clip.pcm, RATE, &mode.opts(bps)) {
        Ok(s) => s,
        Err(e) => {
            println!("| {} | {} | {} | {} | — | — | — | error: {e} |", clip.name, clip.class, mode.tag(), bps / 1000);
            return;
        }
    };
    let dec = match decode_with(&stream, &DecodeOptions::unbounded()) {
        Ok(d) => d,
        Err(e) => {
            println!("| {} | {} | {} | {} | — | — | — | decode error: {e} |", clip.name, clip.class, mode.tag(), bps / 1000);
            return;
        }
    };
    let actual = stream.len() as f64 * 8.0 / secs / 1000.0;
    let (mut snr, mut lf, mut hf) = (0.0, 0.0, 0.0);
    for c in 0..ch.min(dec.channels.len()) {
        let (s, l, h) = measures(&clip.pcm[c], &dec.channels[c], mode.priming());
        snr += s / ch as f64;
        lf += l / ch as f64;
        hf += h / ch as f64;
    }
    println!(
        "| {} | {} | {} | {} | {:.1} | {:.1} | {:.1} | {:.1} |",
        clip.name, clip.class, mode.tag(), bps / 1000, actual, snr, lf, hf
    );
}

/// (priming SNR, LF spectral SNR, HF mean |level error| dB).
fn measures(want: &[f32], got: &[f32], priming: usize) -> (f64, f64, f64) {
    let n = want.len().min(got.len().saturating_sub(priming));
    let w = &want[..n];
    let g = &got[priming..priming + n];
    let (mut ps, mut pe) = (0.0f64, 0.0f64);
    for i in 0..n {
        let s = f64::from(w[i]);
        let e = s - f64::from(g[i]);
        ps += s * s;
        pe += e * e;
    }
    let snr = if pe == 0.0 { 200.0 } else { 10.0 * (ps / pe).log10() };
    // Block spectra (2048, Hann), 43 bins of 23.4 Hz per ~1 kHz.
    let fft_n = 2048usize;
    let split_bin = (SPLIT_HZ / (RATE as f64 / fft_n as f64)) as usize;
    let (mut lf_s, mut lf_e) = (0.0f64, 0.0f64);
    let mut hf_err = 0.0f64;
    let mut hf_cnt = 0usize;
    let mut bw = vec![0.0f64; fft_n];
    let mut bg = vec![0.0f64; fft_n];
    let blocks = n / fft_n;
    for b in 0..blocks {
        for i in 0..fft_n {
            let win = 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / fft_n as f64).cos();
            bw[i] = f64::from(w[b * fft_n + i]) * win;
            bg[i] = f64::from(g[b * fft_n + i]) * win;
        }
        let sw = power_spectrum(&bw);
        let sg = power_spectrum(&bg);
        for k in 1..split_bin {
            lf_s += sw[k];
            lf_e += (sw[k].sqrt() - sg[k].sqrt()).powi(2);
        }
        // 16-bin (375 Hz) groups above the split: SBR-scale resolution,
        // not bins; groups more than 60 dB under the loudest are skipped.
        let top_bin = (TOP_HZ / (RATE as f64 / fft_n as f64)) as usize;
        let groups: Vec<(f64, f64)> = (split_bin..top_bin)
            .step_by(16)
            .map(|g| {
                let hi = (g + 16).min(top_bin);
                (sw[g..hi].iter().sum::<f64>(), sg[g..hi].iter().sum::<f64>())
            })
            .collect();
        let peak = groups.iter().map(|g| g.0).fold(0.0, f64::max);
        for (s, d) in groups {
            if s > peak * 1e-6 {
                hf_err += (10.0 * (d.max(1e-30) / s).log10()).abs();
                hf_cnt += 1;
            }
        }
    }
    let lf_snr = if lf_e == 0.0 { 200.0 } else { 10.0 * (lf_s / lf_e).log10() };
    let hf = if hf_cnt == 0 { 0.0 } else { hf_err / hf_cnt as f64 };
    (snr, lf_snr, hf)
}

/// Real-input power spectrum of `x` (length power of two), naive radix-2.
fn power_spectrum(x: &[f64]) -> Vec<f64> {
    let n = x.len();
    let mut re: Vec<f64> = x.to_vec();
    let mut im = vec![0.0f64; n];
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * std::f64::consts::PI / len as f64;
        for start in (0..n).step_by(len) {
            for k in 0..len / 2 {
                let (wr, wi) = ((ang * k as f64).cos(), (ang * k as f64).sin());
                let (ar, ai) = (re[start + k + len / 2], im[start + k + len / 2]);
                let (tr, ti) = (ar * wr - ai * wi, ar * wi + ai * wr);
                re[start + k + len / 2] = re[start + k] - tr;
                im[start + k + len / 2] = im[start + k] - ti;
                re[start + k] += tr;
                im[start + k] += ti;
            }
        }
        len <<= 1;
    }
    (0..n).map(|k| re[k] * re[k] + im[k] * im[k]).collect()
}

fn clips(repo: &Path) -> Vec<Clip> {
    let mut v = vec![
        Clip { name: "sine440", class: "tonal", pcm: vec![sine(440.0, 0.5)] },
        Clip { name: "noise", class: "noise", pcm: vec![noise(0.4, 1)] },
        Clip { name: "mix", class: "tone+noise", pcm: vec![mix(1), mix(2)] },
        Clip { name: "tremolo", class: "stereo tonal", pcm: tremolo() },
        Clip { name: "click", class: "transient", pcm: vec![clicks()] },
        Clip { name: "voice-like", class: "harmonic+HF", pcm: vec![voice(3), voice(4)] },
    ];
    if let Some(pcm) = lecture(repo) {
        v.push(Clip { name: "lecture", class: "speech (0.25 s)", pcm });
    }
    v
}

fn sine(hz: f64, amp: f32) -> Vec<f32> {
    (0..N).map(|i| amp * ((2.0 * std::f64::consts::PI * hz * i as f64 / RATE as f64).sin() as f32)).collect()
}

fn lcg(seed: &mut u32) -> f32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 5;
    (*seed as f32 / u32::MAX as f32) - 0.5
}

fn noise(amp: f32, seed: u32) -> Vec<f32> {
    let mut s = 0x1234_5678u32.wrapping_mul(seed.max(1));
    (0..N).map(|_| 2.0 * amp * lcg(&mut s)).collect()
}

fn mix(seed: u32) -> Vec<f32> {
    let t = sine(440.0 + 100.0 * seed as f64, 0.3);
    let n = noise(0.05, seed);
    t.iter().zip(&n).map(|(a, b)| a + b).collect()
}

fn tremolo() -> Vec<Vec<f32>> {
    let l: Vec<f32> = (0..N)
        .map(|i| {
            let t = i as f64 / RATE as f64;
            let env = 0.5 + 0.5 * (2.0 * std::f64::consts::PI * 8.0 * t).sin();
            (0.4 * env * (2.0 * std::f64::consts::PI * 880.0 * t).sin()) as f32
        })
        .collect();
    let r = l.iter().map(|v| 0.8 * v).collect();
    vec![l, r]
}

fn clicks() -> Vec<f32> {
    let mut v = vec![0.0f32; N];
    let mut s = 99u32;
    for i in 0..N {
        v[i] = 0.02 * lcg(&mut s);
    }
    for c in 0..(SECS * 4) {
        let at = c * RATE as usize / 4 + 300;
        for k in 0..64 {
            v[at + k] = 0.8 * (1.0 - k as f32 / 64.0) * if k % 2 == 0 { 1.0 } else { -1.0 };
        }
    }
    v
}

fn voice(seed: u32) -> Vec<f32> {
    let mut out = vec![0.0f32; N];
    for h in 1..=40 {
        let f = 150.0 * h as f64;
        for (i, v) in out.iter_mut().enumerate() {
            let vib = 1.0 + 0.01 * (2.0 * std::f64::consts::PI * 5.0 * i as f64 / RATE as f64).sin();
            *v += 0.012 * ((2.0 * std::f64::consts::PI * f * vib * i as f64 / RATE as f64).sin() as f32);
        }
    }
    let mut s = 7u32.wrapping_mul(seed);
    let (mut y1, mut y2) = (0.0f32, 0.0f32);
    for (i, v) in out.iter_mut().enumerate() {
        let x = lcg(&mut s);
        y1 = 0.55 * (y1 + x - y2);
        y2 = x;
        let duck = if (i / 19_200) % 2 == 0 { 1.0 } else { 0.25 };
        *v += 0.18 * duck * y1 + 0.01 * x;
    }
    out
}

fn lecture(repo: &Path) -> Option<Vec<Vec<f32>>> {
    let bytes = fs::read(repo.join("src/goldens/lecture.m4a")).ok()?;
    let dec = decode_with(&bytes, &DecodeOptions::unbounded()).ok()?;
    Some(dec.channels)
}

fn cpu_and_delay() {
    let pcm = vec![noise(0.3, 5).repeat(5), noise(0.3, 6).repeat(5)]; // 10 s stereo
    println!();
    println!("| mode | req kbps | encode ms / 10 s stereo | ×realtime | declared priming (output samples) | extra internal latency |");
    println!("|---|---:|---:|---:|---:|---|");
    for (mode, bps, lat) in [
        (Mode::Lc, 128_000u32, "none (causal)"),
        (Mode::Lc, 48_000, "none (causal)"),
        (Mode::LcLookahead, 128_000, "+1024 samples (held frame)"),
        (Mode::He, 48_000, "none beyond the SBR window"),
        (Mode::He, 24_000, "none beyond the SBR window"),
    ] {
        let o = mode.opts(bps);
        let _ = encode_with(&pcm, RATE, &o);
        let mut best = f64::MAX;
        for _ in 0..3 {
            let t = Instant::now();
            let _ = encode_with(&pcm, RATE, &o);
            best = best.min(t.elapsed().as_secs_f64() * 1e3);
        }
        println!(
            "| {} | {} | {:.1} | {:.0}× | {} | {} |",
            mode.tag(),
            bps / 1000,
            best,
            10_000.0 / best,
            mode.priming(),
            lat
        );
    }
}
