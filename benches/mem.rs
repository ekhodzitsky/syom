//! Peak RSS + allocation count vs in-process peers (same bytes → PCM).

use std::alloc::{GlobalAlloc, Layout, System};
use std::io::Cursor;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use oxideav_aac::decode::StreamDecoder;
use rusty_aac::AacDecoder;
use symphonia::core::codecs::CodecParameters;
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use syom::{decode, encode};

const LC_ADTS: &[u8] = include_bytes!("../src/goldens/sine48.adts");
const LC_M4A: &[u8] = include_bytes!("../src/goldens/sine441.m4a");
const HE_ADTS: &[u8] = include_bytes!("../src/goldens/he48.adts");
const HE_M4A: &[u8] = include_bytes!("../src/goldens/he48.m4a");
const PS_ADTS: &[u8] = include_bytes!("../src/goldens/ps48.adts");
const MC_ADTS: &[u8] = include_bytes!("../src/goldens/mc51.adts");

struct Counter;
static ALLOCS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);

unsafe impl GlobalAlloc for Counter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Relaxed);
        BYTES.fetch_add(layout.size() as u64, Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

use Ordering::Relaxed;

#[global_allocator]
static ALLOC: Counter = Counter;

fn reset_alloc() {
    ALLOCS.store(0, Relaxed);
    BYTES.store(0, Relaxed);
}

fn alloc_snap() -> (u64, u64) {
    (ALLOCS.load(Relaxed), BYTES.load(Relaxed))
}

fn max_rss_bytes() -> u64 {
    #[repr(C)]
    struct RUsage {
        _times: [i64; 4],
        ru_maxrss: i64,
        _rest: [i64; 14],
    }
    unsafe extern "C" {
        fn getrusage(who: i32, usage: *mut RUsage) -> i32;
    }
    let mut u = RUsage {
        _times: [0; 4],
        ru_maxrss: 0,
        _rest: [0; 14],
    };
    // SAFETY: RUSAGE_SELF, stack `u` is valid for getrusage.
    let rc = unsafe { getrusage(0, &mut u) };
    if rc != 0 {
        return 0;
    }
    // macOS: ru_maxrss is bytes. Linux: kilobytes.
    #[cfg(target_os = "macos")]
    {
        u.ru_maxrss.max(0) as u64
    }
    #[cfg(not(target_os = "macos"))]
    {
        (u.ru_maxrss.max(0) as u64).saturating_mul(1024)
    }
}

fn rusty_adts(bytes: &[u8]) -> usize {
    let mut dec = AacDecoder::new();
    let mut pos = 0usize;
    let mut n = 0usize;
    while pos + 7 <= bytes.len() {
        let Ok(hdr) = rusty_aac::parse_adts(&bytes[pos..]) else {
            break;
        };
        let end = (pos + hdr.frame_length).min(bytes.len());
        if let Ok(pcm) = dec.decode(&bytes[pos..end], None) {
            n += pcm.samples.len();
        }
        if hdr.frame_length == 0 {
            break;
        }
        pos = end;
    }
    n
}

fn oxideav_adts(bytes: &[u8]) -> usize {
    let mut dec = StreamDecoder::new();
    dec.decode_all(bytes)
        .ok()
        .map(|fs| fs.iter().map(|f| f.pcm.len()).sum())
        .unwrap_or(0)
}

fn symphonia_all(bytes: &[u8], ext: &str) -> usize {
    let src = Cursor::new(bytes.to_vec());
    let mss = MediaSourceStream::new(Box::new(src), Default::default());
    let mut hint = Hint::new();
    hint.with_extension(ext);
    let Ok(mut format) = symphonia::default::get_probe().probe(
        &hint,
        mss,
        FormatOptions::default(),
        MetadataOptions::default(),
    ) else {
        return 0;
    };
    let Some(track) = format.default_track(TrackType::Audio) else {
        return 0;
    };
    let Some(CodecParameters::Audio(params)) = track.codec_params.clone() else {
        return 0;
    };
    let Ok(mut decoder) = symphonia::default::get_codecs()
        .make_audio_decoder(&params, &AudioDecoderOptions::default())
    else {
        return 0;
    };
    let mut n = 0usize;
    while let Ok(Some(pkt)) = format.next_packet() {
        if let Ok(buf) = decoder.decode(&pkt) {
            n += buf.frames();
        }
    }
    n
}

fn syom_n(bytes: &[u8]) -> usize {
    decode(bytes)
        .ok()
        .and_then(|d| d.channels.first().map(Vec::len))
        .unwrap_or(0)
}

/// Stereo encode input: the LC sine golden plus a 0.8× shadow channel.
fn enc_input() -> (Vec<Vec<f32>>, u32) {
    let fallback = (vec![vec![0.0; 1024]; 2], 48_000);
    let Ok(dec) = decode(LC_ADTS) else {
        return fallback;
    };
    let l = dec.channels.into_iter().next().unwrap_or_default();
    let r = l.iter().map(|&x| x * 0.8).collect();
    (vec![l, r], dec.sample_rate)
}

/// rusty_aac 0.5 encode → ADTS-wrapped byte count (parity with syom).
fn rusty_encode(pcm: &[Vec<f32>], rate: u32) -> usize {
    let mut enc = rusty_aac::AacEncoder::new(rusty_aac::AacEncoderConfig {
        bitrate_bps: 128_000,
        ..Default::default()
    });
    let planes: Vec<&[f32]> = pcm.iter().map(Vec::as_slice).collect();
    if enc.push_pcm_planar(&planes, rate).is_err() {
        return 0;
    }
    enc.finish();
    let mut total = 0usize;
    while let Ok(p) = enc.next_packet() {
        let hdr = rusty_aac::AdtsHeader {
            object_type: 2,
            sample_rate: rate,
            channels: pcm.len() as u16,
            frame_length: 7 + p.data.len(),
            header_len: 7,
        };
        total += rusty_aac::write_adts_header(&hdr).len() + p.data.len();
    }
    total
}

fn row(name: &str, peer: &str, samples: usize, iters: u32, work: impl Fn()) {
    reset_alloc();
    let t0 = Instant::now();
    for _ in 0..iters {
        work();
    }
    let wall = t0.elapsed();
    let (allocs, bytes) = alloc_snap();
    let rss = max_rss_bytes();
    let per = wall / iters;
    println!(
        "{name:8} {peer:10} samples={samples:<6} wall/iter={per:?}  allocs={allocs}  alloc_bytes={bytes}  peak_rss={rss}"
    );
}

fn main() {
    let iters = 200u32;
    println!("iters={iters}  (allocs/bytes are cumulative over {iters} iters)");
    for (name, bytes, ext, rusty, oxideav) in [
        ("lc_adts", LC_ADTS, "aac", true, true),
        ("lc_m4a", LC_M4A, "m4a", false, false),
        ("he_adts", HE_ADTS, "aac", true, true),
        ("he_m4a", HE_M4A, "m4a", false, false),
        ("ps_adts", PS_ADTS, "aac", true, true),
        ("mc_adts", MC_ADTS, "aac", true, true),
    ] {
        let ns = syom_n(bytes);
        row(name, "syom", ns, iters, || {
            let _ = decode(bytes);
        });
        if rusty {
            let nr = rusty_adts(bytes);
            row(name, "rusty_aac", nr, iters, || {
                let _ = rusty_adts(bytes);
            });
        }
        if oxideav {
            let no = oxideav_adts(bytes);
            row(name, "oxideav", no, iters, || {
                let _ = oxideav_adts(bytes);
            });
        }
        let nsy = symphonia_all(bytes, ext);
        row(name, "symphonia", nsy, iters, || {
            let _ = symphonia_all(bytes, ext);
        });
    }
    // Encode: same planar f32 input both sides, ADTS bytes out.
    let (pcm, rate) = enc_input();
    let out_bytes = encode(&pcm, rate).map(|b| b.len()).unwrap_or(0);
    row("enc_lc_st", "syom", out_bytes, iters, || {
        let _ = encode(&pcm, rate);
    });
    let rusty_bytes = rusty_encode(&pcm, rate);
    row("enc_lc_st", "rusty_aac", rusty_bytes, iters, || {
        let _ = rusty_encode(&pcm, rate);
    });
}
