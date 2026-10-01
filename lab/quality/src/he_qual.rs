//! TASK-95 HE v1/v2 low-rate qualification matrix. Isolated; never run
//! by `cargo test`. Every engine encodes the same dev synth clips; every
//! ADTS stream is decoded by the neutral ffmpeg 7.0.2 CLI (offline
//! oracle). Metrics: waveform SNR after xcorr alignment (not PEAQ),
//! spectral LF SNR (< 6.75 kHz), HF band-level error (6.75–15.375 kHz,
//! the SBR range), and stereo ILD/ICC error (PS is never qualified by a
//! mono metric). Timing/memory peers run as processes via peer_run.py;
//! syom additionally has a `self` subcommand so it is measurable in the
//! same process lane.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::Instant;
use syom::ps_measure::encode_he_v2_adts;
use syom::{decode_with, encode_with, DecodeOptions, EncodeOptions};

const RATE: u32 = 48_000;
const SECS: usize = 2;
const N: usize = RATE as usize * SECS;
const SPLIT_HZ: f64 = 6_750.0;
const TOP_HZ: f64 = 15_375.0;

struct Clip {
    name: &'static str,
    class: &'static str,
    pcm: Vec<Vec<f32>>,
}

#[derive(Clone, Copy, PartialEq)]
enum Engine {
    SyomLc,
    SyomHe,
    SyomHe2,
    FdkLc,
    FdkHe,
    FdkHe2,
    OxLc,
    OxHe,
    FaacLc,
    AvcLc,
}

