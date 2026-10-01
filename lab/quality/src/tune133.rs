//! TASK-133 LC-core tuning iterator: syom-only cells on the TASK-95
//! deficit classes (tremolo, lecture, lecture-m) plus the guard classes
//! (noise-st, mix-st). Same clips/metrics as `syom_he_qualify` (neutral
//! ffmpeg decode, declared-delay ±128 xcorr, waveform SNR, LF SNR <
//! 6.75 kHz, HF band-level error). Isolated; never run by `cargo test`.
//!
//! `syom_tune133 speech2` rebuilds the second speech clip from the seeds
//! below and scores HE v1 at 24 kbps for syom and the in-tree FDK driver
//! on that clip and on lecture / lecture-m. The PCM is not stored.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use syom::{DecodeOptions, EncodeOptions, decode_with, encode_with};

const RATE: u32 = 48_000;
const SECS: usize = 2;
const N: usize = RATE as usize * SECS;
const SPLIT_HZ: f64 = 6_750.0;
const TOP_HZ: f64 = 15_375.0;
/// Not the voice-like pair (seeds 3/4, f0 150/113) and not `lecture.m4a`.
const SPEECH2_L_SEED: u32 = 41;
const SPEECH2_L_F0: f64 = 132.0;
const SPEECH2_L_F1: f64 = 270.0;
const SPEECH2_L_F2: f64 = 2_290.0;
const SPEECH2_R_SEED: u32 = 73;
const SPEECH2_R_F0: f64 = 196.0;
const SPEECH2_R_F1: f64 = 730.0;
const SPEECH2_R_F2: f64 = 1_090.0;
/// 8 ms bursts: sample 0, and 1.05 s.
const SPEECH2_PLOSIVE_AT: [usize; 2] = [0, 50_400];
const HE1_24_BPS: u32 = 24_000;
const SYOM_HE_DELAY: i32 = 3018;

struct Clip {
    name: &'static str,
    pcm: Vec<Vec<f32>>,
}

fn main() -> ExitCode {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo = root.parent().unwrap().parent().unwrap();
    // Optional cell filter: `tune133 [clip] [mode] [kbps]` substrings.
    // `speech2` is the second-clip HE v1 24 kbps arm (syom and FDK).
    let flt: Vec<String> = std::env::args().skip(1).collect();
    if flt.first().map(String::as_str) == Some("speech2") {
        return speech2_report(repo);
    }
    let work = std::env::temp_dir().join("syom-tune133");
    let _ = fs::remove_dir_all(&work);
    fs::create_dir_all(&work).expect("work dir");
    let keep = |clip: &str, mode: &str, kbps: u32| {
        flt.iter().all(|f| {
            clip.contains(f.as_str())
                || mode.contains(f.as_str())
                || f.parse::<u32>().map_or(false, |v| v == kbps / 1000)
        })
    };
    println!("| clip | engine | req k | actual k | delay | SNR | LF SNR | HF err | note |");
    println!("|---|---|---:|---:|---:|---:|---:|---:|---|");
    for clip in clips(repo) {
        let ch = clip.pcm.len();
        let rates: &[u32] = if ch == 1 {
            &[24_000]
        } else {
            &[24_000, 32_000, 48_000, 64_000, 128_000]
        };
        for &bps in rates {
            for he in [false, true] {
                let mode = if he { "he1" } else { "lc" };
                if !keep(clip.name, mode, bps) {
                    continue;
                }
                row(&work, &clip, he, bps);
            }
        }
    }
    ExitCode::SUCCESS
}

