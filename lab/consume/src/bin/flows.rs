//! TASK-102 AC#1: realistic consumer flows against the packaged syom crate.
//!
//! Each scenario prints one `cell` evidence line and returns an error
//! string on failure; `main` exits nonzero if any cell fails. Fixtures are
//! synthesized and coded by syom itself — this lab measures integration
//! cost, not codec conformance (goldens + oracles cover that; no external
//! codec runs here).

use std::alloc::{GlobalAlloc, Layout as AllocLayout, System};
use std::io::{Cursor, Write};
use std::sync::atomic::{AtomicUsize, Ordering};

use syom::{
    AacError, DecodeOptions, Decoder, EncodeContainer, EncodeOptions, Encoder, Layout,
    ProbeContainer, ProbeDuration,
};

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

/// Deterministic stereo source: distinct tones per channel, 1 s at 48 kHz.
fn stereo_source() -> (Vec<f32>, Vec<f32>) {
    let l: Vec<f32> = (0..48_000)
        .map(|i| 0.3 * (i as f32 * 2.0 * std::f32::consts::PI * 440.0 / 48_000.0).sin())
        .collect();
    let r: Vec<f32> = (0..48_000)
        .map(|i| 0.2 * (i as f32 * 2.0 * std::f32::consts::PI * 660.0 / 48_000.0 + 0.5).sin())
        .collect();
    (l, r)
}

fn snr_db(reference: &[f32], decoded: &[f32]) -> f64 {
    let mut signal = 0.0f64;
    let mut noise = 0.0f64;
    for (&s, &d) in reference.iter().zip(decoded) {
        signal += f64::from(s) * f64::from(s);
        let e = f64::from(s) - f64::from(d);
        noise += e * e;
    }
    10.0 * (signal / noise.max(1e-30)).log10()
}

fn corr(a: &[f32], b: &[f32]) -> f64 {
    let (mut ab, mut aa, mut bb) = (0.0f64, 0.0f64, 0.0f64);
    for (&x, &y) in a.iter().zip(b) {
        ab += f64::from(x) * f64::from(y);
        aa += f64::from(x) * f64::from(x);
        bb += f64::from(y) * f64::from(y);
    }
    ab / (aa.sqrt() * bb.sqrt()).max(1e-30)
}

fn class(e: &AacError) -> String {
    let name = match e {
        AacError::Io(_) => "Io",
        AacError::NotAac => "NotAac",
        AacError::UnsupportedSampleRate { .. } => "UnsupportedSampleRate",
        AacError::TooLong { .. } => "TooLong",
        AacError::InvalidLimits(_) => "InvalidLimits",
        AacError::Limit { .. } => "Limit",
        AacError::Unsupported(_) => "Unsupported",
        AacError::Truncated { .. } => "Truncated",
        AacError::NeedMore { .. } => "NeedMore",
        AacError::Malformed(_) => "Malformed",
        AacError::Lifecycle { .. } => "Lifecycle",
        AacError::InvalidPcm(_) => "InvalidPcm",
        AacError::Format(_) => "Format",
        AacError::Decode(_) => "Decode",
        AacError::Encode(_) => "Encode",
        _ => "future-variant",
    };
    format!("{name}: {e}")
}

type Cell = Result<String, String>;

fn ok(name: &str, detail: String) -> Cell {
    Ok(format!("cell {name}: {detail}"))
}

fn fail(name: &str, detail: impl std::fmt::Display) -> Cell {
    Err(format!("cell {name}: {detail}"))
}