impl Engine {
    fn tag(self) -> &'static str {
        match self {
            Engine::SyomLc => "syom-lc",
            Engine::SyomHe => "syom-he1",
            Engine::SyomHe2 => "syom-he2",
            Engine::FdkLc => "fdk-lc",
            Engine::FdkHe => "fdk-he1",
            Engine::FdkHe2 => "fdk-he2",
            Engine::OxLc => "oxideav-lc",
            Engine::OxHe => "oxideav-he1",
            Engine::FaacLc => "faac-lc",
            Engine::AvcLc => "lavc9-lc",
        }
    }
    /// HE v2 / PS is stereo-only.
    fn stereo_only(self) -> bool {
        matches!(self, Engine::SyomHe2 | Engine::FdkHe2)
    }
    /// Declared encoder delay (output samples, ffmpeg-decode path):
    /// syom product priming constants, FDK `AACENC_InfoStruct.nDelay`,
    /// oxideav/FAAC from the 2026-09-23 impulse probe (lab/quality
    /// HE_QUALIFY.md). `None` = free envelope alignment (FAAC).
    fn declared_delay(self, fdk_json_delay: Option<i32>) -> Option<i32> {
        match self {
            Engine::SyomLc | Engine::AvcLc => Some(1024),
            Engine::SyomHe | Engine::SyomHe2 => Some(3018),
            Engine::FdkLc | Engine::FdkHe | Engine::FdkHe2 => fdk_json_delay,
            Engine::OxLc => Some(1024),
            Engine::OxHe => Some(3030),
            Engine::FaacLc => None,
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("iid") {
        return iid_grid();
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo = root.parent().unwrap().parent().unwrap();
    if args.len() == 7 && args[1] == "self" {
        return self_encode(&args);
    }
    let work = std::env::temp_dir().join("syom-he-qualify");
    let _ = fs::remove_dir_all(&work);
    fs::create_dir_all(&work).expect("work dir");
    matrix(repo, &work);
    timing(repo, &work);
    ExitCode::SUCCESS
}

/// `self MODE CH BPS IN.planar-f32le OUT.adts`: one syom encode, process lane.
fn self_encode(args: &[String]) -> ExitCode {
    let ch: usize = args[3].parse().unwrap_or(0);
    let bps: u32 = args[4].parse().unwrap_or(0);
    let raw = fs::read(&args[5]).expect("read pcm");
    if ch == 0 || raw.len() % (ch * 4) != 0 {
        eprintln!("self MODE CH BPS IN.f32 OUT.adts");
        return ExitCode::from(2);
    }
    let n = raw.len() / 4 / ch;
    let planes: Vec<Vec<f32>> = (0..ch)
        .map(|c| {
            raw[c * n * 4..(c + 1) * n * 4]
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes(b.try_into().expect("4 bytes")))
                .collect()
        })
        .collect();
    let opts = EncodeOptions::adts().with_bitrate_bps(bps);
    let opts = match args[2].as_str() {
        "lc" => opts,
        "he1" => opts.with_he(true),
        "he2" => opts.with_he_v2(true),
        _ => {
            eprintln!("mode lc|he1|he2");
            return ExitCode::from(2);
        }
    };
    match encode_with(&planes, RATE, &opts) {
        Ok(adts) => {
            fs::write(&args[6], &adts).expect("write adts");
            println!(
                "{{\"ok\":true,\"engine\":\"syom\",\"adts_bytes\":{}}}",
                adts.len()
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            println!("{{\"ok\":false,\"error\":\"{e}\"}}");
            ExitCode::FAILURE
        }
    }
}

fn syom_opts(e: Engine, bps: u32) -> EncodeOptions {
    let o = EncodeOptions::adts().with_bitrate_bps(bps);
    match e {
        Engine::SyomHe => o.with_he(true),
        Engine::SyomHe2 => o.with_he_v2(true),
        _ => o,
    }
}

/// Encode one cell; returns (adts bytes, encode wall ms, driver-declared
/// delay if the engine reports one) or an error note.
fn encode_cell(
    repo: &Path,
    work: &Path,
    clip: &Clip,
    e: Engine,
    bps: u32,
) -> Result<(Vec<u8>, f64, Option<i32>), String> {
    let tag = format!("{}-{}-{}", clip.name, e.tag(), bps / 1000);
    let planar = work.join(format!("{tag}.f32"));
    let adts = work.join(format!("{tag}.adts"));
    let t0 = Instant::now();
    let mut declared = None;
    match e {
        Engine::SyomLc | Engine::SyomHe | Engine::SyomHe2 => {
            let out =
                encode_with(&clip.pcm, RATE, &syom_opts(e, bps)).map_err(|e| e.to_string())?;
            return Ok((out, t0.elapsed().as_secs_f64() * 1e3, None));
        }
        Engine::FdkLc | Engine::FdkHe | Engine::FdkHe2 => {
            write_planar(&planar, &clip.pcm);
            let aot = match e {
                Engine::FdkLc => "2",
                Engine::FdkHe => "5",
                _ => "29",
            };
            declared = run_driver(
                &repo.join("lab/fdk/fdk_driver"),
                &[
                    "encode-pcm".into(),
                    RATE.to_string(),
                    clip.pcm.len().to_string(),
                    bps.to_string(),
                    aot.into(),
                    planar.to_string_lossy().into(),
                    adts.to_string_lossy().into(),
                ],
            )?
            .and_then(|s| json_get(&s, "delay").map(|v| v as i32));
        }
        Engine::OxLc | Engine::OxHe => {
            write_planar(&planar, &clip.pcm);
            let mode = if e == Engine::OxHe { "he" } else { "lc" };
            run_driver(
                &repo.join("lab/oxideav/target/release/oxideav_driver"),
                &[
                    "encode-pcm".into(),
                    RATE.to_string(),
                    clip.pcm.len().to_string(),
                    bps.to_string(),
                    mode.into(),
                    planar.to_string_lossy().into(),
                    adts.to_string_lossy().into(),
                ],
            )?;
        }
        Engine::FaacLc => {
            write_planar(&planar, &clip.pcm);
            run_driver(
                &repo.join("lab/faac/faac_driver"),
                &[
                    "encode-pcm".into(),
                    RATE.to_string(),
                    clip.pcm.len().to_string(),
                    (bps / clip.pcm.len() as u32).to_string(),
                    planar.to_string_lossy().into(),
                    adts.to_string_lossy().into(),
                ],
            )?;
        }
        Engine::AvcLc => {
            write_planar(&planar, &clip.pcm);
            run_driver(
                &repo.join("lab/libavcodec/avc_driver"),
                &[
                    "encode-lc-adts".into(),
                    RATE.to_string(),
                    clip.pcm.len().to_string(),
                    bps.to_string(),
                    planar.to_string_lossy().into(),
                    adts.to_string_lossy().into(),
                ],
            )?;
        }
    }
    let ms = t0.elapsed().as_secs_f64() * 1e3;
    let bytes = fs::read(&adts).map_err(|e| e.to_string())?;
    Ok((bytes, ms, declared))
}

fn run_driver(bin: &Path, args: &[String]) -> Result<Option<String>, String> {
    if !bin.is_file() {
        return Err(format!("missing {}", bin.display()));
    }
    let out = Command::new(bin)
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!(
            "{}: {}",
            bin.file_name().unwrap_or_default().to_string_lossy(),
            String::from_utf8_lossy(&out.stdout)
        ));
    }
    Ok(String::from_utf8(out.stdout)
        .ok()
        .map(|s| s.trim().to_string()))
}