/// HE v1 at 24 kbps on `speech2` / `speech2-m` and lecture / lecture-m.
/// Syom delay is the product priming (3018); FDK delay is the driver
/// `nDelay`. Both then use the same ±128 sample refinement as `row`.
fn speech2_report(repo: &Path) -> ExitCode {
    let work = std::env::temp_dir().join("syom-tune133-speech2");
    let _ = fs::remove_dir_all(&work);
    fs::create_dir_all(&work).expect("work dir");
    println!(
        "speech2: {SECS} s, {RATE} Hz, voice() + two formants + plosives at {} and {}",
        SPEECH2_PLOSIVE_AT[0], SPEECH2_PLOSIVE_AT[1]
    );
    println!(
        "L seed {SPEECH2_L_SEED} f0 {SPEECH2_L_F0} F1 {SPEECH2_L_F1} F2 {SPEECH2_L_F2}; \
         R seed {SPEECH2_R_SEED} f0 {SPEECH2_R_F0} F1 {SPEECH2_R_F1} F2 {SPEECH2_R_F2}; \
         mono = mean"
    );
    let stereo = speech2_stereo();
    let mono = speech2_mono(&stereo);
    let Some(lec) = lecture(repo) else {
        eprintln!("lecture.m4a missing");
        return ExitCode::from(2);
    };
    let n = lec[0].len().min(lec.get(1).map_or(0, Vec::len));
    let lec_m: Vec<f32> = (0..n).map(|i| 0.5 * (lec[0][i] + lec[1][i])).collect();
    let clips = [
        Clip {
            name: "speech2",
            pcm: stereo,
        },
        Clip {
            name: "speech2-m",
            pcm: vec![mono],
        },
        Clip {
            name: "lecture",
            pcm: lec,
        },
        Clip {
            name: "lecture-m",
            pcm: vec![lec_m],
        },
    ];
    println!("| clip | engine | req k | actual k | delay | SNR | LF SNR | HF err | note |");
    println!("|---|---|---:|---:|---:|---:|---:|---:|---|");
    let mut failed = false;
    for clip in &clips {
        if let Err(err) = speech2_syom(&work, clip) {
            failed = true;
            println!(
                "| {} | syom-he1 | 24 | — | — | — | — | — | {} |",
                clip.name,
                err.replace('|', "/")
            );
        }
        if let Err(err) = speech2_fdk(repo, &work, clip) {
            failed = true;
            println!(
                "| {} | fdk-he1 | 24 | — | — | — | — | — | {} |",
                clip.name,
                err.replace('|', "/")
            );
        }
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn speech2_syom(work: &Path, clip: &Clip) -> Result<(), String> {
    let opts = EncodeOptions::adts()
        .with_bitrate_bps(HE1_24_BPS)
        .with_he(true);
    let adts = encode_with(&clip.pcm, RATE, &opts).map_err(|e| e.to_string())?;
    let tag = format!("{}-syom-he1-24", clip.name);
    let score = score_adts(work, &tag, &clip.pcm, &adts, SYOM_HE_DELAY)?;
    print_score(clip.name, "syom-he1", &score);
    Ok(())
}

fn speech2_fdk(repo: &Path, work: &Path, clip: &Clip) -> Result<(), String> {
    let tag = format!("{}-fdk-he1-24", clip.name);
    let planar = work.join(format!("{tag}.f32"));
    let adts_path = work.join(format!("{tag}.adts"));
    let mut buf = Vec::new();
    for ch in &clip.pcm {
        for &x in ch {
            buf.extend_from_slice(&x.to_le_bytes());
        }
    }
    fs::write(&planar, &buf).map_err(|e| e.to_string())?;
    let bin = repo.join("lab/fdk/fdk_driver");
    let out = Command::new(&bin)
        .args([
            "encode-pcm",
            &RATE.to_string(),
            &clip.pcm.len().to_string(),
            &HE1_24_BPS.to_string(),
            "5",
            planar.to_str().ok_or("planar path")?,
            adts_path.to_str().ok_or("adts path")?,
        ])
        .output()
        .map_err(|e| e.to_string())?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    if !out.status.success() {
        return Err(format!("fdk: {}", stdout.trim()));
    }
    let delay = json_delay(&stdout).ok_or_else(|| format!("no delay in {stdout}"))?;
    let adts = fs::read(&adts_path).map_err(|e| e.to_string())?;
    let score = score_adts(work, &tag, &clip.pcm, &adts, delay)?;
    print_score(clip.name, "fdk-he1", &score);
    Ok(())
}

struct CellScore {
    actual_k: f64,
    lag: i32,
    snr: f64,
    lf: f64,
    hf: f64,
    note: String,
}

fn print_score(clip: &str, engine: &str, s: &CellScore) {
    println!(
        "| {clip} | {engine} | 24 | {actual:.1} | {lag} | {snr:.1} | {lf:.1} | {hf:.1} | {note} |",
        actual = s.actual_k,
        lag = s.lag,
        snr = s.snr,
        lf = s.lf,
        hf = s.hf,
        note = s.note,
    );
}

fn json_delay(s: &str) -> Option<i32> {
    let key = "\"delay\":";
    let i = s.find(key)? + key.len();
    let rest = &s[i..];
    let end = rest.find([',', '}']).unwrap_or(rest.len());
    rest[..end].trim().parse().ok()
}

/// Same alignment and band metrics as [`row`].
fn score_adts(
    work: &Path,
    tag: &str,
    pcm: &[Vec<f32>],
    adts: &[u8],
    declared: i32,
) -> Result<CellScore, String> {
    let ch = pcm.len();
    let path = work.join(format!("{tag}.scored.adts"));
    fs::write(&path, adts).map_err(|e| e.to_string())?;
    let secs = pcm[0].len() as f64 / RATE as f64;
    let actual = adts.len() as f64 * 8.0 / secs / 1000.0;
    let out_ch = ffprobe_channels(&path).unwrap_or(ch);
    let dec = decode_ffmpeg(&path, out_ch)?;
    let dec = if ch == 1 && dec.len() == 2 {
        vec![dec[0].clone()]
    } else {
        dec
    };
    if dec.len() != ch || rms(&dec[0]) < 1e-6 {
        return Err("silent/shape".into());
    }
    let lag = refine_lag(&pcm[0], &dec[0], declared, 128);
    let mut note = String::new();
    if (lag - declared).abs() > 96 {
        note = format!("align drift declared={declared} used={lag}");
    }
    let (mut ps, mut pe, mut lf, mut hf) = (0.0f64, 0.0f64, 0.0, 0.0);
    for c in 0..ch {
        let (w, g) = aligned(&pcm[c], &dec[c], lag);
        for i in 0..w.len() {
            let s = f64::from(w[i]);
            let er = s - f64::from(g[i]);
            ps += s * s;
            pe += er * er;
        }
        let (l, h) = band_measures(w, g);
        lf += l / ch as f64;
        hf += h / ch as f64;
    }
    let snr = if pe == 0.0 {
        200.0
    } else {
        10.0 * (ps / pe).log10()
    };
    Ok(CellScore {
        actual_k: actual,
        lag,
        snr,
        lf,
        hf,
        note,
    })
}

fn row(work: &Path, clip: &Clip, he: bool, bps: u32) {
    let ch = clip.pcm.len();
    let mode = if he { "he1" } else { "lc" };
    let head = format!("| {} | {} | {} |", clip.name, mode, bps / 1000);
    let opts = EncodeOptions::adts().with_bitrate_bps(bps).with_he(he);
    let adts = match encode_with(&clip.pcm, RATE, &opts) {
        Ok(a) => a,
        Err(e) => {
            println!("{head} — | — | — | — | — | encode: {e} |");
            return;
        }
    };
    let tag = format!("{}-{}-{}", clip.name, mode, bps / 1000);
    let path = work.join(format!("{tag}.adts"));
    fs::write(&path, &adts).expect("write adts");
    let secs = clip.pcm[0].len() as f64 / RATE as f64;
    let actual = adts.len() as f64 * 8.0 / secs / 1000.0;
    let out_ch = ffprobe_channels(&path).unwrap_or(ch);
    let dec = match decode_ffmpeg(&path, out_ch) {
        Ok(d) => d,
        Err(err) => {
            println!(
                "{head} {actual:.1} | — | — | — | — | decode: {} |",
                err.replace('|', "/")
            );
            return;
        }
    };
    let mut note = String::new();
    let dec = if ch == 1 && dec.len() == 2 {
        vec![dec[0].clone()]
    } else {
        dec
    };
    if dec.len() != ch || rms(&dec[0]) < 1e-6 {
        println!("{head} {actual:.1} | — | — | — | — | silent/shape |");
        return;
    }
    let declared = if he { 3018 } else { 1024 };
    let lag = refine_lag(&clip.pcm[0], &dec[0], declared, 128);
    if (lag - declared).abs() > 96 {
        note = format!("align drift declared={declared} used={lag}");
    }
    let (mut ps, mut pe, mut lf, mut hf) = (0.0f64, 0.0f64, 0.0, 0.0);
    for c in 0..ch {
        let (w, g) = aligned(&clip.pcm[c], &dec[c], lag);
        for i in 0..w.len() {
            let s = f64::from(w[i]);
            let er = s - f64::from(g[i]);
            ps += s * s;
            pe += er * er;
        }
        let (l, h) = band_measures(w, g);
        lf += l / ch as f64;
        hf += h / ch as f64;
    }
    let snr = if pe == 0.0 {
        200.0
    } else {
        10.0 * (ps / pe).log10()
    };
    if std::env::var_os("TUNE133_TRACE").is_some() {
        let (w, g) = aligned(&clip.pcm[0], &dec[0], lag);
        for (i, (wc, gc)) in w.chunks(2048).zip(g.chunks(2048)).enumerate() {
            let (mut s, mut e) = (0.0f64, 0.0f64);
            for k in 0..wc.len().min(gc.len()) {
                s += f64::from(wc[k]) * f64::from(wc[k]);
                let d = f64::from(wc[k]) - f64::from(gc[k]);
                e += d * d;
            }
            let v = if e == 0.0 {
                200.0
            } else {
                10.0 * (s / e.max(1e-30)).log10()
            };
            let (l, _) = band_measures(wc, gc);
            eprintln!("trace {i} snr={v:.1} lf={l:.1}");
        }
    }
    println!("{head} {actual:.1} | {lag} | {snr:.1} | {lf:.1} | {hf:.1} | {note} |");
}

fn aligned<'a>(r: &'a [f32], d: &'a [f32], lag: i32) -> (&'a [f32], &'a [f32]) {
    if lag >= 0 {
        let k = lag as usize;
        let n = r.len().min(d.len().saturating_sub(k));
        (&r[..n], &d[k..k + n])
    } else {
        let k = (-lag) as usize;
        let n = (r.len().saturating_sub(k)).min(d.len());
        (&r[k..k + n], &d[..n])
    }
}

/// (LF spectral SNR, HF mean |band level error| dB) — he_qual.rs method.
fn band_measures(w: &[f32], g: &[f32]) -> (f64, f64) {
    let fft_n = 2048usize;
    let split_bin = (SPLIT_HZ / (RATE as f64 / fft_n as f64)) as usize;
    let top_bin = (TOP_HZ / (RATE as f64 / fft_n as f64)) as usize;
    let (mut lf_s, mut lf_e, mut hf_err, mut hf_cnt) = (0.0f64, 0.0f64, 0.0f64, 0usize);
    let blocks = w.len().min(g.len()) / fft_n;
    for b in 0..blocks {
        let mut bw = vec![0.0f64; fft_n];
        let mut bg = vec![0.0f64; fft_n];
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
    let lf = if lf_e == 0.0 {
        200.0
    } else {
        10.0 * (lf_s / lf_e).log10()
    };
    (
        lf,
        if hf_cnt == 0 {
            0.0
        } else {
            hf_err / hf_cnt as f64
        },
    )
}

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
                let (er, ei) = (re[start + k], im[start + k]);
                let (or_, oi) = (re[start + k + len / 2], im[start + k + len / 2]);
                let (tr, ti) = (or_ * wr - oi * wi, or_ * wi + oi * wr);
                re[start + k] = er + tr;
                im[start + k] = ei + ti;
                re[start + k + len / 2] = er - tr;
                im[start + k + len / 2] = ei - ti;
            }
        }
        len <<= 1;
    }
    (0..n).map(|k| re[k] * re[k] + im[k] * im[k]).collect()
}

