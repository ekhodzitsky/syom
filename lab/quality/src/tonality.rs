//! Isolated TASK-69 tonality A/B. Not invoked by cargo test.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;
use syom::{decode_with, encode_with, DecodeOptions, EncodeOptions};
use syom_lab_score::{score_encode_pair, Outcome, Pair};

const RATE: u32 = 48_000;
const N: usize = 48_000;

struct Clip {
    name: &'static str,
    class: &'static str,
    pcm: Vec<Vec<f32>>,
}

fn main() -> ExitCode {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo = root.parent().unwrap().parent().unwrap();
    println!("# TASK-69 Johnston SFM tonality A/B (synth + lecture, 48 kHz)");
    println!();
    println!("Candidate: scale target_q by SFM (noise 0.25×). Coded mask unchanged.");
    println!("ATH stays off. Decoder: syom unbounded. SNR is **not** PEAQ.");
    println!();
    println!(
        "| clip | class | ton | req bps | bytes | actual bps | delay | valid | xcorr SNR | priming SNR | max_abs | note |"
    );
    println!("|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---|");
    for clip in &clips(repo) {
        for bps in [64_000u32, 128_000] {
            for ton in [false, true] {
                print_row(clip, bps, ton);
            }
        }
    }
    cpu_note();
    ExitCode::SUCCESS
}

fn clips(repo: &Path) -> Vec<Clip> {
    let nse = noise(0.2);
    let mix: Vec<f32> = sine(440.0, 0.4)
        .iter()
        .zip(nse.iter())
        .map(|(s, n)| s + n)
        .collect();
    let mut v = vec![
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
            name: "tremolo",
            class: "stereo",
            pcm: {
                let l = sine(440.0, 0.4);
                let r: Vec<f32> = l.iter().map(|x| x * 0.8).collect();
                vec![l, r]
            },
        },
        Clip {
            name: "silence",
            class: "silence",
            pcm: vec![vec![0.0f32; N]],
        },
        Clip {
            name: "click",
            class: "transient",
            pcm: vec![clicks()],
        },
        Clip {
            name: "mix",
            class: "mixed",
            pcm: vec![mix],
        },
    ];
    if let Some(pcm) = lecture(repo) {
        v.push(Clip {
            name: "lecture",
            class: "speech",
            pcm,
        });
    }
    v
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

fn lecture(repo: &Path) -> Option<Vec<Vec<f32>>> {
    let p = repo.join("src/goldens/lecture.m4a");
    let bytes = fs::read(p).ok()?;
    let dec = decode_with(&bytes, &DecodeOptions::unbounded()).ok()?;
    let n = dec.channels[0].len().min(N);
    Some(dec.channels.iter().map(|c| c[..n].to_vec()).collect())
}

fn print_row(clip: &Clip, bps: u32, ton: bool) {
    let opts = EncodeOptions::adts()
        .with_bitrate_bps(bps)
        .with_tonality(ton);
    let tag = if ton { "on" } else { "off" };
    match encode_with(&clip.pcm, RATE, &opts) {
        Ok(adts) => match decode_with(&adts, &DecodeOptions::unbounded()) {
            Ok(dec) => {
                let n = clip.pcm.len().min(dec.channels.len());
                let deg: Vec<Vec<f32>> = dec.channels[..n].to_vec();
                let refp: Vec<Vec<f32>> = clip.pcm[..n].to_vec();
                let out = score_encode_pair(Pair {
                    rate: RATE,
                    reference: &refp,
                    degraded: &deg,
                    coded_bytes: Some(adts.len() as u64),
                });
                match out {
                    Outcome::Scored(s) => {
                        let snr = format!("{:.1}", s.snr_db);
                        let abps = s
                            .actual_bps
                            .map(|v| format!("{v:.0}"))
                            .unwrap_or_else(|| "—".into());
                        let prime = priming_snr(&refp[0], &deg[0]);
                        let note = if (s.delay_samples - 1024).abs() > 64 {
                            "xcorr delay ≠ priming".into()
                        } else {
                            String::new()
                        };
                        println!(
                            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {:.1} | {:.3e} | {} |",
                            clip.name,
                            clip.class,
                            tag,
                            bps,
                            adts.len(),
                            abps,
                            s.delay_samples,
                            s.valid_samples,
                            snr,
                            prime,
                            s.max_abs,
                            note
                        );
                    }
                    Outcome::Diagnostic { kind, detail } => {
                        println!(
                            "| {} | {} | {} | {} | {} | — | — | — | — | — | {:?} {} |",
                            clip.name,
                            clip.class,
                            tag,
                            bps,
                            adts.len(),
                            kind,
                            detail.replace('|', "/")
                        );
                    }
                    Outcome::Unscorable { reason } => {
                        println!(
                            "| {} | {} | {} | {} | {} | — | — | — | — | — | {} |",
                            clip.name,
                            clip.class,
                            tag,
                            bps,
                            adts.len(),
                            reason.replace('|', "/")
                        );
                    }
                }
            }
            Err(e) => println!(
                "| {} | {} | {} | {} | {} | — | — | — | — | — | decode {} |",
                clip.name,
                clip.class,
                tag,
                bps,
                adts.len(),
                e
            ),
        },
        Err(e) => println!(
            "| {} | {} | {} | {} | 0 | — | — | — | — | — | encode {} |",
            clip.name, clip.class, tag, bps, e
        ),
    }
}

fn priming_snr(want: &[f32], got: &[f32]) -> f64 {
    let d = 1024usize;
    if want.len() <= d + d || got.len() <= d + d {
        return f64::NAN;
    }
    let w = &want[d..want.len() - d];
    let g = &got[2 * d..2 * d + w.len()];
    let n = w.len().min(g.len());
    let mut ps = 0.0f64;
    let mut pe = 0.0f64;
    for i in 0..n {
        let s = f64::from(w[i]);
        let e = s - f64::from(g[i]);
        ps += s * s;
        pe += e * e;
    }
    if pe == 0.0 {
        200.0
    } else {
        10.0 * (ps / pe).log10()
    }
}

fn cpu_note() {
    let pcm = vec![noise(0.4).repeat(10)];
    let off = EncodeOptions::adts();
    let on = EncodeOptions::adts().with_tonality(true);
    let _ = encode_with(&pcm, RATE, &off);
    let t0 = Instant::now();
    let _ = encode_with(&pcm, RATE, &off);
    let ms_off = t0.elapsed().as_secs_f64() * 1e3;
    let _ = encode_with(&pcm, RATE, &on);
    let t1 = Instant::now();
    let _ = encode_with(&pcm, RATE, &on);
    let ms_on = t1.elapsed().as_secs_f64() * 1e3;
    let pct = if ms_off > 0.0 {
        100.0 * (ms_on - ms_off) / ms_off
    } else {
        0.0
    };
    println!();
    println!(
        "CPU (10 s noise, one-shot): ton-off {ms_off:.1} ms, ton-on {ms_on:.1} ms ({pct:+.1}%). Opt-in; default is ton-off."
    );
}