struct Probe {
    profile: String,
    rate: u32,
    channels: usize,
}

fn ffprobe(adts: &Path) -> Probe {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=profile,sample_rate,channels",
            "-of",
            "csv=p=0",
        ])
        .arg(adts)
        .output();
    let mut p = Probe {
        profile: "?".into(),
        rate: 0,
        channels: 0,
    };
    if let Ok(o) = out {
        let s = String::from_utf8_lossy(&o.stdout);
        let f: Vec<&str> = s.trim().split(',').collect();
        if f.len() >= 3 {
            p.profile = f[0].into();
            p.rate = f[1].parse().unwrap_or(0);
            p.channels = f[2].parse().unwrap_or(0);
        }
    }
    p
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
    Ok(deinterleave(&raw, ch.max(1)))
}

fn deinterleave(raw: &[u8], ch: usize) -> Vec<Vec<f32>> {
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
    planes
}

fn write_planar(path: &Path, pcm: &[Vec<f32>]) {
    let mut buf = Vec::new();
    for ch in pcm {
        for &x in ch {
            buf.extend_from_slice(&x.to_le_bytes());
        }
    }
    fs::write(path, buf).expect("write planar");
}

/// Aligned views of reference/degraded channel at `lag` (positive = deg late).
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

/// (LF spectral SNR, HF mean |band level error| dB) — curves.rs method.
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