fn rms(x: &[f32]) -> f64 {
    if x.is_empty() {
        return 0.0;
    }
    (x.iter().map(|&v| f64::from(v) * f64::from(v)).sum::<f64>() / x.len() as f64).sqrt()
}

fn corr_n(a: &[f32], b: &[f32]) -> f64 {
    let (mut s, mut ea, mut eb) = (0.0, 0.0, 0.0);
    for (&x, &y) in a.iter().zip(b) {
        let (x, y) = (f64::from(x), f64::from(y));
        s += x * y;
        ea += x * x;
        eb += y * y;
    }
    if ea * eb == 0.0 {
        0.0
    } else {
        s / (ea * eb).sqrt()
    }
}

fn refine_lag(r: &[f32], d: &[f32], center: i32, radius: i32) -> i32 {
    let (mut best_lag, mut best) = (center, f64::NEG_INFINITY);
    for lag in (center - radius).max(0)..=(center + radius) {
        let (a, b) = aligned(r, d, lag);
        if a.len() < r.len() / 2 {
            continue;
        }
        let c = corr_n(a, b);
        if c > best + 1e-9 {
            best = c;
            best_lag = lag;
        }
    }
    best_lag
}

fn ffprobe_channels(adts: &Path) -> Option<usize> {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=channels",
            "-of",
            "csv=p=0",
        ])
        .arg(adts)
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

