//! Isolated scoring CLI. Not linked by cargo test --workspace.

use std::env;
use std::process::ExitCode;
use syom::{decode_with, DecodeOptions};
use syom_lab_score::{score_pair, Outcome, Pair};

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let Some(cmd) = args.next() else {
        eprintln!("usage: syom_score controls | adts CODED.adts REF.adts");
        return ExitCode::from(2);
    };
    match cmd.as_str() {
        "controls" => {
            print_controls();
            ExitCode::SUCCESS
        }
        "adts" => {
            let Some(coded) = args.next() else {
                eprintln!("adts: missing coded");
                return ExitCode::from(2);
            };
            let Some(reference) = args.next() else {
                eprintln!("adts: missing reference bitstream (decoded independently)");
                return ExitCode::from(2);
            };
            match score_adts(&coded, &reference) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("FAIL {e}");
                    ExitCode::from(1)
                }
            }
        }
        other => {
            eprintln!("unknown {other}");
            ExitCode::from(2)
        }
    }
}

fn print_controls() {
    fn run(name: &str, p: Pair<'_>) {
        match score_pair(p) {
            Outcome::Scored(s) => println!(
                "control={name} outcome=scored delay={} valid={} snr_db={:.3} max_abs={:.3e} bps={:?}",
                s.delay_samples, s.valid_samples, s.snr_db, s.max_abs, s.actual_bps
            ),
            Outcome::Unscorable { reason } => {
                println!("control={name} outcome=unscorable reason={reason}")
            }
            Outcome::Diagnostic { kind, detail } => {
                println!("control={name} outcome=diagnostic kind={kind:?} detail={detail}")
            }
        }
    }
    let n = 2048usize;
    let s: Vec<f32> = (0..n)
        .map(|i| (0.5 * (2.0 * std::f64::consts::PI * 440.0 * i as f64 / 48_000.0).sin()) as f32)
        .collect();
    let z = vec![0.0f32; n];
    let r = vec![s.clone()];
    run(
        "identical",
        Pair {
            rate: 48_000,
            reference: &r,
            degraded: &r,
            coded_bytes: Some(800),
        },
    );
    let rz = vec![z.clone()];
    run(
        "silent",
        Pair {
            rate: 48_000,
            reference: &rz,
            degraded: &rz,
            coded_bytes: None,
        },
    );
    let mut delayed = vec![0.0f32; 256];
    delayed.extend_from_slice(&s);
    let rd = vec![delayed];
    run(
        "delayed",
        Pair {
            rate: 48_000,
            reference: &r,
            degraded: &rd,
            coded_bytes: None,
        },
    );
    let rt = vec![s[..128].to_vec()];
    run(
        "truncated",
        Pair {
            rate: 48_000,
            reference: &r,
            degraded: &rt,
            coded_bytes: None,
        },
    );
    let hi: Vec<f32> = (0..n)
        .map(|i| (0.5 * (2.0 * std::f64::consts::PI * 660.0 * i as f64 / 48_000.0).sin()) as f32)
        .collect();
    let stereo_r = vec![s.clone(), hi.clone()];
    let stereo_s = vec![hi, s.clone()];
    run(
        "channel_swap",
        Pair {
            rate: 48_000,
            reference: &stereo_r,
            degraded: &stereo_s,
            coded_bytes: None,
        },
    );
    let stereo_d = vec![s.clone(), vec![0.0f32; n]];
    run(
        "mismatch_ch",
        Pair {
            rate: 48_000,
            reference: &r,
            degraded: &stereo_d,
            coded_bytes: None,
        },
    );
}

fn score_adts(coded: &str, reference: &str) -> Result<(), String> {
    let c = std::fs::read(coded).map_err(|e| e.to_string())?;
    let r = std::fs::read(reference).map_err(|e| e.to_string())?;
    let dc = decode_with(&c, &DecodeOptions::unbounded()).map_err(|e| e.to_string())?;
    let dr = decode_with(&r, &DecodeOptions::unbounded()).map_err(|e| e.to_string())?;
    if dc.sample_rate != dr.sample_rate {
        println!(
            "outcome=unscorable reason=rate {} vs {}",
            dr.sample_rate, dc.sample_rate
        );
        return Ok(());
    }
    match score_pair(Pair {
        rate: dr.sample_rate,
        reference: &dr.channels,
        degraded: &dc.channels,
        coded_bytes: Some(c.len() as u64),
    }) {
        Outcome::Scored(s) => println!(
            "outcome=scored rate={} ch={} delay={} valid={} remainder={} snr_db={:.3} max_abs={:.3e} actual_bps={:?} coded_bytes={} peaq=unavailable visqol=unavailable",
            dr.sample_rate,
            dr.channels.len(),
            s.delay_samples,
            s.valid_samples,
            s.priming_remainder,
            s.snr_db,
            s.max_abs,
            s.actual_bps,
            c.len()
        ),
        Outcome::Unscorable { reason } => println!("outcome=unscorable reason={reason}"),
        Outcome::Diagnostic { kind, detail } => {
            println!("outcome=diagnostic kind={kind:?} detail={detail} peaq=unavailable visqol=unavailable")
        }
    }
    Ok(())
}
