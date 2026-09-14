//! Isolated same-bitrate LC encode baseline. Not linked by cargo test.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use syom::{encode_with, EncodeOptions};
use syom_lab_score::{score_encode_pair, Outcome, Pair};

const RATE: u32 = 48_000;
const N: usize = 48_000; // 1 s

struct Clip {
    name: &'static str,
    class: &'static str,
    pcm: Vec<Vec<f32>>,
}

fn main() -> ExitCode {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo = root.parent().unwrap().parent().unwrap();
    let work = std::env::temp_dir().join("syom-lab-quality");
    let _ = fs::create_dir_all(&work);
    let clips = clips();
    let mut rows = Vec::new();
    for clip in &clips {
        for bps in [64_000u32, 128_000] {
            rows.push(run_syom(&work, clip, bps));
            if let Some(r) = run_avc(repo, &work, clip, bps) {
                rows.push(r);
            }
            if let Some(r) = run_ffmpeg_cli(&work, clip, bps) {
                rows.push(r);
            }
            if glint_has_encode_pcm(repo) {
                if let Some(r) = run_glint(repo, &work, clip, bps) {
                    rows.push(r);
                }
            }
        }
    }
    print_report(&rows);
    ExitCode::SUCCESS
}

fn clips() -> Vec<Clip> {
    vec![
        Clip {
            name: "sine440",
            class: "tonal",
            pcm: vec![sine(440.0, 0.5)],
        },
        Clip {
            name: "noise",
            class: "noise",
            pcm: vec![noise(0.4)],
        },
        Clip {
            name: "click",
            class: "transient",
            pcm: vec![clicks()],
        },
        Clip {
            name: "tremolo",
            class: "stereo",
            pcm: {
                let l = sine(440.0, 0.4);
                let r: Vec<f32> = l.iter().map(|x| x * 0.8).collect();
                vec![l, r]
            },
        },
    ]
}

fn sine(hz: f64, amp: f32) -> Vec<f32> {
    (0..N)
        .map(|i| amp * (2.0 * std::f64::consts::PI * hz * i as f64 / f64::from(RATE)).sin() as f32)
        .collect()
}

fn noise(amp: f32) -> Vec<f32> {
    let mut s = 0x0BAD_F00Du32;
    (0..N)
        .map(|_| {
            s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            amp * (((s >> 9) as f32 / (1u32 << 23) as f32) * 2.0 - 1.0)
        })
        .collect()
}

fn clicks() -> Vec<f32> {
    let mut v = vec![0.0f32; N];
    let mut i = 0;
    while i < N {
        v[i] = 0.9;
        i += 2048;
    }
    v
}

struct Row {
    clip: String,
    class: String,
    engine: String,
    requested_bps: u32,
    adts_bytes: usize,
    actual_bps: Option<f64>,
    delay: i32,
    valid: usize,
    decoded_len: usize,
    snr_db: Option<f64>,
    max_abs: Option<f32>,
    note: String,
}

fn run_syom(work: &Path, clip: &Clip, bps: u32) -> Row {
    let opts = EncodeOptions::adts().with_bitrate_bps(bps);
    match encode_with(&clip.pcm, RATE, &opts) {
        Ok(adts) => {
            let path = work.join(format!("{}-{}-syom.adts", clip.name, bps));
            let _ = fs::write(&path, &adts);
            score_row(
                clip,
                "syom",
                bps,
                &adts,
                decode_ffmpeg_cli(&path, clip.pcm.len()),
            )
        }
        Err(e) => fail_row(clip, "syom", bps, e.to_string()),
    }
}

