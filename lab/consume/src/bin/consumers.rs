//! TASK-102 AC#2: faithful minimal reproductions of the two real consumer
//! call patterns, built against the packaged syom crate.
//!
//! kover (`kover-infer/src/extract_proc.rs`, `kover-core/src/media/mod.rs`)
//! treats AAC as a helper *process*: `HELPER IN.mp4 -o OUT.pcm` writes
//! 16 kHz s16le mono through two temp files, and any failure collapses to
//! an exit status mapped to one untyped `KoverError::Media`. The in-library
//! reproduction below keeps the same input (M4A bytes in memory) and the
//! same output contract (16 kHz s16le mono bytes) without the process,
//! the temp files, or the early quantization.
//!
//! sluh (`src/wav.rs`) ingests WAVE via ryf and resamples to 16 kHz mono
//! s16; AAC only ever reaches it as already-extracted PCM. The reproduction
//! shows the syom seam feeding that contract directly as planar f32.

use std::alloc::{GlobalAlloc, Layout as AllocLayout, System};
use std::io::Cursor;
use std::sync::atomic::{AtomicUsize, Ordering};

use syom::{DecodeOptions, EncodeOptions, ProbeDuration, ProbeTrim};

static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static ALLOC_BYTES: AtomicUsize = AtomicUsize::new(0);

struct Counting;

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: AllocLayout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        ALLOC_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        System.alloc(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: AllocLayout) {
        System.dealloc(ptr, layout)
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn alloc_stats() -> (usize, usize) {
    (
        ALLOCS.load(Ordering::Relaxed),
        ALLOC_BYTES.load(Ordering::Relaxed),
    )
}

/// Lab-only deterministic Lanczos-3 resampler (radius stretched for
/// downsampling, normalized). Not product code; syom never resamples.
fn resample(src: &[f32], from: u32, to: u32) -> Vec<f32> {
    fn lanczos3(x: f64) -> f64 {
        let a = x.abs();
        if a >= 3.0 {
            return 0.0;
        }
        if a == 0.0 {
            return 1.0;
        }
        let px = std::f64::consts::PI * x;
        px.sin() * (px / 3.0).sin() * 3.0 / (px * px)
    }
    let step = f64::from(from) / f64::from(to);
    let stretch = step.max(1.0);
    let radius = 3.0 * stretch;
    let n = (src.len() as f64 / step) as usize;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let center = i as f64 * step;
        let lo = (center - radius).ceil().max(0.0) as usize;
        let hi = ((center + radius) as usize).min(src.len().saturating_sub(1));
        let (mut acc, mut wsum) = (0.0f64, 0.0f64);
        for (j, &s) in src.iter().enumerate().take(hi + 1).skip(lo) {
            let w = lanczos3((j as f64 - center) / stretch);
            acc += w * f64::from(s);
            wsum += w;
        }
        out.push((acc / wsum) as f32);
    }
    out
}

