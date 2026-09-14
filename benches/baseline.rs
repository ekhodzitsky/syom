//! Matched-output decode/encode baseline (TASK-15). Not Criterion.
//!
//! Every timed cell first passes [`syom::decode_cmp::run_preflight`] (or
//! encode_cmp). 4 warmup + 20 measured reps. Native lavc/FDK are not linked.

use std::hint::black_box;
use std::time::Instant;

use syom::decode_cmp::{
    Candidate, Lane, run_preflight, syom_candidate, syom_collect, syom_discard,
};

// Types `mod peers` imports via `use super::{...}`.
#[allow(unused_imports)]
use syom::decode_cmp::{Collect, Pcm};
use syom::encode_cmp::{run_encode_preflight, syom_encode_candidate};
use syom::{DecodeOptions, Decoder, decode, encode};

#[allow(dead_code)]
mod peers;

const WARM: usize = 4;
const REPS: usize = 20;
const LC_ADTS: &[u8] = include_bytes!("../src/goldens/sine48.adts");
const LC_M4A: &[u8] = include_bytes!("../src/goldens/sine441.m4a");
const HE_ADTS: &[u8] = include_bytes!("../src/goldens/he48.adts");
const HE_M4A: &[u8] = include_bytes!("../src/goldens/he48.m4a");
const PS_ADTS: &[u8] = include_bytes!("../src/goldens/ps48.adts");
const MC_ADTS: &[u8] = include_bytes!("../src/goldens/mc51.adts");
const LECTURE: &[u8] = include_bytes!("../src/goldens/lecture.m4a");

fn adts() -> [Candidate; 4] {
    [
        syom_candidate(),
        peers::rusty_candidate(),
        peers::oxideav_candidate(),
        peers::symphonia_adts_candidate(),
    ]
}

fn m4a() -> [Candidate; 4] {
    [
        syom_candidate(),
        peers::rusty_candidate(),
        peers::oxideav_candidate(),
        peers::symphonia_m4a_candidate(),
    ]
}

fn mc() -> [Candidate; 3] {
    [
        syom_candidate(),
        peers::oxideav_candidate(),
        peers::symphonia_adts_candidate(),
    ]
}

fn median(sorted: &[f64]) -> f64 {
    let n = sorted.len();
    if n == 0 {
        return f64::NAN;
    }
    if n % 2 == 1 {
        sorted[n / 2]
    } else {
        0.5 * (sorted[n / 2 - 1] + sorted[n / 2])
    }
}

fn p95(sorted: &[f64]) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    let i = ((sorted.len() as f64 - 1.0) * 0.95).round() as usize;
    sorted[i.min(sorted.len() - 1)]
}

fn bootstrap_median_ci(samples: &[f64]) -> (f64, f64) {
    if samples.is_empty() {
        return (f64::NAN, f64::NAN);
    }
    let mut rng = 0x9e37_79b9u64;
    let mut meds = [0.0f64; 1000];
    let mut buf = vec![0.0; samples.len()];
    for m in &mut meds {
        for slot in &mut buf {
            rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
            let i = (rng as usize) % samples.len();
            *slot = samples[i];
        }
        buf.sort_by(|a, b| a.total_cmp(b));
        *m = median(&buf);
    }
    meds.sort_by(|a, b| a.total_cmp(b));
    (meds[25], meds[974])
}

fn time_ns(mut f: impl FnMut()) -> Vec<f64> {
    for _ in 0..WARM {
        f();
    }
    let mut out = Vec::with_capacity(REPS);
    for _ in 0..REPS {
        let t0 = Instant::now();
        f();
        out.push(t0.elapsed().as_secs_f64() * 1e9);
    }
    out
}

fn summarize(raw: &[f64]) -> (f64, f64, f64, f64) {
    let mut s = raw.to_vec();
    s.sort_by(|a, b| a.total_cmp(b));
    let (lo, hi) = bootstrap_median_ci(&s);
    (median(&s), p95(&s), lo, hi)
}

