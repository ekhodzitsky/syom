use std::{fs, hint::black_box, time::Instant};
use syom::{ChannelMode, DecodeOptions, Frame};
fn append(dst: &mut Vec<f32>, f: &Frame<'_>) {
    let Some(first) = f.planar.first() else {
        return;
    };
    let at = dst.len();
    dst.extend_from_slice(first);
    for ch in f.planar.iter().skip(1) {
        for (x, y) in dst[at..].iter_mut().zip(ch.iter()) {
            *x += y;
        }
    }
    let n = f.planar.len() as f32;
    for x in &mut dst[at..] {
        *x /= n;
    }
}
fn decode(data: &[u8], mode: &str, collect: bool) -> Vec<f32> {
    let mut opts = DecodeOptions::unbounded();
    if mode == "mono" {
        opts.channel_mode = ChannelMode::Mono;
    }
    let mut out = Vec::with_capacity(24576);
    let mut count = 0usize;
    let info = syom::decode_streaming(data, &opts, |f| {
        count += f.samples;
        if mode == "splitmix" {
            append(&mut out, &f);
        } else if mode == "mono" {
            out.extend_from_slice(f.planar[0]);
        } else {
            black_box(f.planar);
        }
        if !collect && out.len() >= 24576 {
            black_box(&out);
            out.clear();
        }
        Ok(())
    })
    .expect("decode");
    black_box((count, info));
    black_box(&out);
    out
}
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = &args[2];
    let data = fs::read(path).unwrap();
    if args[1] == "verify" {
        let a = decode(&data, "splitmix", true);
        let b = decode(&data, "mono", true);
        let count = a
            .iter()
            .zip(&b)
            .filter(|(x, y)| x.to_bits() != y.to_bits())
            .count();
        let max = a
            .iter()
            .zip(&b)
            .map(|(x, y)| (*x - *y).abs())
            .fold(0f32, f32::max);
        println!("{{\"file\":\"{path}\",\"manual_samples\":{},\"mono_samples\":{},\"differing_bits\":{count},\"max_abs\":{max:e}}}",a.len(),b.len());
        return;
    }
    let mode = &args[1];
    let warm: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(2);
    let reps: usize = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(7);
    let mut times = Vec::new();
    for i in 0..warm + reps {
        let start = Instant::now();
        black_box(decode(&data, mode, false));
        let ms = start.elapsed().as_secs_f64() * 1e3;
        if i >= warm {
            times.push(ms);
        }
    }
    let raw = times
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",");
    times.sort_by(f64::total_cmp);
    println!(
        "{{\"file\":\"{path}\",\"mode\":\"{mode}\",\"median_ms\":{},\"samples_ms\":[{raw}]}}",
        times[times.len() / 2]
    );
}
