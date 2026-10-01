//! Isolated oxideav-aac 0.1.7 encode adapter (TASK-95). Opt-in lab CLI;
//! never built or spawned by `cargo test --workspace`. Input is planar
//! f32le (one plane per channel), output is ADTS.
use std::fs;
use std::process::ExitCode;

use oxideav_aac::encoder::{EncoderConfig, StreamEncoder};
use oxideav_aac::he_aac_encoder::{HeAacConfig, HeAacEncoder};

fn to_s16_interleaved(planes: &[Vec<f32>]) -> Vec<i16> {
    let n = planes.first().map_or(0, Vec::len);
    let mut out = Vec::with_capacity(n * planes.len());
    for i in 0..n {
        for p in planes {
            out.push((p[i].clamp(-1.0, 1.0) * 32767.0).round() as i16);
        }
    }
    out
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 2 && args[1] == "id" {
        println!("oxideav-aac-0.1.7 ADTS lc|he s16-input");
        return ExitCode::SUCCESS;
    }
    if args.len() == 8 && args[1] == "encode-pcm" {
        let rate: u32 = args[2].parse().unwrap_or(0);
        let ch: u8 = args[3].parse().unwrap_or(0);
        let bps: u32 = args[4].parse().unwrap_or(0);
        let mode = args[5].as_str();
        let raw = match fs::read(&args[6]) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("read {}: {e}", args[6]);
                return ExitCode::from(2);
            }
        };
        if rate == 0 || !(1..=2).contains(&ch) || bps == 0 || raw.len() % (ch as usize * 4) != 0 {
            eprintln!("encode-pcm RATE CH BITRATE_BPS lc|he IN.f32 OUT.adts");
            return ExitCode::from(2);
        }
        let n = raw.len() / 4 / ch as usize;
        let planes: Vec<Vec<f32>> = (0..ch as usize)
            .map(|c| {
                raw[c * n * 4..(c + 1) * n * 4]
                    .chunks_exact(4)
                    .map(|b| f32::from_le_bytes(b.try_into().expect("4 bytes")))
                    .collect()
            })
            .collect();
        let pcm = to_s16_interleaved(&planes);
        let adts = match mode {
            "lc" => StreamEncoder::new(EncoderConfig {
                sample_rate: rate,
                channels: ch,
                bitrate: bps,
            })
            .and_then(|mut e| e.encode_all(&pcm)),
            "he" => HeAacEncoder::new(HeAacConfig::new(rate, ch, bps))
                .and_then(|mut e| e.encode_all(&pcm)),
            _ => {
                eprintln!("mode must be lc or he");
                return ExitCode::from(2);
            }
        };
        match adts {
            Ok(bytes) => {
                if fs::write(&args[7], &bytes).is_err() {
                    eprintln!("write {}", args[7]);
                    return ExitCode::from(2);
                }
                println!(
                    "{{\"ok\":true,\"engine\":\"oxideav-aac-0.1.7\",\"mode\":\"{mode}\",\"adts_bytes\":{},\"bitrate_bps\":{bps}}}",
                    bytes.len()
                );
                ExitCode::SUCCESS
            }
            Err(e) => {
                println!("{{\"ok\":false,\"engine\":\"oxideav-aac-0.1.7\",\"mode\":\"{mode}\",\"error\":\"{e:?}\"}}");
                ExitCode::FAILURE
            }
        }
    } else {
        eprintln!("usage: {} id | encode-pcm RATE CH BPS lc|he IN.f32 OUT.adts", args[0]);
        ExitCode::from(2)
    }
}
