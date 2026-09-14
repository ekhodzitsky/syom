//! Isolated glint-audio 0.11.0 AAC-LC smoke. Not linked by cargo test.

use glint::{decode_audio, AacDecoder, AacEncoder};
use std::env;
use std::fs;
use std::path::Path;
use std::process::ExitCode;
use std::time::Instant;
use syom::{decode_with, DecodeOptions};

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let Some(cmd) = args.next() else {
        eprintln!(
            "usage: glint_smoke decode <adts> | encode-sine RATE CH KBPS QUALITY OUT | encode-pcm RATE CH KBPS QUALITY IN.s16le.interleaved OUT | overhead | smoke <sine48.adts>"
        );
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
        "encode-sine" => {
            let got: Vec<String> = args.collect();
            if got.len() != 5 {
                eprintln!("encode-sine RATE CH KBPS QUALITY OUT.adts");
                return ExitCode::from(2);
            }
            match encode_sine(&got) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("FAIL encode-sine: {e}");
                    ExitCode::from(1)
                }
            }
        }
        "encode-pcm" => {
            let got: Vec<String> = args.collect();
            if got.len() != 6 {
                eprintln!("encode-pcm RATE CH KBPS QUALITY IN.s16le.interleaved OUT.adts");
                return ExitCode::from(2);
            }
            match encode_pcm_file(&got) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("FAIL encode-pcm: {e}");
                    ExitCode::from(1)
                }
            }
        }
        "overhead" => match overhead() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("FAIL overhead: {e}");
                ExitCode::from(1)
            }
        },
        "smoke" => {
            let Some(path) = args.next() else {
                eprintln!("smoke: missing sine48.adts");
                return ExitCode::from(2);
            };
            match smoke_all(Path::new(&path)) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("FAIL smoke: {e}");
                    ExitCode::from(1)
                }
            }
        }
        other => {
            eprintln!("unknown command {other}");
            ExitCode::from(2)
        }
    }
}

fn decode_adts(path: &Path) -> Result<(), String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let mut dec = AacDecoder::new()?;
    let pcm = dec.decode(&bytes);
    if pcm.iter().any(|x| !x.is_finite()) {
        return Err("non-finite PCM".into());
    }
    let info = dec
        .frame_info(&bytes)
        .ok_or_else(|| "no ADTS frame".to_string())?;
    let ch = info.channels.max(1);
    let samples = pcm.len() as u32 / ch;
    let whole = decode_audio(&bytes);
    println!(
        "ok engine=glint-aac-dec path={} rate={} ch={} samples={} bytes={} whole={}",
        path.display(),
        info.sample_rate,
        ch,
        samples,
        bytes.len(),
        whole
            .as_ref()
            .map(|d| format!("{}x{}/{}", d.sample_rate, d.channels, d.pcm.len()))
            .unwrap_or_else(|| "none".into())
    );
    Ok(())
}

fn encode_sine(got: &[String]) -> Result<(), String> {
    let rate: u32 = got[0].parse().map_err(|_| "rate")?;
    let ch: u32 = got[1].parse().map_err(|_| "ch")?;
    let kbps: u32 = got[2].parse().map_err(|_| "kbps")?;
    let quality: u32 = got[3].parse().map_err(|_| "quality")?;
    let out = Path::new(&got[4]);
    let n = rate * 2;
    let pcm = sine_i16(n, ch, rate, 440.0, 0.5);
    let adts = encode_pcm_aac_checked(&pcm, rate, ch, kbps, quality)?;
    fs::write(out, &adts).map_err(|e| e.to_string())?;
    let indep = syom_decode(&adts)?;
    println!(
        "ok encode-sine rate={} ch={} kbps={} quality={} adts_bytes={} syom_rate={} syom_ch={} syom_samples={} finite={} actual_bps={:.0}",
        rate,
        ch,
        kbps,
        quality,
        adts.len(),
        indep.rate,
        indep.ch,
        indep.samples,
        indep.finite,
        8.0 * adts.len() as f64 / (indep.samples as f64 / indep.rate as f64)
    );
    Ok(())
}

