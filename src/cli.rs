//! Clap surface. Decode lives in `extract`.

use crate::error::SyomError;
use crate::extract;
use clap::Parser;
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "syom",
    version,
    about = "On-device audio extract",
    long_about = "syom — съём. SYOM: Speech Yielded from Original Media.\n\
MP4/M4A (AAC or PCM), ADTS, WAV in. 16 kHz s16le mono out. No cloud. No ffmpeg."
)]
pub struct Cli {
    /// Input file (MP4/M4A AAC or PCM, ADTS AAC, or PCM WAV).
    pub input: PathBuf,
    /// Output path. `.pcm` is raw s16le; anything else is WAV. `-` is WAV on stdout.
    #[arg(short, long)]
    pub output: Option<PathBuf>,
}

pub fn main() -> Result<(), SyomError> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("syom=info")),
        )
        .init();
    run(Cli::parse())
}

pub fn run(cli: Cli) -> Result<(), SyomError> {
    let pcm = extract::file_to_pcm16(&cli.input)?;
    match cli.output.as_deref() {
        Some(p) if p.as_os_str() == "-" => {
            let wav = crate::wav::encode_16k_mono_s16(&pcm);
            std::io::stdout().write_all(&wav)?;
            Ok(())
        }
        Some(p) => extract::write_out(p, &pcm),
        None => extract::write_out(&PathBuf::from("out.wav"), &pcm),
    }
}

#[cfg(test)]
#[path = "cli_tests.rs"]
mod cli_tests;
