//! In-process AAC decode vs Rust peers (bytes → PCM).
//!
//! oxideav-aac is parser-only (not a decode peer). C peers (lavc / libfdk)
//! are not linked here; see BENCH.md.

use std::hint::black_box;
use std::io::Cursor;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
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

fn syom_ok(bytes: &[u8]) {
    let _ = black_box(decode(bytes));
}

fn rusty_adts(bytes: &[u8]) {
    let mut dec = AacDecoder::new();
    let mut pos = 0usize;
    while pos + 7 <= bytes.len() {
        let Ok(hdr) = rusty_aac::parse_adts(&bytes[pos..]) else {
            break;
        };
        let end = (pos + hdr.frame_length).min(bytes.len());
        let _ = dec.decode(&bytes[pos..end], None);
        if hdr.frame_length == 0 {
            break;
        }
        pos = end;
    }
}

fn symphonia_all(bytes: &[u8], ext: &str) {
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
        return;
    };
    let mut format = probed.format;
    let Some(track) = format.default_track() else {
        return;
    };
    let Ok(mut decoder) =
        symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default())
    else {
        return;
    };
    while let Ok(pkt) = format.next_packet() {
        let _ = decoder.decode(&pkt);
    }
}

fn group(c: &mut Criterion, name: &str, bytes: &[u8], ext: &str, rusty: bool) {
    let mut g = c.benchmark_group(name);
    g.throughput(Throughput::Bytes(bytes.len() as u64));
    g.bench_function("syom", |b| b.iter(|| syom_ok(bytes)));
    if rusty {
        g.bench_function("rusty_aac", |b| b.iter(|| rusty_adts(bytes)));
    }
    g.bench_function("symphonia", |b| b.iter(|| symphonia_all(bytes, ext)));
    g.finish();
}

fn benches(c: &mut Criterion) {
    group(c, "lc_adts", LC_ADTS, "aac", true);
    group(c, "lc_m4a", LC_M4A, "m4a", false);
    group(c, "he_adts", HE_ADTS, "aac", true);
    group(c, "he_m4a", HE_M4A, "m4a", false);
}

criterion_group!(benches_main, benches);
criterion_main!(benches_main);
