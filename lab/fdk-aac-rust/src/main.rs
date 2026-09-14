//! Isolated rust-only fdk-aac-rust decode/encode smoke. Not linked by cargo test.

use fdk_aac_rust::adts::AdtsHeader;
use fdk_aac_rust::decoder::{AacLcDecoder, DecodedAacLcFrame};
use fdk_aac_rust::encoder::{
    ConfiguredPureRustEncoder, EncoderParameter, PureRustEncoderParameters,
};
use std::env;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let Some(cmd) = args.next() else {
        eprintln!("usage: fdk_aac_rust_smoke decode <adts> | encode-lc");
        return ExitCode::from(2);
    };
    match cmd.as_str() {
        "decode" => {
            let Some(path) = args.next() else {
                eprintln!("decode: missing path");
                return ExitCode::from(2);
            };
            match decode_adts(Path::new(&path)) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("FAIL {path}: {e}");
                    ExitCode::from(1)
                }
            }
        }
        "encode-lc" => match encode_lc_silence() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("FAIL encode-lc: {e}");
                ExitCode::from(1)
            }
        },
        other => {
            eprintln!("unknown command {other}");
            ExitCode::from(2)
        }
    }
}

fn decode_adts(path: &Path) -> Result<(), String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let first = AdtsHeader::parse(&bytes).map_err(|e| format!("adts header: {e:?}"))?;
    let rate = first.sample_rate().ok_or("sample rate")?;
    let mut dec =
        AacLcDecoder::from_adts_header(first).map_err(|e| format!("open decoder: {e:?}"))?;
    let mut pos = 0usize;
    let mut frames = 0u64;
    let mut samples = 0u64;
    let mut channels = 0usize;
    while pos + 7 <= bytes.len() {
        let hdr = AdtsHeader::parse(&bytes[pos..]).map_err(|e| format!("frame {frames}: {e:?}"))?;
        let n = hdr.frame_length;
        if n == 0 || pos + n > bytes.len() {
            break;
        }
        let decoded = dec
            .decode_adts_frame_f32(&bytes[pos..pos + n])
            .map_err(|e| format!("decode frame {frames}: {e:?}"))?;
        channels = decoded.channels();
        let pcm = pcm_of(&decoded);
        if pcm.iter().any(|x| !x.is_finite()) {
            return Err(format!("non-finite PCM at frame {frames}"));
        }
        samples += decoded.samples_per_channel() as u64;
        frames += 1;
        pos += n;
    }
    if frames == 0 {
        return Err("no frames".into());
    }
    println!(
        "ok path={} rate={} ch={} frames={} samples={} bytes={}",
        path.display(),
        rate,
        channels,
        frames,
        samples,
        bytes.len()
    );
    Ok(())
}

fn pcm_of(frame: &DecodedAacLcFrame) -> Vec<f32> {
    frame.interleaved_f32()
}

fn encode_lc_silence() -> Result<(), String> {
    let mut p = PureRustEncoderParameters::new(1);
    p.set_parameter(EncoderParameter::SampleRate, 48_000)
        .map_err(|e| format!("sr: {e:?}"))?;
    p.set_parameter(EncoderParameter::ChannelMode, 1)
        .map_err(|e| format!("ch: {e:?}"))?;
    p.set_parameter(EncoderParameter::Bitrate, 128_000)
        .map_err(|e| format!("br: {e:?}"))?;
    p.set_parameter(EncoderParameter::TransportMux, 2)
        .map_err(|e| format!("mux: {e:?}"))?;
    p.set_parameter(EncoderParameter::Afterburner, 0)
        .map_err(|e| format!("ab: {e:?}"))?;
    let mut enc =
        ConfiguredPureRustEncoder::from_parameters(&p).map_err(|e| format!("open enc: {e:?}"))?;
    let n = enc.input_samples_per_channel();
    let pcm = vec![0.0f32; n];
    let out = enc
        .encode_transport_f32(&pcm)
        .map_err(|e| format!("encode: {e:?}"))?;
    if out.is_empty() {
        return Err("empty transport".into());
    }
    println!(
        "ok encode-lc rate=48000 ch=1 frame_samples={} adts_bytes={} delay={}",
        n,
        out.len(),
        enc.encoder_delay()
    );
    Ok(())
}
