//! Long synthetic streams, chunk equivalence, seek, and retained heap.
//! No claim about perceptual quality or a consumer's natural recordings.
use std::alloc::{GlobalAlloc, Layout, System};
use std::io::Cursor;
use std::sync::atomic::{AtomicUsize, Ordering};
use syom::{DecodeOptions, Decoder, EncodeOptions};

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
struct Alloc;
unsafe impl GlobalAlloc for Alloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            let live = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(live, Ordering::Relaxed);
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        unsafe { System.dealloc(p, layout) };
    }
}
#[global_allocator]
static ALLOC: Alloc = Alloc;

fn hash(h: &mut u64, samples: &[f32]) {
    for &x in samples {
        assert!(x.is_finite());
        for b in x.to_bits().to_le_bytes() {
            *h = (*h ^ u64::from(b)).wrapping_mul(0x100000001b3);
        }
    }
}

fn main() -> syom::Result<()> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for path in [
        "sine48.adts",
        "he48.adts",
        "ps48.adts",
        "mc51.adts",
        "fmp4_lc.mp4",
        "ld64m.loas",
    ] {
        let bytes = std::fs::read(root.join("src/goldens").join(path))?;
        let reference = syom::decode_with(&bytes, &DecodeOptions::audio())?;
        let mut expected = vec![0xcbf29ce484222325; reference.channels.len()];
        for (h, p) in expected.iter_mut().zip(&reference.channels) {
            hash(h, p);
        }
        for chunk in [1, 7, 1319, 65536] {
            let mut dec = Decoder::new(DecodeOptions::audio());
            let mut got = vec![0xcbf29ce484222325; expected.len()];
            let mut count = 0usize;
            let mut sink = |f: syom::Frame<'_>| {
                assert_eq!(f.planar.len(), got.len());
                for (h, p) in got.iter_mut().zip(f.planar) {
                    hash(h, p);
                }
                count += f.samples;
                Ok(())
            };
            for part in bytes.chunks(chunk) {
                dec.feed(part, &mut sink)?;
            }
            dec.finish(&mut sink)?;
            assert_eq!(count, reference.channels[0].len());
            assert_eq!(got, expected, "{path} chunks={chunk}");
        }
        println!("chunk-equivalence {path}: chunks 1/7/1319/65536; exact PCM and sample count");
    }

    let n = 120 * 48_000;
    let pcm: Vec<f32> = (0..n).map(|i| 0.2 * (i as f32 * 0.03).sin()).collect();
    let bytes = syom::encode_with(&[&pcm], 48_000, &EncodeOptions::m4a())?;
    let reference = syom::decode_with(&bytes, &DecodeOptions::audio())?;
    assert_eq!(reference.channels[0].len(), n);
    let mut seek = syom::M4aSeek::open(Cursor::new(&bytes), DecodeOptions::audio())?;
    for at in [0, 1, 1023, 48_123, n - 2048] {
        seek.seek(at as i64)?;
        let mut first = Vec::new();
        seek.decode(|f| {
            if first.is_empty() {
                first.extend_from_slice(f.planar[0]);
            }
            Ok(())
        })?;
        assert!(!first.is_empty());
        assert_eq!(first, reference.channels[0][at..at + first.len()]);
    }
    println!("long-m4a: 120 s mono; exact source length; 5 sample-exact seeks");
    drop(seek);
    drop((pcm, reference, bytes));

    // Repeat complete untagged AAC frames: a valid long stream, not a natural recording.
    let packet = std::fs::read(root.join("src/goldens/sine48.adts"))?;
    let mut dec = Decoder::new(DecodeOptions::audio());
    let baseline = LIVE.load(Ordering::Relaxed);
    PEAK.store(baseline, Ordering::Relaxed);
    let mut total = 0usize;
    let mut warm_live = 0;
    let mut end_live = 0;
    for i in 0..2164 {
        for chunk in packet.chunks(1319) {
            dec.feed(chunk, |f| {
                assert!(
                    f.planar
                        .iter()
                        .flat_map(|p| p.iter())
                        .all(|x| x.is_finite())
                );
                total += f.samples;
                Ok(())
            })?;
        }
        if i == 9 {
            warm_live = LIVE.load(Ordering::Relaxed);
        }
        if i == 2163 {
            end_live = LIVE.load(Ordering::Relaxed);
        }
    }
    dec.finish(|f| {
        total += f.samples;
        Ok(())
    })?;
    assert_eq!(total, 2164 * 13312);
    assert_eq!(end_live, warm_live, "retained workspace grew with duration");
    println!(
        "long-stream: {total} samples (>600 s); retained heap warm={warm_live} end={end_live}; peak delta={} B",
        PEAK.load(Ordering::Relaxed).saturating_sub(baseline)
    );
    Ok(())
}