fn run_avc(repo: &Path, work: &Path, clip: &Clip, bps: u32) -> Option<Row> {
    let drv = repo.join("lab/libavcodec/avc_driver");
    if !drv.is_file() {
        return None;
    }
    let planar = work.join(format!("{}-{}-avc.f32", clip.name, bps));
    let adts = work.join(format!("{}-{}-avc.adts", clip.name, bps));
    write_planar(&planar, &clip.pcm);
    let out = Command::new(&drv)
        .args([
            "encode-lc-adts",
            &RATE.to_string(),
            &clip.pcm.len().to_string(),
            &bps.to_string(),
            planar.to_str()?,
            adts.to_str()?,
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return Some(fail_row(
            clip,
            "lavc9",
            bps,
            String::from_utf8_lossy(&out.stderr).into(),
        ));
    }
    let bytes = fs::read(&adts).ok()?;
    Some(score_row(
        clip,
        "lavc9",
        bps,
        &bytes,
        decode_ffmpeg_cli(&adts, clip.pcm.len()),
    ))
}

fn run_ffmpeg_cli(work: &Path, clip: &Clip, bps: u32) -> Option<Row> {
    let wav = work.join(format!("{}-{}-ff.f32", clip.name, bps));
    let adts = work.join(format!("{}-{}-ff.adts", clip.name, bps));
    write_interleaved(&wav, &clip.pcm);
    let kbps = bps / 1000;
    let st = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-f",
            "f32le",
            "-ar",
            &RATE.to_string(),
            "-ac",
            &clip.pcm.len().to_string(),
            "-i",
            wav.to_str()?,
            "-c:a",
            "aac",
            "-b:a",
            &format!("{kbps}k"),
            "-f",
            "adts",
            adts.to_str()?,
        ])
        .output()
        .ok()?;
    if !st.status.success() {
        return Some(fail_row(
            clip,
            "ffmpeg-cli",
            bps,
            "ffmpeg encode failed".into(),
        ));
    }
    let bytes = fs::read(&adts).ok()?;
    Some(score_row(
        clip,
        "ffmpeg-cli",
        bps,
        &bytes,
        decode_ffmpeg_cli(&adts, clip.pcm.len()),
    ))
}

fn glint_has_encode_pcm(repo: &Path) -> bool {
    let bin = repo.join("lab/glint/target/release/glint_smoke");
    if !bin.is_file() {
        return false;
    }
    let out = Command::new(&bin).output().ok();
    out.map(|o| {
        let t = String::from_utf8_lossy(&o.stderr);
        t.contains("encode-pcm")
    })
    .unwrap_or(false)
}

fn run_glint(repo: &Path, work: &Path, clip: &Clip, bps: u32) -> Option<Row> {
    let bin = repo.join("lab/glint/target/release/glint_smoke");
    if !bin.is_file() {
        return None;
    }
    let s16 = work.join(format!("{}-{}-glint.s16", clip.name, bps));
    let adts = work.join(format!("{}-{}-glint.adts", clip.name, bps));
    write_s16le(&s16, &clip.pcm);
    let kbps = bps / 1000;
    let st = Command::new(&bin)
        .args([
            "encode-pcm",
            &RATE.to_string(),
            &clip.pcm.len().to_string(),
            &kbps.to_string(),
            "1",
            s16.to_str()?,
            adts.to_str()?,
        ])
        .output()
        .ok()?;
    if !st.status.success() {
        return Some(fail_row(
            clip,
            "glint",
            bps,
            String::from_utf8_lossy(&st.stderr).into(),
        ));
    }
    let bytes = fs::read(&adts).ok()?;
    Some(score_row(
        clip,
        "glint",
        bps,
        &bytes,
        decode_ffmpeg_cli(&adts, clip.pcm.len()),
    ))
}

fn decode_ffmpeg_cli(adts: &Path, ch: usize) -> Result<Vec<Vec<f32>>, String> {
    let out = adts.with_extension("dec.f32");
    let st = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-i",
            adts.to_str().unwrap(),
            "-ac",
            &ch.to_string(),
            "-f",
            "f32le",
            "-acodec",
            "pcm_f32le",
            out.to_str().unwrap(),
        ])
        .output()
        .map_err(|e| e.to_string())?;
    if !st.status.success() {
        return Err("ffmpeg decode failed".into());
    }
    let raw = fs::read(&out).map_err(|e| e.to_string())?;
    Ok(deinterleave_f32(&raw, ch))
}

