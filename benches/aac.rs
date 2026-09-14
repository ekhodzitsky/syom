//! In-process AAC decode vs Rust peers (bytes → planar PCM).
//!
//! Every timed candidate first passes [`syom::decode_cmp::run_preflight`].
//! Failed collects abort the group; HE/core-only and mismatched lengths
//! are non-comparable (no throughput). Primary lane is planar split at
//! native rate. C peers (lavc / libfdk) are not linked here; see BENCH.md.

use std::hint::black_box;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use syom::decode_cmp::{Candidate, Lane, run_preflight, syom_candidate, syom_discard};
use syom::{decode, encode};

// Types `mod peers` imports via `use super::{...}`.
#[allow(unused_imports)]
use syom::decode_cmp::{Collect, Pcm};

mod peers;

const LC_ADTS: &[u8] = include_bytes!("../src/goldens/sine48.adts");
const LC_M4A: &[u8] = include_bytes!("../src/goldens/sine441.m4a");
const HE_ADTS: &[u8] = include_bytes!("../src/goldens/he48.adts");
const HE_M4A: &[u8] = include_bytes!("../src/goldens/he48.m4a");
const PS_ADTS: &[u8] = include_bytes!("../src/goldens/ps48.adts");
const MC_ADTS: &[u8] = include_bytes!("../src/goldens/mc51.adts");

fn adts_candidates() -> [Candidate; 4] {
    [
        syom_candidate(),
        peers::rusty_candidate(),
        peers::oxideav_candidate(),
        peers::symphonia_adts_candidate(),
    ]
}

fn m4a_candidates() -> [Candidate; 4] {
    [
        syom_candidate(),
        peers::rusty_candidate(),
        peers::oxideav_candidate(),
        peers::symphonia_m4a_candidate(),
    ]
}

/// 5.1: rusty_aac matches split PCM but is ~1 s/iter — not a wall peer.
fn mc_candidates() -> [Candidate; 3] {
    [
        syom_candidate(),
        peers::oxideav_candidate(),
        peers::symphonia_adts_candidate(),
    ]
}

fn timed_collect(c: &mut Criterion, name: &'static str, bytes: &'static [u8], cands: &[Candidate]) {
    let pf = run_preflight(name, bytes, Lane::PlanarSplit, cands);
    eprint!("{}", pf.report());
    let Some(timed) = pf.timed_ids() else {
        panic!("preflight abort\n{}", pf.report());
    };
    let mut g = c.benchmark_group(name);
    g.throughput(Throughput::Bytes(bytes.len() as u64));
    for cand in cands {
        if !timed.contains(&cand.id) {
            continue;
        }
        let collect = cand.collect;
        g.bench_function(cand.id, move |b| {
            b.iter(|| black_box(collect(bytes, Lane::PlanarSplit)))
        });
    }
    g.finish();
}

fn timed_speech(c: &mut Criterion, bytes: &'static [u8]) {
    let cands = [syom_candidate()];
    let pf = run_preflight("lc_adts_speech", bytes, Lane::SpeechDownmix, &cands);
    eprint!("{}", pf.report());
    if pf.timed_ids().is_none() {
        panic!("preflight abort\n{}", pf.report());
    }
    let mut g = c.benchmark_group("lc_adts_speech");
    g.throughput(Throughput::Bytes(bytes.len() as u64));
    g.bench_function("syom", |b| {
        b.iter(|| black_box(syom::decode_cmp::syom_collect(bytes, Lane::SpeechDownmix)))
    });
    g.finish();
}

fn timed_discard(c: &mut Criterion, bytes: &'static [u8]) {
    let cands = adts_candidates();
    let pf = run_preflight("lc_adts_discard", bytes, Lane::DiscardOutput, &cands);
    eprint!("{}", pf.report());
    let Some(timed) = pf.timed_ids() else {
        panic!("preflight abort\n{}", pf.report());
    };
    let mut g = c.benchmark_group("lc_adts_discard");
    g.throughput(Throughput::Bytes(bytes.len() as u64));
    if timed.contains(&"syom") {
        g.bench_function("syom", |b| b.iter(|| black_box(syom_discard(bytes))));
    }
    if timed.contains(&"rusty_aac") {
        g.bench_function("rusty_aac", |b| {
            b.iter(|| black_box(peers::rusty_discard(bytes)))
        });
    }
    if timed.contains(&"oxideav-aac") {
        g.bench_function("oxideav-aac", |b| {
            b.iter(|| black_box(peers::oxideav_discard(bytes)))
        });
    }
    if timed.contains(&"symphonia") {
        g.bench_function("symphonia", |b| {
            b.iter(|| black_box(peers::symphonia_adts_discard(bytes)))
        });
    }
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
    let adts = adts_candidates();
    let m4a = m4a_candidates();
    timed_collect(c, "lc_adts", LC_ADTS, &adts);
    timed_collect(c, "lc_m4a", LC_M4A, &m4a);
    timed_collect(c, "he_adts", HE_ADTS, &adts);
    timed_collect(c, "he_m4a", HE_M4A, &m4a);
    timed_collect(c, "ps_adts", PS_ADTS, &adts);
    timed_collect(c, "mc_adts", MC_ADTS, &mc_candidates());
    timed_speech(c, LC_ADTS);
    timed_discard(c, LC_ADTS);
    let (mono, mono_rate) = enc_mono_input();
    enc_group(c, "enc_lc_mono", &mono, mono_rate);
    let (stereo, stereo_rate) = enc_stereo_input();
    enc_group(c, "enc_lc_stereo", &stereo, stereo_rate);
}

criterion_group!(benches_main, benches);
criterion_main!(benches_main);
