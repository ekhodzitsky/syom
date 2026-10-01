//! Isolated profile-expansion probe (TASK-97). Not linked by cargo test.
//!
//!   syom_probe decode FILE                     -> JSON decode outcome
//!   syom_probe decode-pcm FILE OUT.s16le       -> decode, write interleaved s16
//!   syom_probe encode IN.s16le RATE CH BPS <lc|he|he2> OUT.adts

use std::env;
use std::fs;
use std::process::ExitCode;
use syom::{decode_with, encode_with, DecodeOptions, EncodeOptions};

fn read_s16le(path: &str, ch: usize) -> Result<Vec<Vec<f32>>, String> {
    let raw = fs::read(path).map_err(|e| format!("read {path}: {e}"))?;
    let n = raw.len() / 2;
    let mut planes = vec![Vec::with_capacity(n / ch); ch];
    for i in 0..n {
        let v = i16::from_le_bytes([raw[2 * i], raw[2 * i + 1]]);
        planes[i % ch].push(v as f32 / 32768.0);
    }
    Ok(planes)
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("decode") | Some("decode-pcm") if args.len() >= 2 => {
            let data = match fs::read(&args[1]) {
                Ok(d) => d,
                Err(e) => {
                    println!("{{\"ok\":false,\"engine\":\"syom\",\"error\":\"read {e}\"}}");
                    return ExitCode::from(2);
                }
            };
            match decode_with(&data, &DecodeOptions::unbounded()) {
                Ok(pcm) => {
                    let samples = pcm.channels.first().map_or(0, Vec::len);
                    println!(
                        "{{\"ok\":true,\"engine\":\"syom\",\"rate\":{},\"channels\":{},\"samples\":{},\"core_rate\":{}}}",
                        pcm.sample_rate,
                        pcm.channels.len(),
                        samples,
                        pcm.core_rate
                    );
                    if args[0] == "decode-pcm" {
                        let mut out = Vec::with_capacity(samples * pcm.channels.len() * 2);
                        for i in 0..samples {
                            for plane in &pcm.channels {
                                let v = (plane[i].clamp(-1.0, 1.0) * 32767.0).round() as i16;
                                out.extend_from_slice(&v.to_le_bytes());
                            }
                        }
                        if let Err(e) = fs::write(&args[2], out) {
                            eprintln!("write {e}");
                            return ExitCode::from(2);
                        }
                    }
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    println!("{{\"ok\":false,\"engine\":\"syom\",\"error\":\"{e:?}\"}}");
                    ExitCode::from(1)
                }
            }
        }
        Some("encode") if args.len() == 7 => {
            let ch: usize = args[3].parse().unwrap_or(0);
            let rate: u32 = args[2].parse().unwrap_or(0);
            let bps: u32 = args[4].parse().unwrap_or(0);
            let planes = match read_s16le(&args[1], ch) {
                Ok(p) => p,
                Err(e) => {
                    println!("{{\"ok\":false,\"engine\":\"syom\",\"error\":\"{e}\"}}");
                    return ExitCode::from(2);
                }
            };
            let samples = planes.first().map_or(0, Vec::len);
            let mut opts = EncodeOptions::adts().with_bitrate_bps(bps);
            match args[5].as_str() {
                "lc" => {}
                "he" => opts.he = true,
                "he2" => {
                    opts.he = true;
                    opts.ps = true;
                }
                other => {
                    println!("{{\"ok\":false,\"engine\":\"syom\",\"error\":\"mode {other}\"}}");
                    return ExitCode::from(2);
                }
            }
            match encode_with(&planes, rate, &opts) {
                Ok(bytes) => {
                    let actual = bytes.len() as f64 * 8.0 * f64::from(rate) / samples as f64;
                    println!(
                        "{{\"ok\":true,\"engine\":\"syom\",\"mode\":\"{}\",\"rate\":{},\"channels\":{},\"bitrate_bps\":{},\"stream_bytes\":{},\"actual_bps\":{:.1}}}",
                        args[5], rate, ch, bps, bytes.len(), actual
                    );
                    if let Err(e) = fs::write(&args[6], bytes) {
                        eprintln!("write {e}");
                        return ExitCode::from(2);
                    }
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    println!("{{\"ok\":false,\"engine\":\"syom\",\"error\":\"{e:?}\"}}");
                    ExitCode::from(1)
                }
            }
        }
        _ => {
            eprintln!("usage: syom_probe decode FILE | decode-pcm FILE OUT | encode IN RATE CH BPS <lc|he|he2> OUT");
            ExitCode::from(2)
        }
    }
}