/// Stereo spatial metrics over the aligned region: (ILD dB, ICC).
fn spatial(l: &[f32], r: &[f32]) -> (f64, f64) {
    let (mut el, mut er, mut lr, mut ml, mut mr) = (0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64);
    let n = l.len().min(r.len());
    for i in 0..n {
        let (a, b) = (f64::from(l[i]), f64::from(r[i]));
        el += a * a;
        er += b * b;
        ml += a;
        mr += b;
    }
    ml /= n as f64;
    mr /= n as f64;
    let (mut sl, mut sr) = (0.0f64, 0.0f64);
    for i in 0..n {
        let (a, b) = (f64::from(l[i]) - ml, f64::from(r[i]) - mr);
        lr += a * b;
        sl += a * a;
        sr += b * b;
    }
    let icc = if sl * sr == 0.0 {
        1.0
    } else {
        lr / (sl * sr).sqrt()
    };
    (10.0 * (el / er.max(1e-30)).log10(), icc)
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

fn matrix(repo: &Path, work: &Path) {
    let engines = [
        Engine::SyomLc,
        Engine::SyomHe,
        Engine::SyomHe2,
        Engine::FdkLc,
        Engine::FdkHe,
        Engine::FdkHe2,
        Engine::OxLc,
        Engine::OxHe,
        Engine::FaacLc,
        Engine::AvcLc,
    ];
    println!("# TASK-95 HE qualification matrix (48 kHz, {SECS} s dev synth + lecture golden)");
    println!();
    println!("Neutral decode: ffmpeg 7.0.2 pcm_f32le for every engine. `actual kbps` = 8·ADTS bytes / source seconds. `delay` = xcorr lag vs source (not the declared priming). `valid` = decoded samples after delay vs source N. SNR is a waveform error after alignment, not PEAQ. LF SNR < {SPLIT_HZ:.0} Hz; HF err = mean |level error| of 375 Hz bands in {SPLIT_HZ:.0}–{TOP_HZ:.0} Hz; ILD err / ICC err on stereo clips only.");
    println!();
    println!("| clip | class | engine | req k | actual k | enc ms | profile | out Hz/ch | delay | valid | SNR | LF SNR | HF err | ILD err | ICC err | note |");
    println!("|---|---|---|---:|---:|---:|---|---|---:|---:|---:|---:|---:|---:|---:|---|");
    for clip in &clips(repo) {
        for e in engines {
            let ch = clip.pcm.len();
            if e.stereo_only() && ch != 2 {
                continue;
            }
            let rates: &[u32] = match (e, ch) {
                (Engine::SyomHe2 | Engine::FdkHe2, _) => &[24_000, 32_000, 48_000],
                (_, 1) => &[24_000],
                _ => &[24_000, 32_000, 48_000, 64_000],
            };
            for &bps in rates {
                row(repo, work, clip, e, bps);
            }
        }
    }
}

fn row(repo: &Path, work: &Path, clip: &Clip, e: Engine, bps: u32) {
    let ch = clip.pcm.len();
    let head = format!(
        "| {} | {} | {} | {} |",
        clip.name,
        clip.class,
        e.tag(),
        bps / 1000
    );
    let (adts, enc_ms, drv_delay) = match encode_cell(repo, work, clip, e, bps) {
        Ok(v) => v,
        Err(err) => {
            println!(
                "{head} — | — | — | — | — | — | — | — | — | — | {} |",
                err.replace('|', "/")
            );
            return;
        }
    };
    let tag = format!("{}-{}-{}", clip.name, e.tag(), bps / 1000);
    let path = work.join(format!("{tag}.adts"));
    fs::write(&path, &adts).expect("write adts");
    let secs = clip.pcm[0].len() as f64 / RATE as f64;
    let actual = adts.len() as f64 * 8.0 / secs / 1000.0;
    let probe = ffprobe(&path);
    let dec = match decode_ffmpeg(&path, probe.channels) {
        Ok(d) => d,
        Err(err) => {
            println!("{head} {actual:.1} | {enc_ms:.1} | {} | {}/{} | — | — | — | — | — | — | decode: {} |",
                probe.profile, probe.rate, probe.channels, err.replace('|', "/"));
            return;
        }
    };
    let mut note = String::new();
    // Mono source, ffmpeg implicit-PS probe returns stereo: collapse when
    // the two channels are identical (dual mono), otherwise it is a real
    // spatialization finding.
    let dec = if ch == 1 && dec.len() == 2 {
        let n = dec[0].len().min(dec[1].len());
        let same = corr(&dec[0][..n], &dec[1][..n]);
        let energy = corr(&dec[0][..n], &dec[0][..n]);
        if energy > 0.0 && (same / energy - 1.0).abs() < 1e-3 {
            note = "ffmpeg dual-mono upmix (implicit PS probe)".into();
            vec![dec[0].clone()]
        } else {
            println!("{head} {actual:.1} | {enc_ms:.1} | {} | {}/{} | — | — | — | — | — | — | mono src spatialized to stereo by decode chain |",
                probe.profile, probe.rate, probe.channels);
            return;
        }
    } else {
        dec
    };
    let mut shape_note = String::new();
    if probe.rate != RATE || dec.len() != ch {
        shape_note = format!("shape {}/{} != {RATE}/{ch}", probe.rate, dec.len());
        note = format!("{note} {shape_note}").trim().to_string();
    }
    // syom streams: independent-lineage acceptance via FDK and FAAD2.
    if matches!(e, Engine::SyomLc | Engine::SyomHe | Engine::SyomHe2) {
        for (name, bin) in [
            ("fdk", repo.join("lab/fdk/fdk_driver")),
            ("faad", repo.join("lab/faad2/faad_driver")),
        ] {
            if !bin.is_file() {
                continue;
            }
            match Command::new(&bin).args(["decode-adts"]).arg(&path).output() {
                Ok(o) => {
                    let s = String::from_utf8_lossy(&o.stdout).into_owned();
                    let samples = json_get(&s, "samples")
                        .map(|v| format!("{v:.0}"))
                        .unwrap_or_else(|| "0".into());
                    let partial = s.contains("partial_error") || s.contains("\"error\"");
                    note = if partial {
                        format!("{note} {name}-dec PARTIAL samples={samples}")
                            .trim()
                            .to_string()
                    } else if o.status.success() {
                        format!("{note} {name}-dec ok samples={samples}")
                            .trim()
                            .to_string()
                    } else {
                        format!("{note} {name}-dec FAIL").trim().to_string()
                    };
                }
                Err(_) => {}
            }
        }
    }
    if rms(&clip.pcm[0]) < 1e-6 || rms(&dec[0]) < 1e-6 {
        println!(
            "{head} {actual:.1} | {enc_ms:.1} | {} | {}/{} | — | — | — | — | — | — | silent |",
            probe.profile,
            probe.rate,
            dec.len()
        );
        return;
    }
    // Alignment: declared engine delay refined by local xcorr; FAAC (no
    // declared delay) gets a free envelope search.
    let lag = match e.declared_delay(drv_delay) {
        Some(d) => {
            let r = refine_lag(&clip.pcm[0], &dec[0], d, 128);
            if (r - d).abs() > 96 {
                note = format!("{note} align drift declared={d} used={r}")
                    .trim()
                    .to_string();
            }
            r
        }
        None => {
            let coarse = envelope_lag(&clip.pcm[0], &dec[0]);
            let r = refine_lag(&clip.pcm[0], &dec[0], coarse, 240);
            note = format!("{note} free-align lag={r}").trim().to_string();
            r
        }
    };
    if dec.len() != ch {
        println!("{head} {actual:.1} | {enc_ms:.1} | {} | {}/{} | — | — | — | — | — | — | channel mismatch {ch} vs {} {} |",
            probe.profile, probe.rate, probe.channels, dec.len(), note.replace('|', "/"));
        return;
    }
    let n_src = clip.pcm[0].len();
    let (mut ps, mut pe) = (0.0f64, 0.0f64);
    let mut lf = 0.0;
    let mut hf = 0.0;
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
    let valid = n_src.min(dec[0].len().saturating_sub(lag.max(0) as usize));
    let (mut ild_err, mut icc_err) = (f64::NAN, f64::NAN);
    if ch == 2 {
        let (wl, dl) = aligned(&clip.pcm[0], &dec[0], lag);
        let (wr, dr) = aligned(&clip.pcm[1], &dec[1], lag);
        let n = wl.len().min(wr.len());
        let (si, sc) = spatial(&wl[..n], &wr[..n]);
        let (di, dc) = spatial(&dl[..n], &dr[..n]);
        ild_err = di - si;
        icc_err = dc - sc;
        // Swap probe at the final lag (the score crate's swap diagnostic
        // false-positives on decorrelated content; require correlation).
        let same = corr_n(&wl[..n], &dl[..n]);
        let cross = corr_n(&wl[..n], &dr[..n]);
        if cross > 0.5 && cross > 1.25 * same {
            note = format!("{note} REAL channel swap?").trim().to_string();
        }
    }
    let f = |v: f64| {
        if v.is_nan() {
            "—".to_string()
        } else {
            format!("{v:.1}")
        }
    };
    println!(
        "{head} {actual:.1} | {enc_ms:.1} | {} | {}/{} | {} | {} | {} | {} | {} | {} | {} | {} |",
        probe.profile,
        probe.rate,
        probe.channels,
        lag,
        valid,
        f(snr),
        f(lf),
        f(hf),
        f(ild_err),
        f(icc_err),
        note.replace('|', "/")
    );
}

fn rms(x: &[f32]) -> f64 {
    if x.is_empty() {
        return 0.0;
    }
    (x.iter().map(|&v| f64::from(v) * f64::from(v)).sum::<f64>() / x.len() as f64).sqrt()
}

fn corr(a: &[f32], b: &[f32]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(&x, &y)| f64::from(x) * f64::from(y))
        .sum()
}