fn to_s16le(pcm: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(pcm.len() * 2);
    for &x in pcm {
        let v = (f64::from(x) * 32767.0)
            .round()
            .clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

/// kover's `MediaIo::extract` seam, in-library: M4A bytes -> 16 kHz s16le mono.
fn extract_like_kover(mp4: &[u8]) -> syom::Result<(u32, usize, Vec<u8>)> {
    // The speech default is exactly kover's ask: one mono plane. Nothing is
    // resampled or quantized by syom; rate/layout come from the stream.
    let speech = syom::decode_with(mp4, &DecodeOptions::speech())?;
    let native = speech.sample_rate;
    let valid = speech.channels[0].len();
    let mono16 = resample(&speech.channels[0], native, 16_000);
    Ok((native, valid, to_s16le(&mono16)))
}

fn main() {
    // 2 s stereo 48 kHz "speech-ish" source, muxed as M4A in memory: this is
    // what kover holds when it would spawn the helper.
    let l: Vec<f32> = (0..96_000)
        .map(|i| 0.25 * (i as f32 * 0.11).sin() + 0.1 * (i as f32 * 0.031).sin())
        .collect();
    let r: Vec<f32> = l.iter().map(|&x| 0.6 * x).collect();
    let planes: [&[f32]; 2] = [&l, &r];
    let mut m4a = Cursor::new(Vec::new());
    if let Err(e) = syom::encode_write_m4a(&mut m4a, &planes, 48_000, &EncodeOptions::m4a()) {
        eprintln!("fixture m4a encode failed: {e}");
        std::process::exit(1);
    }
    let m4a = m4a.into_inner();
    let mut failed = false;

    // --- kover cell -------------------------------------------------------
    let (a0, b0) = alloc_stats();
    match extract_like_kover(&m4a) {
        Ok((native, valid, s16)) => {
            let (a1, b1) = alloc_stats();
            let expect = 2 * 16_000 * 2; // 2 s, 16 kHz, s16le mono
            let line = format!(
                "cell kover-extract: {} B M4A in memory -> {} B s16le 16 kHz mono; native {} Hz / {} valid samples retained until the consumer's own resample; one-shot extract cost {} allocs / {} B heap; zero processes, zero temp files, zero WAV wraps",
                m4a.len(), s16.len(), native, valid, a1 - a0, b1 - b0);
            if s16.len() != expect || native != 48_000 || valid != 96_000 {
                println!("FAIL {line} (want {expect} B out, 48000 Hz, 96000 samples)");
                failed = true;
            } else {
                println!("{line}");
            }
        }
        Err(e) => {
            println!("FAIL cell kover-extract: {e}");
            failed = true;
        }
    }
    // Where the helper collapses every failure to an exit status, the
    // library surfaces a typed, matchable error.
    match extract_like_kover(b"\x00\x00\x00\x18not-really-an-mp4") {
        Err(e) => println!("cell kover-error: garbage M4A -> {e:?} (helper path: exit status -> KoverError::Media, cause lost)"),
        Ok(_) => {
            println!("FAIL cell kover-error: garbage decoded");
            failed = true;
        }
    }

    // --- sluh cell --------------------------------------------------------
    // sluh's contract is 16 kHz mono s16 (`wav.rs::load_16k_mono_s16`); the
    // repro feeds it from AAC without a WAV file or an s16 intermediate.
    match extract_like_kover(&m4a) {
        Ok((_, _, s16)) => println!(
            "cell sluh-ingest: AAC -> speech mono f32 (native) -> consumer resample -> {} B s16le; s16 quantization happens once, at the ASR boundary",
            s16.len()),
        Err(e) => {
            println!("FAIL cell sluh-ingest: {e}");
            failed = true;
        }
    }

    // Native rate and layout survive when the consumer asks for them.
    match syom::decode_with(&m4a, &DecodeOptions::audio()) {
        Ok(full) => {
            let exact = full.channels.iter().all(|c| c.len() == 96_000);
            let shape = (
                full.channels.len(),
                full.sample_rate,
                full.layout,
                full.priming,
            );
            if exact && full.channels.len() == 2 && full.sample_rate == 48_000 {
                println!(
                    "cell native-layout: same bytes as full fidelity -> {:?} (2 planes, sample-exact 96000 via elst, priming {:?}); the helper contract discards all of this",
                    shape, full.priming);
            } else {
                println!("FAIL cell native-layout: {shape:?} exact={exact}");
                failed = true;
            }
        }
        Err(e) => {
            println!("FAIL cell native-layout: {e}");
            failed = true;
        }
    }
    match syom::probe(&m4a) {
        Ok(p) => {
            let okd = matches!(p.duration, ProbeDuration::Exact { samples: 96_000 })
                && matches!(p.trim, ProbeTrim::Exact { priming: 1024, .. });
            println!(
                "cell probe: duration {:?}, trim {:?} — exact before any PCM is decoded{}",
                p.duration,
                p.trim,
                if okd { "" } else { " (UNEXPECTED)" }
            );
            if !okd {
                failed = true;
            }
        }
        Err(e) => {
            println!("FAIL cell probe: {e}");
            failed = true;
        }
    }

    // No PCM rewrap on the way in either: borrowed planes encode directly.
    let borrowed: syom::Result<Vec<u8>> = syom::encode(&planes, 48_000);
    let owned = syom::encode(&[l.clone(), r.clone()], 48_000);
    match (borrowed, owned) {
        (Ok(a), Ok(b)) if a == b => println!(
            "cell borrowed-encode: &[&[f32]] planes encode byte-identical to owned ({} B) — no consumer-side copy",
            a.len()),
        _ => {
            println!("FAIL cell borrowed-encode");
            failed = true;
        }
    }

    if failed {
        std::process::exit(1);
    }
}