fn decode_ffmpeg(adts: &Path, ch: usize) -> Result<Vec<Vec<f32>>, String> {
    let out = adts.with_extension("dec.f32");
    let st = Command::new("ffmpeg")
        .args(["-y", "-v", "error", "-i"])
        .arg(adts)
        .args(["-f", "f32le", "-acodec", "pcm_f32le"])
        .arg(&out)
        .output()
        .map_err(|e| e.to_string())?;
    if !st.status.success() {
        return Err(String::from_utf8_lossy(&st.stderr).trim().into());
    }
    let raw = fs::read(&out).map_err(|e| e.to_string())?;
    let frames = raw.len() / 4 / ch;
    let mut planes = vec![Vec::with_capacity(frames); ch];
    for i in 0..frames {
        for (c, p) in planes.iter_mut().enumerate() {
            let off = (i * ch + c) * 4;
            p.push(f32::from_le_bytes(
                raw[off..off + 4].try_into().expect("4 bytes"),
            ));
        }
    }
    Ok(planes)
}

fn clips(repo: &Path) -> Vec<Clip> {
    let mut v = vec![
        Clip {
            name: "tremolo",
            pcm: tremolo(),
        },
        Clip {
            name: "noise-st",
            pcm: vec![noise(0.35, 11), noise(0.35, 12)],
        },
        Clip {
            name: "mix-st",
            pcm: vec![mix(1), mix(2)],
        },
        Clip {
            name: "voice-like",
            pcm: vec![voice(3, 150.0), voice(4, 113.0)],
        },
        Clip {
            name: "click-st",
            pcm: vec![clicks(21), clicks(22)],
        },
        Clip {
            name: "ambience",
            pcm: ambience(),
        },
    ];
    if let Some(pcm) = lecture(repo) {
        let mono: Vec<f32> = (0..pcm[0].len().min(pcm.get(1).map_or(0, Vec::len)))
            .map(|i| 0.5 * (pcm[0][i] + pcm[1][i]))
            .collect();
        v.push(Clip {
            name: "lecture",
            pcm,
        });
        v.push(Clip {
            name: "lecture-m",
            pcm: vec![mono],
        });
    }
    v
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

fn ambience() -> Vec<Vec<f32>> {
    let tone = |hz: f64, amp: f32| -> Vec<f32> {
        (0..N)
            .map(|i| amp * (2.0 * std::f64::consts::PI * hz * i as f64 / RATE as f64).sin() as f32)
            .collect()
    };
    let l: Vec<f32> = tone(880.0, 0.30)
        .iter()
        .zip(noise(0.06, 31))
        .map(|(a, b)| a + b)
        .collect();
    let r: Vec<f32> = tone(1_320.0, 0.15)
        .iter()
        .zip(noise(0.06, 32))
        .map(|(a, b)| a + b)
        .collect();
    vec![l, r]
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

/// [`voice`] passed through two fixed resonators, then two noise bursts.
/// Formant gains are textbook vowel levels (F2 about 6 dB under F1), not
/// a fit to the 20 dB cutoff. Mono is the channel mean, as lecture-m is.
fn speech2_stereo() -> Vec<Vec<f32>> {
    vec![
        speech2_channel(SPEECH2_L_SEED, SPEECH2_L_F0, SPEECH2_L_F1, SPEECH2_L_F2),
        speech2_channel(SPEECH2_R_SEED, SPEECH2_R_F0, SPEECH2_R_F1, SPEECH2_R_F2),
    ]
}

fn speech2_mono(stereo: &[Vec<f32>]) -> Vec<f32> {
    stereo[0]
        .iter()
        .zip(&stereo[1])
        .map(|(l, r)| 0.5 * (l + r))
        .collect()
}

fn speech2_channel(seed: u32, f0: f64, f1: f64, f2: f64) -> Vec<f32> {
    let src = voice(seed, f0);
    let low = resonator(&src, f1, 90.0);
    let high = resonator(&src, f2, 140.0);
    let mut out = mix_peaks(&low, 0.55, &high, 0.28);
    for &at in &SPEECH2_PLOSIVE_AT {
        add_plosive(&mut out, seed, at);
    }
    let peak = out.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    if peak > 0.9 {
        let g = 0.9 / peak;
        for s in &mut out {
            *s *= g;
        }
    }
    out
}

fn resonator(x: &[f32], hz: f64, bw: f64) -> Vec<f64> {
    let r = (-std::f64::consts::PI * bw / f64::from(RATE)).exp();
    let theta = 2.0 * std::f64::consts::PI * hz / f64::from(RATE);
    let a1 = 2.0 * r * theta.cos();
    let a2 = -(r * r);
    let (mut y1, mut y2) = (0.0f64, 0.0f64);
    let mut y = Vec::with_capacity(x.len());
    for &s in x {
        let yn = f64::from(s) + a1 * y1 + a2 * y2;
        y2 = y1;
        y1 = yn;
        y.push(yn);
    }
    y
}

fn mix_peaks(a: &[f64], pa: f64, b: &[f64], pb: f64) -> Vec<f32> {
    let scale = |x: &[f64], peak: f64| {
        let m = x.iter().fold(0.0f64, |acc, v| acc.max(v.abs()));
        if m == 0.0 { 0.0 } else { peak / m }
    };
    let (ga, gb) = (scale(a, pa), scale(b, pb));
    a.iter()
        .zip(b)
        .map(|(u, v)| (ga * u + gb * v) as f32)
        .collect()
}

fn add_plosive(x: &mut [f32], seed: u32, at: usize) {
    let mut s = 0xC0FF_EE00u32
        .wrapping_mul(seed.max(1))
        .wrapping_add((at as u32) | 1);
    let mut prev = 0.0f32;
    let n = 384.min(x.len().saturating_sub(at));
    for k in 0..n {
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        let noise = (s as f32 / u32::MAX as f32) - 0.5;
        let hp = noise - prev;
        prev = noise;
        let env = (-(k as f32) / 64.0).exp();
        x[at + k] += 0.9 * env * hp;
    }
}

fn lecture(repo: &Path) -> Option<Vec<Vec<f32>>> {
    let bytes = fs::read(repo.join("src/goldens/lecture.m4a")).ok()?;
    let dec = decode_with(&bytes, &DecodeOptions::unbounded()).ok()?;
    Some(dec.channels)
}