/// Normalized correlation at offset 0.
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

/// Waveform xcorr refine in `[center - radius, center + radius]`.
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

/// Coarse delay from RMS-envelope correlation (hop 120, win 480), immune
/// to tonal period aliases.
fn envelope_lag(r: &[f32], d: &[f32]) -> i32 {
    const HOP: usize = 120;
    const WIN: usize = 480;
    let env = |x: &[f32]| -> Vec<f64> {
        (0..x.len().saturating_sub(WIN) / HOP)
            .map(|i| {
                let s = x[i * HOP..i * HOP + WIN]
                    .iter()
                    .map(|&v| f64::from(v) * f64::from(v))
                    .sum::<f64>();
                (s / WIN as f64).sqrt()
            })
            .collect()
    };
    let er = env(r);
    let ed = env(d);
    let max_frames = 100; // 12 000 samples
    let (mut best_lag, mut best) = (0usize, f64::NEG_INFINITY);
    for lag in 0..max_frames.min(ed.len().saturating_sub(1)) {
        let n = er.len().min(ed.len() - lag);
        if n < er.len() / 2 {
            continue;
        }
        let (mut s, mut ea, mut eb) = (0.0, 0.0, 0.0);
        for i in 0..n {
            s += er[i] * ed[i + lag];
            ea += er[i] * er[i];
            eb += ed[i + lag] * ed[i + lag];
        }
        if ea * eb == 0.0 {
            continue;
        }
        let c = s / (ea * eb).sqrt();
        if c > best + 1e-9 {
            best = c;
            best_lag = lag;
        }
    }
    (best_lag * HOP) as i32
}