fn score_row(
    clip: &Clip,
    engine: &str,
    bps: u32,
    adts: &[u8],
    decoded: Result<Vec<Vec<f32>>, String>,
) -> Row {
    let decoded = match decoded {
        Ok(d) => d,
        Err(e) => return fail_row(clip, engine, bps, e),
    };
    let ch = clip.pcm.len();
    let planes = if decoded.len() == ch {
        decoded
    } else if decoded.len() == 1 && ch == 1 {
        decoded
    } else {
        // ffmpeg may emit interleaved as one stream; reshape
        decoded
    };
    let n_ch = clip.pcm.len();
    let deg = if planes.len() == n_ch {
        planes
    } else {
        return fail_row(
            clip,
            engine,
            bps,
            format!("ch {} vs {}", planes.len(), n_ch),
        );
    };
    let outcome = score_encode_pair(Pair {
        rate: RATE,
        reference: &clip.pcm,
        degraded: &deg,
        coded_bytes: Some(adts.len() as u64),
    });
    match outcome {
        Outcome::Scored(s) => {
            let mut note = String::new();
            let mut snr = Some(s.snr_db);
            if clip.class == "tonal" && (s.delay_samples - 1024).abs() > 64 {
                note = format!(
                    "sine xcorr alias delay={} (not priming); SNR not ranked",
                    s.delay_samples
                );
                snr = None;
            }
            Row {
                clip: clip.name.into(),
                class: clip.class.into(),
                engine: engine.into(),
                requested_bps: bps,
                adts_bytes: adts.len(),
                actual_bps: s.actual_bps,
                delay: s.delay_samples,
                valid: s.valid_samples,
                decoded_len: deg[0].len(),
                snr_db: snr,
                max_abs: Some(s.max_abs),
                note,
            }
        }
        Outcome::Diagnostic { kind, detail } => Row {
            clip: clip.name.into(),
            class: clip.class.into(),
            engine: engine.into(),
            requested_bps: bps,
            adts_bytes: adts.len(),
            actual_bps: Some(8.0 * adts.len() as f64 / (N as f64 / RATE as f64)),
            delay: 0,
            valid: 0,
            decoded_len: deg[0].len(),
            snr_db: None,
            max_abs: None,
            note: format!("{kind:?} {detail}"),
        },
        Outcome::Unscorable { reason } => fail_row(clip, engine, bps, reason),
    }
}

fn fail_row(clip: &Clip, engine: &str, bps: u32, note: String) -> Row {
    Row {
        clip: clip.name.into(),
        class: clip.class.into(),
        engine: engine.into(),
        requested_bps: bps,
        adts_bytes: 0,
        actual_bps: None,
        delay: 0,
        valid: 0,
        decoded_len: 0,
        snr_db: None,
        max_abs: None,
        note,
    }
}

fn write_planar(path: &Path, pcm: &[Vec<f32>]) {
    let mut buf = Vec::new();
    for ch in pcm {
        for &x in ch {
            buf.extend_from_slice(&x.to_le_bytes());
        }
    }
    fs::write(path, buf).unwrap();
}

fn write_interleaved(path: &Path, pcm: &[Vec<f32>]) {
    let n = pcm[0].len();
    let mut buf = Vec::with_capacity(n * pcm.len() * 4);
    for i in 0..n {
        for ch in pcm {
            buf.extend_from_slice(&ch[i].to_le_bytes());
        }
    }
    fs::write(path, buf).unwrap();
}

fn write_s16le(path: &Path, pcm: &[Vec<f32>]) {
    let n = pcm[0].len();
    let mut buf = Vec::with_capacity(n * pcm.len() * 2);
    for i in 0..n {
        for ch in pcm {
            let v = (ch[i].clamp(-1.0, 1.0) * 32767.0).round() as i16;
            buf.extend_from_slice(&v.to_le_bytes());
        }
    }
    fs::write(path, buf).unwrap();
}

fn deinterleave_f32(raw: &[u8], ch_hint: usize) -> Vec<Vec<f32>> {
    let nsamp = raw.len() / 4;
    let ch = if ch_hint == 0 {
        // caller will reshape; treat as mono samples then fix in score_row
        1
    } else {
        ch_hint
    };
    let frames = nsamp / ch.max(1);
    let mut planes = vec![Vec::with_capacity(frames); ch.max(1)];
    for i in 0..frames {
        for c in 0..ch.max(1) {
            let off = (i * ch.max(1) + c) * 4;
            let v = f32::from_le_bytes(raw[off..off + 4].try_into().unwrap());
            planes[c].push(v);
        }
    }
    planes
}

fn print_report(rows: &[Row]) {
    println!("# TASK-16 encoder quality matrix (synth dev, 48 kHz, 1 s)");
    println!();
    println!("Independent decode: ffmpeg pcm_f32le. SNR after alignment (not PEAQ).");
    println!();
    println!(
        "| clip | class | engine | req bps | bytes | actual bps | delay | valid | dec_len | SNR dB | max_abs | note |"
    );
    println!("|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---|");
    for r in rows {
        let snr = r
            .snr_db
            .map(|v| format!("{v:.1}"))
            .unwrap_or_else(|| "—".into());
        let bps = r
            .actual_bps
            .map(|v| format!("{v:.0}"))
            .unwrap_or_else(|| "—".into());
        let ma = r
            .max_abs
            .map(|v| format!("{v:.3e}"))
            .unwrap_or_else(|| "—".into());
        println!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            r.clip,
            r.class,
            r.engine,
            r.requested_bps,
            r.adts_bytes,
            bps,
            r.delay,
            r.valid,
            r.decoded_len,
            snr,
            ma,
            r.note.replace('|', "/")
        );
    }
}