fn print_cell(group: &str, id: &str, raw: &[f64]) {
    let (med, p95v, lo, hi) = summarize(raw);
    print!("| {group} | {id} | {med:.0} | {p95v:.0} | {lo:.0}–{hi:.0} |");
    for x in raw {
        print!(" {x:.0}");
    }
    println!(" |");
}

fn group_decode(name: &'static str, bytes: &'static [u8], cands: &[Candidate], lane: Lane) {
    let pf = run_preflight(name, bytes, lane, cands);
    eprint!("{}", pf.report());
    let Some(timed) = pf.timed_ids() else {
        println!("| {name} | *(aborted)* | | | | |");
        return;
    };
    for cand in cands {
        if !timed.contains(&cand.id) {
            continue;
        }
        let collect = cand.collect;
        let raw = time_ns(|| {
            black_box(collect(bytes, lane));
        });
        print_cell(name, cand.id, &raw);
    }
}

fn adts_len(b: &[u8], i: usize) -> Option<usize> {
    if i + 7 > b.len() || b[i] != 0xff || b[i + 1] & 0xf0 != 0xf0 {
        return None;
    }
    let fl =
        ((b[i + 3] as usize & 3) << 11) | ((b[i + 4] as usize) << 3) | ((b[i + 5] as usize) >> 5);
    if fl < 7 { None } else { Some(fl) }
}

struct Latency {
    start: Vec<f64>,
    first: Vec<f64>,
    steady: Vec<f64>,
    finish: Vec<f64>,
}

fn latency_ns(bytes: &[u8]) -> Option<Latency> {
    let mut start = Vec::new();
    let mut first = Vec::new();
    let mut steady = Vec::new();
    let mut finish = Vec::new();
    for iter in 0..(WARM + REPS) {
        let t_new = Instant::now();
        let mut dec = Decoder::new(DecodeOptions::unbounded());
        let new_ns = t_new.elapsed().as_secs_f64() * 1e9;
        let mut i = 0usize;
        let mut first_ns = 0.0;
        let mut saw = false;
        let t_first = Instant::now();
        while i < bytes.len() && !saw {
            let n = adts_len(bytes, i).unwrap_or(1);
            let end = (i + n).min(bytes.len());
            let _ = dec.feed(&bytes[i..end], |_| {
                if !saw {
                    first_ns = t_first.elapsed().as_secs_f64() * 1e9;
                    saw = true;
                }
                Ok(())
            });
            i = end;
        }
        let t_st = Instant::now();
        while i < bytes.len() {
            let n = adts_len(bytes, i).unwrap_or(1);
            let end = (i + n).min(bytes.len());
            let _ = dec.feed(&bytes[i..end], |_| Ok(()));
            i = end;
        }
        let st_ns = t_st.elapsed().as_secs_f64() * 1e9;
        let t_fin = Instant::now();
        let _ = dec.finish(|_| Ok(()));
        let fin_ns = t_fin.elapsed().as_secs_f64() * 1e9;
        if iter >= WARM {
            start.push(new_ns);
            first.push(first_ns);
            steady.push(st_ns);
            finish.push(fin_ns);
        }
    }
    Some(Latency {
        start,
        first,
        steady,
        finish,
    })
}

fn enc_input() -> (Vec<Vec<f32>>, u32) {
    match decode(LC_ADTS) {
        Ok(d) => (d.channels, d.sample_rate),
        Err(_) => (vec![vec![0.0; 1024]], 48_000),
    }
}