/// Encode timing + memory lane: 10 s stereo noise, one thread. syom is
/// timed in-process (20 reps) and as a process via `self` like the peers
/// (5 reps, wall includes launch + PCM read; memory is child peak RSS).
fn timing(repo: &Path, work: &Path) {
    println!();
    println!("## Timing / memory lane (10 s stereo noise 48 kHz)");
    println!();
    println!("| engine | mode | req k | lane | reps | median ms | p95 ms | peak RSS KiB |");
    println!("|---|---|---:|---|---:|---:|---:|---:|");
    let pcm: Vec<Vec<f32>> = (0..2).map(|c| noise(0.3, 5 + c as u32).repeat(5)).collect();
    let planar = work.join("timing.f32");
    write_planar(&planar, &pcm);
    let peer_run = repo.join("lab/quality/peer_run.py");
    for (e, bps) in [
        (Engine::SyomLc, 48_000u32),
        (Engine::SyomHe, 48_000),
        (Engine::SyomHe2, 32_000),
        (Engine::FdkLc, 48_000),
        (Engine::FdkHe, 48_000),
        (Engine::FdkHe2, 32_000),
        (Engine::OxLc, 48_000),
        (Engine::OxHe, 48_000),
        (Engine::FaacLc, 48_000),
        (Engine::AvcLc, 48_000),
    ] {
        let (mut times, mut rss) = (Vec::new(), Vec::new());
        if matches!(e, Engine::SyomLc | Engine::SyomHe | Engine::SyomHe2) {
            let opts = syom_opts(e, bps);
            let _ = encode_with(&pcm, RATE, &opts);
            for _ in 0..20 {
                let t = Instant::now();
                let _ = encode_with(&pcm, RATE, &opts);
                times.push(t.elapsed().as_secs_f64() * 1e3);
            }
            print_timing(e.tag(), bps, "in-process", &times, None);
        }
        // process lane for every engine (syom via the self subcommand)
        let out = work.join(format!("timing-{}.adts", e.tag()));
        let cmd: Vec<String> = match e {
            Engine::SyomLc | Engine::SyomHe | Engine::SyomHe2 => vec![
                std::env::current_exe().unwrap().to_string_lossy().into(),
                "self".into(),
                match e {
                    Engine::SyomHe => "he1",
                    Engine::SyomHe2 => "he2",
                    _ => "lc",
                }
                .into(),
                "2".into(),
                bps.to_string(),
                planar.to_string_lossy().into(),
                out.to_string_lossy().into(),
            ],
            Engine::FdkLc | Engine::FdkHe | Engine::FdkHe2 => vec![
                repo.join("lab/fdk/fdk_driver").to_string_lossy().into(),
                "encode-pcm".into(),
                RATE.to_string(),
                "2".into(),
                bps.to_string(),
                match e {
                    Engine::FdkHe => "5",
                    Engine::FdkHe2 => "29",
                    _ => "2",
                }
                .into(),
                planar.to_string_lossy().into(),
                out.to_string_lossy().into(),
            ],
            Engine::OxLc | Engine::OxHe => vec![
                repo.join("lab/oxideav/target/release/oxideav_driver")
                    .to_string_lossy()
                    .into(),
                "encode-pcm".into(),
                RATE.to_string(),
                "2".into(),
                bps.to_string(),
                if e == Engine::OxHe {
                    "he".into()
                } else {
                    "lc".into()
                },
                planar.to_string_lossy().into(),
                out.to_string_lossy().into(),
            ],
            Engine::FaacLc => vec![
                repo.join("lab/faac/faac_driver").to_string_lossy().into(),
                "encode-pcm".into(),
                RATE.to_string(),
                "2".into(),
                (bps / 2).to_string(),
                planar.to_string_lossy().into(),
                out.to_string_lossy().into(),
            ],
            Engine::AvcLc => vec![
                repo.join("lab/libavcodec/avc_driver")
                    .to_string_lossy()
                    .into(),
                "encode-lc-adts".into(),
                RATE.to_string(),
                "2".into(),
                bps.to_string(),
                planar.to_string_lossy().into(),
                out.to_string_lossy().into(),
            ],
        };
        times.clear();
        for _ in 0..5 {
            let o = Command::new("python3").arg(&peer_run).args(&cmd).output();
            match o {
                Ok(o) if o.status.success() => {
                    let s = String::from_utf8_lossy(&o.stdout);
                    if let Some(v) = json_get(&s, "wall_ms") {
                        times.push(v);
                    }
                    if let Some(v) = json_get(&s, "maxrss_kb") {
                        rss.push(v);
                    }
                }
                _ => break,
            }
        }
        if times.is_empty() {
            println!("| {} | — | {bps} | process | 0 | — | — | — |", e.tag());
        } else {
            print_timing(e.tag(), bps, "process", &times, rss.first().copied());
        }
    }
}

