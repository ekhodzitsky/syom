//! Peak RSS + allocation count vs in-process peers (same bytes → PCM).

use std::alloc::{GlobalAlloc, Layout, System};
use std::io::Cursor;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use rusty_aac::AacDecoder;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use syom::decode;

const LC_ADTS: &[u8] = include_bytes!("../src/goldens/sine48.adts");
const LC_M4A: &[u8] = include_bytes!("../src/goldens/sine441.m4a");
const HE_ADTS: &[u8] = include_bytes!("../src/goldens/he48.adts");
const HE_M4A: &[u8] = include_bytes!("../src/goldens/he48.m4a");

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

fn symphonia_all(bytes: &[u8], ext: &str) -> usize {
    let src = Cursor::new(bytes.to_vec());
    let mss = MediaSourceStream::new(Box::new(src), Default::default());
    let mut hint = Hint::new();
    hint.with_extension(ext);
    let Ok(probed) = symphonia::default::get_probe().format(
        &hint,
        mss,
        &FormatOptions::default(),
        &MetadataOptions::default(),
    ) else {
        return 0;
    };
    let mut format = probed.format;
    let Some(track) = format.default_track() else {
        return 0;
    };
    let Ok(mut decoder) =
        symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default())
    else {
        return 0;
    };
    let mut n = 0usize;
    while let Ok(pkt) = format.next_packet() {
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
    for (name, bytes, ext, rusty) in [
        ("lc_adts", LC_ADTS, "aac", true),
        ("lc_m4a", LC_M4A, "m4a", false),
        ("he_adts", HE_ADTS, "aac", true),
        ("he_m4a", HE_M4A, "m4a", false),
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
        let nsy = symphonia_all(bytes, ext);
        row(name, "symphonia", nsy, iters, || {
            let _ = symphonia_all(bytes, ext);
        });
    }
    println!("oxideav-aac: parser-only, not linked");
}
