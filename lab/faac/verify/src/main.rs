//! Independent syom decode of FAAC ADTS. Not invoked by cargo test --workspace.

use std::env;
use std::fs;
use std::process::ExitCode;
use syom::{decode_with, DecodeOptions};

fn main() -> ExitCode {
    let Some(path) = env::args().nth(1) else {
        eprintln!("usage: faac_verify FILE.adts");
        return ExitCode::from(2);
    };
    let bytes = match fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("read {path}: {e}");
            return ExitCode::from(2);
        }
    };
    match decode_with(&bytes, &DecodeOptions::unbounded()) {
        Ok(pcm) => {
            let ch = pcm.channels.len();
            let samples = pcm.channels.first().map(|c| c.len()).unwrap_or(0);
            let finite = pcm
                .channels
                .iter()
                .all(|c| c.iter().all(|x| x.is_finite()));
            println!(
                "{{\"ok\":true,\"engine\":\"syom\",\"rate\":{},\"channels\":{},\"samples\":{},\"finite\":{}}}",
                pcm.sample_rate, ch, samples, finite
            );
            if finite && samples > 0 {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(e) => {
            println!("{{\"ok\":false,\"engine\":\"syom\",\"error\":\"{e}\"}}");
            ExitCode::from(1)
        }
    }
}