/// bytes -> PCM, speech default and full fidelity; stereo kept at native rate.
fn cell_bytes_decode(adts: &[u8], l: &[f32], r: &[f32]) -> Cell {
    let name = "bytes-decode";
    let speech = syom::decode(adts).map_err(|e| format!("speech decode: {e}"))?;
    if speech.channels.len() != 1 || speech.sample_rate != 48_000 {
        return fail(
            name,
            format!(
                "speech shape {:?}",
                (speech.channels.len(), speech.sample_rate)
            ),
        );
    }
    let full = syom::decode_with(adts, &DecodeOptions::audio())
        .map_err(|e| format!("full decode: {e}"))?;
    if (full.channels.len(), full.sample_rate) != (2, 48_000) {
        return fail(
            name,
            format!("full shape {:?}", (full.channels.len(), full.sample_rate)),
        );
    }
    if full.layout != Layout::Mpeg(2) || speech.layout != Layout::SpeechMono {
        return fail(
            name,
            format!("layouts {:?} {:?}", full.layout, speech.layout),
        );
    }
    // ADTS carries no trim: skip the documented 1024-sample priming by hand.
    let dl = &full.channels[0][1024..1024 + l.len()];
    let dr = &full.channels[1][1024..1024 + r.len()];
    let (sl, sr) = (snr_db(l, dl), snr_db(r, dr));
    if sl < 25.0 || sr < 25.0 {
        return fail(name, format!("SNR L {sl:.1} dB R {sr:.1} dB"));
    }
    // Speech mono is documented as the mean of the decoded planes.
    let mean: Vec<f32> = dl.iter().zip(dr).map(|(&a, &b)| 0.5 * (a + b)).collect();
    let ds = &speech.channels[0][1024..1024 + l.len()];
    let c = corr(ds, &mean);
    if c < 0.999 {
        // TASK-122 (fixed 2026-09-23): the LC stereo fast-mono path used to
        // return the LEFT plane untouched (corr mean 0.8319, corr L 1.0000).
        return fail(
            name,
            format!("speech mono != mean of the planes (corr mean {c:.4})"),
        );
    }
    ok(name, format!(
        "1 call each way; speech = {} ch, full = {} ch @ {} Hz {:?}, SNR L {sl:.1} / R {sr:.1} dB, no resample",
        speech.channels.len(), full.channels.len(), full.sample_rate, full.layout))
}