struct SyomOut {
    rate: u32,
    ch: usize,
    samples: usize,
    finite: bool,
}

fn syom_decode(adts: &[u8]) -> Result<SyomOut, String> {
    let pcm = decode_with(adts, &DecodeOptions::unbounded()).map_err(|e| e.to_string())?;
    let ch = pcm.channels.len();
    let samples = pcm.channels.first().map(|c| c.len()).unwrap_or(0);
    let finite = pcm.channels.iter().all(|c| c.iter().all(|x| x.is_finite()));
    Ok(SyomOut {
        rate: pcm.sample_rate,
        ch,
        samples,
        finite,
    })
}

fn sine_i16(n: u32, ch: u32, rate: u32, hz: f64, peak: f64) -> Vec<i16> {
    let mut out = Vec::with_capacity((n * ch) as usize);
    for i in 0..n {
        let s = (2.0 * std::f64::consts::PI * hz * i as f64 / rate as f64).sin() * peak;
        let v = (s * 32767.0).round() as i16;
        for _ in 0..ch {
            out.push(v);
        }
    }
    out
}

fn encode_pcm_file(got: &[String]) -> Result<(), String> {
    let rate: u32 = got[0].parse().map_err(|_| "rate")?;
    let ch: u32 = got[1].parse().map_err(|_| "ch")?;
    let kbps: u32 = got[2].parse().map_err(|_| "kbps")?;
    let quality: u32 = got[3].parse().map_err(|_| "quality")?;
    let raw = fs::read(&got[4]).map_err(|e| e.to_string())?;
    if raw.len() % 2 != 0 {
        return Err("odd s16le".into());
    }
    let mut pcm = Vec::with_capacity(raw.len() / 2);
    for c in raw.chunks_exact(2) {
        pcm.push(i16::from_le_bytes([c[0], c[1]]));
    }
    let adts = encode_pcm_aac_checked(&pcm, rate, ch, kbps, quality)?;
    fs::write(&got[5], &adts).map_err(|e| e.to_string())?;
    println!(
        "ok encode-pcm rate={} ch={} kbps={} quality={} adts_bytes={} samples={}",
        rate,
        ch,
        kbps,
        quality,
        adts.len(),
        pcm.len() as u32 / ch
    );
    Ok(())
}

fn encode_pcm_aac_checked(
    pcm: &[i16],
    rate: u32,
    ch: u32,
    kbps: u32,
    quality: u32,
) -> Result<Vec<u8>, String> {
    let mut enc = AacEncoder::new(rate, ch, kbps, quality)?;
    let frame = enc.samples_per_frame() * enc.channels();
    let mut adts = Vec::new();
    for chunk in pcm.chunks(frame) {
        adts.extend(enc.encode(chunk));
    }
    adts.extend(enc.flush());
    if adts.is_empty() {
        return Err("empty ADTS".into());
    }
    Ok(adts)
}