/// Minimal JSON number extraction for peer_run.py's flat output.
fn json_get(s: &str, key: &str) -> Option<f64> {
    let pat = format!("\"{key}\":");
    let i = s.find(&pat)? + pat.len();
    let rest = &s[i..];
    let end = rest.find([',', '}']).unwrap_or(rest.len());
    rest[..end].trim().parse().ok()
}

fn print_timing(tag: &str, bps: u32, lane: &str, times: &[f64], rss: Option<f64>) {
    let mut t = times.to_vec();
    t.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let med = t[t.len() / 2];
    let p95 = t[(t.len() as f64 * 0.95).ceil() as usize - 1];
    let rss = rss.map(|v| format!("{v:.0}")).unwrap_or_else(|| "—".into());
    println!(
        "| {tag} | — | {} | {lane} | {} | {med:.1} | {p95:.1} | {rss} |",
        bps / 1000,
        t.len()
    );
}

fn clips(repo: &Path) -> Vec<Clip> {
    let mut v = vec![
        Clip {
            name: "voice-like",
            class: "speech-like stereo",
            pcm: vec![voice(3, 150.0), voice(4, 113.0)],
        },
        Clip {
            name: "noise-st",
            class: "noise stereo",
            pcm: vec![noise(0.35, 11), noise(0.35, 12)],
        },
        Clip {
            name: "mix-st",
            class: "tone+noise stereo",
            pcm: vec![mix(1), mix(2)],
        },
        Clip {
            name: "tremolo",
            class: "stereo tonal",
            pcm: tremolo(),
        },
        Clip {
            name: "click-st",
            class: "transient stereo",
            pcm: vec![clicks(21), clicks(22)],
        },
        Clip {
            name: "ambience",
            class: "spatial stereo",
            pcm: ambience(),
        },
    ];
    if let Some(pcm) = lecture(repo) {
        let mono: Vec<f32> = (0..pcm[0].len().min(pcm.get(1).map_or(0, Vec::len)))
            .map(|i| 0.5 * (pcm[0][i] + pcm[1][i]))
            .collect();
        v.push(Clip {
            name: "lecture",
            class: "speech stereo (0.25 s)",
            pcm,
        });
        v.push(Clip {
            name: "lecture-m",
            class: "speech mono (0.25 s)",
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

/// Panned tones + independent HF ambience: the PS spatial probe.
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

/// TASK-134: `iid` subcommand. ambience / mix-st / tremolo, iid_mode 1 vs 4.
fn iid_grid() -> ExitCode {
    let work = std::env::temp_dir().join("syom-ps-iid");
    let _ = fs::remove_dir_all(&work);
    fs::create_dir_all(&work).expect("work dir");
    let ffmpeg = Command::new("ffmpeg")
        .arg("-version")
        .output()
        .is_ok_and(|o| o.status.success());
    println!("# TASK-134 iid_mode 1 vs 4 (48 kHz, {SECS} s, same clips and ILD/ICC as the matrix)");
    println!(
        "decoder: {}",
        if ffmpeg {
            "ffmpeg"
        } else {
            "syom (ffmpeg missing)"
        }
    );
    println!("ps kbps = ps_data() bits / source seconds; ext kbps = extended-data block bits / source seconds.");
    println!();
    println!(
        "| clip | req k | mode | actual k | ILD err | abs ILD | ICC err | ps kbps | ext kbps |"
    );
    println!("|---|---:|---:|---:|---:|---:|---:|---:|---:|");
    let clips = [
        ("ambience", ambience()),
        ("mix-st", vec![mix(1), mix(2)]),
        ("tremolo", tremolo()),
    ];
    struct Cell {
        clip: &'static str,
        req: u32,
        mode: u8,
        ild: f64,
        icc: f64,
        ps: f64,
        ext: f64,
    }
    let mut cells = Vec::new();
    for (name, pcm) in &clips {
        for bps in [24_000u32, 32_000, 48_000] {
            for fine in [false, true] {
                let (adts, ps_bits, ext_bits) =
                    encode_he_v2_adts(pcm, RATE, bps, fine).expect("encode");
                let secs = pcm[0].len() as f64 / f64::from(RATE);
                let actual = adts.len() as f64 * 8.0 / secs / 1000.0;
                let ps_k = ps_bits as f64 / secs / 1000.0;
                let ext_k = ext_bits as f64 / secs / 1000.0;
                let tag = format!("{name}-{bps}-{}", if fine { 4 } else { 1 });
                let dec = match decode_iid(&work, &tag, &adts, ffmpeg) {
                    Ok(d) => d,
                    Err(e) => {
                        println!("| {name} | {} | {} | {actual:.2} | — | — | — | {ps_k:.3} | {ext_k:.3} | {e} |",
                            bps / 1000, if fine { 4 } else { 1 });
                        continue;
                    }
                };
                let (ild, icc) = match ild_icc(pcm, &dec) {
                    Ok(v) => v,
                    Err(e) => {
                        println!("| {name} | {} | {} | {actual:.2} | — | — | — | {ps_k:.3} | {ext_k:.3} | {e} |",
                            bps / 1000, if fine { 4 } else { 1 });
                        continue;
                    }
                };
                let mode = if fine { 4 } else { 1 };
                println!(
                    "| {name} | {} | {mode} | {actual:.2} | {ild:.2} | {:.2} | {icc:.3} | {ps_k:.3} | {ext_k:.3} |",
                    bps / 1000,
                    ild.abs()
                );
                cells.push(Cell {
                    clip: name,
                    req: bps,
                    mode,
                    ild,
                    icc,
                    ps: ps_k,
                    ext: ext_k,
                });
            }
        }
    }
    println!();
    for clip in ["ambience", "mix-st", "tremolo"] {
        let mean = |mode: u8| {
            let v: Vec<_> = cells
                .iter()
                .filter(|c| c.clip == clip && c.mode == mode)
                .collect();
            if v.is_empty() {
                return f64::NAN;
            }
            v.iter().map(|c| c.ild.abs()).sum::<f64>() / v.len() as f64
        };
        let (m1, m4) = (mean(1), mean(4));
        println!(
            "{clip}: mean |ILD| mode1 {m1:.3} mode4 {m4:.3} half {} (m4 <= m1/2)",
            m4 <= m1 * 0.5 + 1e-9
        );
        for bps in [24_000u32, 32_000, 48_000] {
            let get = |mode: u8| {
                cells
                    .iter()
                    .find(|c| c.clip == clip && c.req == bps && c.mode == mode)
            };
            if let (Some(a), Some(b)) = (get(1), get(4)) {
                println!(
                    "  {}: |ILD| {:.3}->{:.3} ICC {:.3}->{:.3} d ps {:+.3} kbps d ext {:+.3} kbps",
                    bps / 1000,
                    a.ild.abs(),
                    b.ild.abs(),
                    a.icc,
                    b.icc,
                    b.ps - a.ps,
                    b.ext - a.ext
                );
            }
        }
    }
    ExitCode::SUCCESS
}

fn decode_iid(work: &Path, tag: &str, adts: &[u8], ffmpeg: bool) -> Result<Vec<Vec<f32>>, String> {
    if !ffmpeg {
        return decode_with(adts, &DecodeOptions::unbounded())
            .map(|d| d.channels)
            .map_err(|e| e.to_string());
    }
    let path = work.join(format!("{tag}.adts"));
    fs::write(&path, adts).map_err(|e| e.to_string())?;
    let probe = ffprobe(&path);
    let ch = if probe.channels == 0 {
        2
    } else {
        probe.channels
    };
    decode_ffmpeg(&path, ch)
}

fn ild_icc(src: &[Vec<f32>], dec: &[Vec<f32>]) -> Result<(f64, f64), String> {
    if src.len() < 2 || dec.len() < 2 {
        return Err(format!("channels src {} dec {}", src.len(), dec.len()));
    }
    let lag = refine_lag(&src[0], &dec[0], 3018, 128);
    let (wl, dl) = aligned(&src[0], &dec[0], lag);
    let (wr, dr) = aligned(&src[1], &dec[1], lag);
    let n = wl.len().min(wr.len());
    if n < 1024 {
        return Err(format!("aligned {n} samples"));
    }
    let (si, sc) = spatial(&wl[..n], &wr[..n]);
    let (di, dc) = spatial(&dl[..n], &dr[..n]);
    Ok((di - si, dc - sc))
}

fn lecture(repo: &Path) -> Option<Vec<Vec<f32>>> {
    let bytes = fs::read(repo.join("src/goldens/lecture.m4a")).ok()?;
    let dec = decode_with(&bytes, &DecodeOptions::unbounded()).ok()?;
    Some(dec.channels)
}