fn loop_one(name: &str) {
    let (fixture, bytes): (&'static str, &[u8]) = match name {
        "he_adts" => ("he_adts", HE_ADTS),
        "ps_adts" => ("ps_adts", PS_ADTS),
        "mc_adts" => ("mc_adts", MC_ADTS),
        _ => ("lc_adts", LC_ADTS),
    };
    let pf = run_preflight(fixture, bytes, Lane::PlanarSplit, &[syom_candidate()]);
    if pf.timed_ids().is_none() {
        eprintln!("preflight abort\n{}", pf.report());
        return;
    }
    for _ in 0..400 {
        black_box(syom_collect(bytes, Lane::PlanarSplit));
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--loop") {
        loop_one(args.get(i + 1).map(String::as_str).unwrap_or("lc_adts"));
        return;
    }

    println!("# TASK-15 matched-output baseline");
    println!();
    println!("host=Linux x86_64; cpu=AMD Ryzen AI 9 HX 370; rustc=1.97.1;");
    println!("build=profile.bench thin-LTO cg=1; threads=1; governor=performance;");
    println!("reps={REPS} after {WARM} warmup; times=nanoseconds wall;");
    println!("preflight=syom::decode_cmp::run_preflight; lane=planar_split unless named.");
    println!();
    println!("| group | peer | median_ns | p95_ns | bootstrap95_median | raw_ns |");
    println!("|---|---|---|---|---|---|");

    let a = adts();
    let m = m4a();
    group_decode("lc_adts", LC_ADTS, &a, Lane::PlanarSplit);
    group_decode("lc_m4a", LC_M4A, &m, Lane::PlanarSplit);
    group_decode("he_adts", HE_ADTS, &a, Lane::PlanarSplit);
    group_decode("he_m4a", HE_M4A, &m, Lane::PlanarSplit);
    group_decode("ps_adts", PS_ADTS, &a, Lane::PlanarSplit);
    group_decode("mc_adts", MC_ADTS, &mc(), Lane::PlanarSplit);
    group_decode(
        "lc_adts_speech",
        LC_ADTS,
        &[syom_candidate()],
        Lane::SpeechDownmix,
    );
    group_decode("lecture_m4a", LECTURE, &m, Lane::PlanarSplit);

    let discard = adts();
    let pf = run_preflight("lc_adts_discard", LC_ADTS, Lane::DiscardOutput, &discard);
    eprint!("{}", pf.report());
    if let Some(timed) = pf.timed_ids()
        && timed.contains(&"syom")
    {
        let raw = time_ns(|| {
            let _ = black_box(syom_discard(LC_ADTS));
        });
        print_cell("lc_adts_discard", "syom", &raw);
    }

    let (pcm, rate) = enc_input();
    let enc_cands = [syom_encode_candidate()];
    let ep = run_encode_preflight("enc_lc_mono", &pcm, rate, 128_000, &enc_cands);
    eprint!("{}", ep.report());
    if ep.timed_ids().is_some() {
        let raw = time_ns(|| {
            let _ = black_box(encode(&pcm, rate));
        });
        print_cell("enc_lc_mono", "syom", &raw);
    }
    let l = pcm.first().cloned().unwrap_or_default();
    let r: Vec<f32> = l.iter().map(|&x| x * 0.8).collect();
    let stereo = vec![l, r];
    let ep2 = run_encode_preflight("enc_lc_stereo", &stereo, rate, 128_000, &enc_cands);
    eprint!("{}", ep2.report());
    if ep2.timed_ids().is_some() {
        let raw = time_ns(|| {
            let _ = black_box(encode(&stereo, rate));
        });
        print_cell("enc_lc_stereo", "syom", &raw);
    }

    println!();
    println!("## latency (syom ADTS push, nanoseconds)");
    println!("| phase | median_ns | p95_ns | bootstrap95_median |");
    println!("|---|---|---|---|");
    if let Some(lat) = latency_ns(LC_ADTS) {
        for (name, raw) in [
            ("startup Decoder::new", &lat.start),
            ("first_output", &lat.first),
            ("steady remaining feeds", &lat.steady),
            ("finish", &lat.finish),
        ] {
            let (med, p95v, lo, hi) = summarize(raw);
            println!("| {name} | {med:.0} | {p95v:.0} | {lo:.0}–{hi:.0} |");
        }
    }
}