fn overhead() -> Result<(), String> {
    const FRAMES: usize = 200;
    const RATE: u32 = 48_000;
    let mut enc_w = AacEncoder::new(RATE, 1, 128, 1)?;
    let spf = enc_w.samples_per_frame();
    let frame = vec![0i16; spf];
    for _ in 0..8 {
        let _ = enc_w.encode(&frame);
    }
    let t0 = Instant::now();
    let mut wbytes = 0usize;
    for _ in 0..FRAMES {
        wbytes += enc_w.encode(&frame).len();
    }
    let wrap_ns = t0.elapsed().as_nanos();

    let cfg = glint_audio_sys::glint_aac_config {
        sample_rate: RATE as i32,
        num_channels: 1,
        bitrate: 128,
        quality: 1,
        vbr: 0,
        vbr_quality: 0,
        reserved: [0; 4],
    };
    let handle = unsafe { glint_audio_sys::glint_aac_create(&cfg) };
    if handle.is_null() {
        return Err("glint_aac_create null".into());
    }
    let ptrs = [frame.as_ptr()];
    let mut sz = 0i32;
    for _ in 0..8 {
        unsafe {
            let _ = glint_audio_sys::glint_aac_encode(handle, ptrs.as_ptr(), &mut sz);
        }
    }
    let t1 = Instant::now();
    let mut nbytes = 0usize;
    for _ in 0..FRAMES {
        unsafe {
            let p = glint_audio_sys::glint_aac_encode(handle, ptrs.as_ptr(), &mut sz);
            if !p.is_null() && sz > 0 {
                nbytes += sz as usize;
            }
        }
    }
    let native_ns = t1.elapsed().as_nanos();
    unsafe { glint_audio_sys::glint_aac_destroy(handle) };
    let ver = unsafe { glint_audio_sys::glint_version() };
    println!(
        "ok overhead frames={} wrap_ns={} native_ns={} wrap_ns_per_frame={} native_ns_per_frame={} wrap_bytes={} native_bytes={} glint_version={}.{}.{} extra_copies=interleave_to_planar_vec+output_to_vec",
        FRAMES,
        wrap_ns,
        native_ns,
        wrap_ns / FRAMES as u128,
        native_ns / FRAMES as u128,
        wbytes,
        nbytes,
        (ver >> 16) & 0xff,
        (ver >> 8) & 0xff,
        ver & 0xff
    );
    Ok(())
}

fn smoke_all(sine48: &Path) -> Result<(), String> {
    let ver = unsafe { glint_audio_sys::glint_version() };
    println!(
        "glint_version={}.{}.{} crate=glint-audio-0.11.0 precision=double isa=compiler_sse2_no_aac_simd_field quality=0,1,2",
        (ver >> 16) & 0xff,
        (ver >> 8) & 0xff,
        ver & 0xff
    );
    decode_adts(sine48)?;
    let syom_ref = fs::read(sine48).map_err(|e| e.to_string())?;
    let s = syom_decode(&syom_ref)?;
    println!(
        "ok syom-ref rate={} ch={} samples={} finite={}",
        s.rate, s.ch, s.samples, s.finite
    );
    let mut dec = AacDecoder::new()?;
    let gpcm = dec.decode(&syom_ref);
    let gch = s.ch.max(1);
    let gsamples = gpcm.len() / gch;
    let syom_pcm =
        decode_with(&syom_ref, &DecodeOptions::unbounded()).map_err(|e| e.to_string())?;
    let mut max_abs = 0.0f32;
    if gsamples == s.samples && syom_pcm.channels.len() == gch {
        for (i, &x) in gpcm.iter().enumerate() {
            let c = i % gch;
            let t = i / gch;
            if t < syom_pcm.channels[c].len() {
                max_abs = max_abs.max((x - syom_pcm.channels[c][t]).abs());
            }
        }
    }
    println!(
        "ok glint-vs-syom-decode sine48 glint_samples={} syom_samples={} length_match={} finite={} max_abs={:.6e}",
        gsamples,
        s.samples,
        gsamples == s.samples,
        gpcm.iter().all(|x| x.is_finite()),
        max_abs
    );
    let scratch = Path::new(env!("CARGO_MANIFEST_DIR")).join("target");
    fs::create_dir_all(&scratch).map_err(|e| e.to_string())?;
    for &(ch, kbps, q) in &[
        (1u32, 64u32, 0u32),
        (1, 64, 1),
        (1, 64, 2),
        (1, 128, 1),
        (2, 128, 0),
        (2, 128, 1),
        (2, 128, 2),
    ] {
        let name = format!("q{q}_c{ch}_{kbps}.adts");
        encode_sine(&[
            "48000".into(),
            ch.to_string(),
            kbps.to_string(),
            q.to_string(),
            scratch.join(name).to_string_lossy().into(),
        ])?;
    }
    overhead()?;
    Ok(())
}
