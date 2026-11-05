use std::{fs, hint::black_box, time::Instant};
#[allow(dead_code)]
mod resample_original;
use resample_original::{ResampleTo16k, SampleRate};
fn native(file: &str) -> (u32, Vec<f32>) {
    let bytes = fs::read(file).unwrap();
    let mut out = Vec::new();
    let opts = syom::DecodeOptions::unbounded();
    let info = syom::decode_streaming(&bytes, &opts, |f| {
        let at = out.len();
        out.extend_from_slice(f.planar[0]);
        for ch in f.planar.iter().skip(1) {
            for (x, y) in out[at..].iter_mut().zip(ch.iter()) {
                *x += y;
            }
        }
        let n = f.planar.len() as f32;
        for x in &mut out[at..] {
            *x /= n;
        }
        Ok(())
    })
    .unwrap();
    (info.sample_rate, out)
}
fn convert(pcm: &[f32], rate: u32, chunk: usize) -> Vec<f32> {
    let mut r = ResampleTo16k::new(SampleRate(rate), None);
    for p in pcm.chunks(chunk) {
        r.stage().extend_from_slice(p);
        r.flush_full().unwrap();
    }
    r.finish().unwrap()
}
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (rate, pcm) = native(&args[2]);
    if args[1] == "verify" {
        let base = convert(&pcm, rate, 1024);
        for chunk in [24576, 48000, 49152] {
            let other = convert(&pcm, rate, chunk);
            let count = base
                .iter()
                .zip(&other)
                .filter(|(x, y)| x.to_bits() != y.to_bits())
                .count();
            let max = base
                .iter()
                .zip(&other)
                .map(|(x, y)| (*x - *y).abs())
                .fold(0f32, f32::max);
            println!("{{\"chunk\":{chunk},\"rate\":{rate},\"native_samples\":{},\"baseline_samples\":{},\"candidate_samples\":{},\"differing_bits\":{count},\"max_abs\":{max:e}}}",pcm.len(),base.len(),other.len());
        }
        return;
    }
    let chunk = args[1].parse().unwrap();
    let mut times = Vec::new();
    for i in 0..9 {
        let start = Instant::now();
        black_box(convert(&pcm, rate, chunk));
        if i >= 2 {
            times.push(start.elapsed().as_secs_f64() * 1e3);
        }
    }
    let raw = times
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",");
    times.sort_by(f64::total_cmp);
    println!("{{\"chunk\":{chunk},\"rate\":{rate},\"native_samples\":{},\"median_ms\":{},\"samples_ms\":[{raw}]}}",pcm.len(),times[times.len()/2]);
}