/// file -> PCM and `Read` -> PCM (ADTS and LOAS), plus sniff/probe.
fn cell_file_and_reader(adts: &[u8]) -> Cell {
    let name = "file-reader-decode";
    let dir = std::env::temp_dir().join(format!("syom-consume-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("cell.adts");
    std::fs::write(&path, adts).map_err(|e| e.to_string())?;
    let from_file = syom::read(&path).map_err(|e| e.to_string());
    let from_file_full = syom::read_with(&path, &DecodeOptions::audio()).map_err(|e| e.to_string());
    let _ = std::fs::remove_dir_all(&dir);
    let from_file = from_file?;
    let from_file_full = from_file_full?;
    if from_file.channels.len() != 1 || from_file_full.channels.len() != 2 {
        return fail(name, "file decode shape");
    }

    // Any `Read` for ADTS; LATM/LOAS from the push encoder streams the same way.
    let from_read =
        syom::decode_read(Cursor::new(adts)).map_err(|e| format!("decode_read: {e}"))?;
    if from_read.channels.len() != 1 || from_read.sample_rate != 48_000 {
        return fail(name, "decode_read shape");
    }
    let mut enc = Encoder::new(
        48_000,
        2,
        &EncodeOptions::adts().with_container(EncodeContainer::Latm),
    )
    .map_err(|e| format!("latm encoder: {e}"))?;
    let tail = vec![0.05f32; 4096];
    let mut loas = Vec::new();
    enc.feed(&[&tail[..], &tail[..]], |f| {
        loas.extend_from_slice(f.au);
        Ok(())
    })
    .map_err(|e| format!("latm feed: {e}"))?;
    enc.finish(|f| {
        loas.extend_from_slice(f.au);
        Ok(())
    })
    .map_err(|e| format!("latm finish: {e}"))?;
    let from_loas = syom::decode_read_with(Cursor::new(&loas), &DecodeOptions::audio())
        .map_err(|e| format!("loas decode: {e}"))?;
    if from_loas.channels.len() != 2 || from_loas.sample_rate != 48_000 {
        return fail(
            name,
            format!(
                "loas shape {:?}",
                (from_loas.channels.len(), from_loas.sample_rate)
            ),
        );
    }

    let sniffs = syom::sniff_is_adts(adts)
        && syom::sniff_is_latm(&loas)
        && !syom::sniff_is_latm(adts)
        && !syom::sniff_is_adts(&loas)
        && syom::sniff_aac(adts)
        && !syom::sniff_is_isobmff(adts);
    if !sniffs {
        return fail(name, "sniff mismatch");
    }
    let probe = syom::probe(adts).map_err(|e| format!("probe: {e}"))?;
    if probe.container != ProbeContainer::Adts || !matches!(probe.duration, ProbeDuration::Unknown)
    {
        return fail(name, format!("probe {:?}", probe.container));
    }
    ok(name, format!(
        "read(path) + read_with; decode_read over ADTS ({} B) and LOAS ({} B); sniff/probe agree (ADTS duration Unknown: no index)",
        adts.len(), loas.len()))
}

/// Raw network access units: ASC + payload push both ways (RTP-style).
fn cell_raw_au() -> Cell {
    let name = "raw-au";
    let pcm: Vec<f32> = (0..10_000).map(|i| 0.1 * (i as f32 * 0.05).sin()).collect();
    let mut enc = Encoder::new(48_000, 1, &EncodeOptions::raw()).map_err(|e| e.to_string())?;
    let asc = enc.asc().to_vec();
    let mut aus: Vec<Vec<u8>> = Vec::new();
    let mut collect = |f: syom::EncodedFrame<'_>| {
        aus.push(f.payload.to_vec());
        Ok(())
    };
    for chunk in pcm.chunks(777) {
        enc.feed(&[chunk], &mut collect)
            .map_err(|e| e.to_string())?;
    }
    let info = enc.finish(&mut collect).map_err(|e| e.to_string())?;
    let mut dec =
        Decoder::from_asc(&asc, DecodeOptions::audio()).map_err(|e| format!("from_asc: {e}"))?;
    let (mut frames, mut samples, mut meta_seen) = (0u64, 0u64, None);
    for au in &aus {
        dec.decode_au(au, |f| {
            frames += 1;
            samples += f.samples as u64;
            if meta_seen.is_none() {
                meta_seen = Some(f.meta);
            }
            Ok(())
        })
        .map_err(|e| format!("decode_au: {e}"))?;
    }
    if frames != aus.len() as u64 || samples != u64::from(info.aac_frames) * 1024 {
        return fail(
            name,
            format!("frames {frames}/{} samples {samples}", aus.len()),
        );
    }
    // Re-framing for a byte-stream consumer is one public call.
    let reframed: Vec<u8> = aus
        .iter()
        .map(|au| syom::wrap_adts_au(au, 48_000, 1))
        .collect::<syom::Result<Vec<_>>>()
        .map_err(|e| e.to_string())?
        .concat();
    let round = syom::decode_with(&reframed, &DecodeOptions::audio())
        .map_err(|e| format!("reframed: {e}"))?;
    if round.channels[0].len() != samples as usize {
        return fail(name, "reframed length");
    }
    ok(name, format!(
        "Encoder::raw + asc ({} B) -> {} AUs; Decoder::from_asc + decode_au -> {samples} samples; frame meta {:?}; wrap_adts_au re-frames",
        asc.len(), aus.len(), meta_seen.map(|m| m.layout)))
}

/// A generic `Write` sink, a failing sink's exact prefix, and M4A seek I/O.
fn cell_sink_encode(l: &[f32], r: &[f32]) -> Cell {
    let name = "sink-encode";
    struct CountingSink {
        buf: Vec<u8>,
        calls: usize,
    }
    impl Write for CountingSink {
        fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
            self.calls += 1;
            self.buf.extend_from_slice(data);
            Ok(data.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut sink = CountingSink {
        buf: Vec::new(),
        calls: 0,
    };
    let planes: [&[f32]; 2] = [l, r];
    let info = syom::encode_write(&mut sink, &planes, 48_000, &EncodeOptions::adts())
        .map_err(|e| format!("encode_write: {e}"))?;
    if info.samples as usize != l.len()
        || sink.buf != syom::encode(&planes, 48_000).map_err(|e| e.to_string())?
    {
        return fail(name, "sink output != one-shot encode");
    }

    struct FailAfter {
        left: usize,
        written: Vec<u8>,
    }
    impl Write for FailAfter {
        fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
            if data.len() > self.left {
                let take = self.left;
                self.written.extend_from_slice(&data[..take]);
                self.left = 0;
                return Err(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "sink full",
                ));
            }
            self.left -= data.len();
            self.written.extend_from_slice(data);
            Ok(data.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut failing = FailAfter {
        left: sink.buf.len() / 2,
        written: Vec::new(),
    };
    match syom::encode_write(&mut failing, &planes, 48_000, &EncodeOptions::adts()) {
        Err(AacError::Io(_)) => {
            if failing.written != sink.buf[..failing.written.len()] {
                return fail(name, "sink error prefix mismatch");
            }
        }
        other => return fail(name, format!("expected Io error, got {}", other.is_ok())),
    }

    let mut m4a = Cursor::new(Vec::new());
    syom::encode_write_m4a(&mut m4a, &planes, 48_000, &EncodeOptions::m4a())
        .map_err(|e| format!("m4a write: {e}"))?;
    let back = syom::decode_seek_with(Cursor::new(m4a.into_inner()), &DecodeOptions::audio())
        .map_err(|e| format!("decode_seek: {e}"))?;
    if back.channels[0].len() != l.len() || back.channels.len() != 2 || back.priming != Some(1024) {
        return fail(
            name,
            format!(
                "m4a roundtrip {:?} priming {:?}",
                back.channels.iter().map(Vec::len).collect::<Vec<_>>(),
                back.priming
            ),
        );
    }
    ok(name, format!(
        "encode_write to a generic Write ({} calls, {} B, byte-exact with one-shot); failing sink -> typed Io with exact prefix; encode_write_m4a/decode_seek roundtrip is sample-exact ({} samples, priming {:?})",
        sink.calls, sink.buf.len(), back.channels[0].len(), back.priming))
}

/// Push streaming in network-sized chunks, bounded by options, zero-alloc steady state.
fn cell_bounded_streaming(adts: &[u8]) -> Cell {
    let name = "bounded-streaming";
    let reference = syom::decode_with(adts, &DecodeOptions::audio())
        .map_err(|e| e.to_string())?
        .channels[0]
        .len();
    let mut dec = Decoder::new(DecodeOptions::audio());
    let mut total = 0usize;
    for chunk in adts.chunks(1_319) {
        dec.feed(chunk, |f| {
            total += f.samples;
            Ok(())
        })
        .map_err(|e| format!("feed: {e}"))?;
    }
    let done = dec
        .finish(|f| {
            total += f.samples;
            Ok(())
        })
        .map_err(|e| format!("finish: {e}"))?;
    if total != reference || done.samples as usize != reference {
        return fail(name, format!("chunked {total} != one-shot {reference}"));
    }

    // The cap fires during `feed`, before the stream is buffered whole.
    let mut capped = Decoder::new(DecodeOptions::audio().with_max_duration_secs(5.0));
    let mut capped_err = None;
    for chunk in adts.chunks(1_319) {
        if let Err(e) = capped.feed(chunk, |_| Ok(())) {
            capped_err = Some(e);
            break;
        }
    }
    let capped_err = capped_err.ok_or("duration cap did not fire")?;
    if !matches!(
        capped_err,
        AacError::TooLong { .. } | AacError::Limit { .. }
    ) {
        return fail(name, format!("cap error: {}", class(&capped_err)));
    }

    // Steady-state heap: pass 1 warms the instance with the same chunked
    // pattern (the resident byte buffer reaches its working capacity),
    // reset() keeps the workspace, pass 2 must not allocate.
    let mut dec = Decoder::new(DecodeOptions::audio());
    for chunk in adts.chunks(1_319) {
        dec.feed(chunk, |_| Ok(())).map_err(|e| e.to_string())?;
    }
    dec.finish(|_| Ok(())).map_err(|e| e.to_string())?;
    dec.reset();
    // finish() takes the resident push buffer, so a fresh stream re-grows it
    // during its first chunks; steady state starts after that warm-up.
    for chunk in adts[..4_000].chunks(1_319) {
        dec.feed(chunk, |_| Ok(())).map_err(|e| e.to_string())?;
    }
    let (a0, b0) = alloc_stats();
    for chunk in adts[4_000..].chunks(1_319) {
        dec.feed(chunk, |_| Ok(())).map_err(|e| e.to_string())?;
    }
    dec.finish(|_| Ok(())).map_err(|e| e.to_string())?;
    let (a1, b1) = alloc_stats();
    let (d_alloc, d_bytes) = (a1 - a0, b1 - b0);
    if d_alloc != 0 {
        return fail(
            name,
            format!("steady-state allocations {d_alloc} (+{d_bytes} B)"),
        );
    }
    ok(name, format!(
        "1319-B chunks == one-shot ({total} samples); 10-s stream trips a 5-s cap with {}; reset() + second pass: {d_alloc} allocations after warm-up",
        class(&capped_err)))
}

/// Typed, distinguishable outcomes for hostile input and misuse.
fn cell_errors(adts: &[u8]) -> Cell {
    let name = "error-handling";
    let mut seen: Vec<String> = Vec::new();
    fn record(
        seen: &mut Vec<String>,
        label: &str,
        r: syom::Result<syom::DecodedAac>,
    ) -> Result<(), String> {
        match r {
            Err(e) => {
                seen.push(format!("{label} -> {}", class(&e)));
                Ok(())
            }
            Ok(_) => Err(format!("{label} unexpectedly succeeded")),
        }
    }
    record(&mut seen, "garbage", syom::decode(b"not aac at all"))?;
    // An ADTS stream cut mid-frame is *not* an error in one-shot decode:
    // finish drops the trailing partial frame (documented in pump.rs), so
    // the loss shows up as fewer samples. Record the measured delta.
    let whole = syom::decode(adts).map_err(|e| e.to_string())?;
    let cut = syom::decode(&adts[..adts.len() - 100]).map_err(|e| e.to_string())?;
    let dropped = whole.channels[0].len() - cut.channels[0].len();
    if dropped != 1024 {
        return fail(
            name,
            format!("tail cut dropped {dropped} samples, want 1024"),
        );
    }
    seen.push(format!(
        "truncated-adts-tail -> Ok, -{dropped} samples (documented drop, not silent success)"
    ));
    // A truncated raw access unit, by contrast, is a hard Truncated error.
    let mut enc = Encoder::new(48_000, 1, &EncodeOptions::raw()).map_err(|e| e.to_string())?;
    let mut au = Vec::new();
    enc.feed(&[&[0.1f32; 1024][..]], |f| {
        au.extend_from_slice(f.payload);
        Ok(())
    })
    .map_err(|e| e.to_string())?;
    let asc = enc.asc().to_vec();
    let mut raw = Decoder::from_asc(&asc, DecodeOptions::audio()).map_err(|e| e.to_string())?;
    match raw.decode_au(&au[..au.len() / 2], |_| Ok(())) {
        Err(e) => seen.push(format!("truncated-raw-au -> {}", class(&e))),
        Ok(()) => return fail(name, "truncated raw AU decoded"),
    }
    // MPEG-4 ADTS header declaring AAC Main (profile 0): legal syntax syom
    // does not implement. 44100 Hz (index 4), stereo, 519-byte frame.
    let mut main_profile = vec![0xFF, 0xF1, 0x10, 0x80, 0x40, 0xFF, 0xFC];
    main_profile.extend_from_slice(&[0u8; 512]);
    record(&mut seen, "aac-main-profile", syom::decode(&main_profile))?;
    record(
        &mut seen,
        "tiny-asc",
        syom::decode_with(&[0x11], &DecodeOptions::audio()),
    )?;

    match syom::encode(&[vec![1.5f32; 2048]], 48_000) {
        Err(e @ AacError::InvalidPcm(_)) => seen.push(format!("|x|>1 -> {}", class(&e))),
        other => return fail(name, format!("loud PCM: {}", other.is_ok())),
    }
    match syom::encode(&[vec![f32::NAN; 1024]], 48_000) {
        Err(e @ AacError::InvalidPcm(_)) => seen.push(format!("NaN -> {}", class(&e))),
        other => return fail(name, format!("NaN PCM: {}", other.is_ok())),
    }
    let seven = vec![vec![0.1f32; 2048]; 7];
    match syom::encode(&seven, 48_000) {
        Err(e) => seen.push(format!("7-planes -> {}", class(&e))),
        Ok(_) => return fail(name, "7 planes encoded"),
    }

    // Lifecycle: a short garbage feed just buffers (no full frame yet); the
    // failure surfaces at finish() and poisons the instance until reset().
    let mut dec = Decoder::new(DecodeOptions::audio());
    let buffered = dec.feed(b"\xDE\xAD\xBE\xEF", |_| Ok(()));
    let failed = dec.finish(|_| Ok(()));
    let poisoned = dec.feed(b"\xDE\xAD\xBE\xEF", |_| Ok(()));
    let poisoned_is_lifecycle = matches!(poisoned, Err(AacError::Lifecycle { .. }));
    dec.reset();
    let recovered = dec.feed(&adts[..2_000], |_| Ok(()));
    if buffered.is_err() || failed.is_ok() || !poisoned_is_lifecycle || recovered.is_err() {
        return fail(
            name,
            format!(
                "lifecycle: buffered {}, failed {}, poisoned {}, recovered {}",
                buffered.is_ok(),
                failed.is_err(),
                poisoned_is_lifecycle,
                recovered.is_ok()
            ),
        );
    }
    seen.push(format!(
        "garbage-feed -> {}; failed instance -> Lifecycle until reset(), then recovers",
        class(&failed.err().unwrap_or(AacError::NotAac))
    ));
    ok(name, seen.join(" | "))
}

fn main() {
    let (l, r) = stereo_source();
    let planes: [&[f32]; 2] = [&l, &r];
    let adts = match syom::encode(&planes, 48_000) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("fixture encode failed: {e}");
            std::process::exit(1);
        }
    };
    // A longer mono stream for the bounded/cap cells.
    let long: Vec<f32> = (0..480_000)
        .map(|i| 0.1 * (i as f32 * 0.03).sin())
        .collect();
    let adts_long = match syom::encode(&[long], 48_000) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("long fixture encode failed: {e}");
            std::process::exit(1);
        }
    };

    let cells = [
        cell_bytes_decode(&adts, &l, &r),
        cell_file_and_reader(&adts),
        cell_raw_au(),
        cell_sink_encode(&l, &r),
        cell_bounded_streaming(&adts_long),
        cell_errors(&adts),
    ];
    let mut failed = 0;
    for cell in cells {
        match cell {
            Ok(line) => println!("{line}"),
            Err(line) => {
                println!("FAIL {line}");
                failed += 1;
            }
        }
    }
    if failed > 0 {
        std::process::exit(1);
    }
}
