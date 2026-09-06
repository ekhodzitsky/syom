//! In-process AAC decode vs Rust peers (bytes → PCM).
//!
//! oxideav-aac 0.1.7 decodes ADTS (LC + SBR + PS) but has no ISOBMFF
//! demux, so it joins the ADTS groups only. C peers (lavc / libfdk) are
//! not linked here; see BENCH.md.

use std::hint::black_box;
use std::io::Cursor;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
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

fn oxideav_adts(bytes: &[u8]) {
    let mut dec = StreamDecoder::new();
    let _ = black_box(dec.decode_all(bytes));
}

fn symphonia_all(bytes: &[u8], ext: &str) {
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
        return;
    };
    let Some(track) = format.default_track(TrackType::Audio) else {
        return;
    };
    let Some(CodecParameters::Audio(params)) = track.codec_params.clone() else {
        return;
    };
    let Ok(mut decoder) = symphonia::default::get_codecs()
        .make_audio_decoder(&params, &AudioDecoderOptions::default())
    else {
        return;
    };
    while let Ok(Some(pkt)) = format.next_packet() {
        let _ = decoder.decode(&pkt);
    }
}

fn group(c: &mut Criterion, name: &str, bytes: &[u8], ext: &str, rusty: bool, oxideav: bool) {
    let mut g = c.benchmark_group(name);
    g.throughput(Throughput::Bytes(bytes.len() as u64));
    g.bench_function("syom", |b| b.iter(|| syom_ok(bytes)));
    if rusty {
        g.bench_function("rusty_aac", |b| b.iter(|| rusty_adts(bytes)));
    }
    if oxideav {
        g.bench_function("oxideav", |b| b.iter(|| oxideav_adts(bytes)));
    }
    g.bench_function("symphonia", |b| b.iter(|| symphonia_all(bytes, ext)));
    g.finish();
}

/// Mono encode input: decode the committed LC sine golden.
fn enc_mono_input() -> (Vec<Vec<f32>>, u32) {
    let fallback = (vec![vec![0.0; 1024]], 48_000);
    let Ok(dec) = decode(LC_ADTS) else {
        return fallback;
    };
    (dec.channels, dec.sample_rate)
}

/// Stereo encode input: the mono golden plus a deterministic 0.8× shadow
/// (correlated enough to exercise M/S).
fn enc_stereo_input() -> (Vec<Vec<f32>>, u32) {
    let (ch, rate) = enc_mono_input();
    let l = ch.into_iter().next().unwrap_or_default();
    let r = l.iter().map(|&x| x * 0.8).collect();
    (vec![l, r], rate)
}

/// rusty_aac 0.5 encode → ADTS-wrapped bytes (parity with syom::encode).
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

fn syom_encode(pcm: &[Vec<f32>], rate: u32) {
    let _ = black_box(encode(pcm, rate));
}

fn enc_group(c: &mut Criterion, name: &str, pcm: &[Vec<f32>], rate: u32) {
    let mut g = c.benchmark_group(name);
    let bytes = (pcm.len() * pcm.first().map_or(0, Vec::len) * 4) as u64;
    g.throughput(Throughput::Bytes(bytes));
    g.bench_function("syom", |b| b.iter(|| syom_encode(pcm, rate)));
    g.bench_function("rusty_aac", |b| b.iter(|| rusty_encode(pcm, rate)));
    g.finish();
}

fn benches(c: &mut Criterion) {
    group(c, "lc_adts", LC_ADTS, "aac", true, true);
    group(c, "lc_m4a", LC_M4A, "m4a", false, false);
    group(c, "he_adts", HE_ADTS, "aac", true, true);
    group(c, "he_m4a", HE_M4A, "m4a", false, false);
    group(c, "ps_adts", PS_ADTS, "aac", true, true);
    // 5.1 LC: rusty_aac decodes it but at ~seconds/iter it is not a
    // meaningful wall peer; sample parity is checked in mem.rs.
    group(c, "mc_adts", MC_ADTS, "aac", false, true);
    let (mono, mono_rate) = enc_mono_input();
    enc_group(c, "enc_lc_mono", &mono, mono_rate);
    let (stereo, stereo_rate) = enc_stereo_input();
    enc_group(c, "enc_lc_stereo", &stereo, stereo_rate);
}

criterion_group!(benches_main, benches);
criterion_main!(benches_main);
