//! Offline qualification of a caller-supplied recording; media stays outside git.
use serde_json::json;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::time::Instant;
use syom::{DecodeOptions, Decoder, M4aSeek};

fn quantile(v: &[f64], q: f64) -> f64 {
    let mut v = v.to_vec();
    v.sort_by(f64::total_cmp);
    v[((v.len() - 1) as f64 * q).ceil() as usize]
}

fn timing(v: &[f64]) -> serde_json::Value {
    json!({"reps":v.len(), "median_ms":quantile(v,0.5),
        "p95_ms":quantile(v,0.95), "p99_ms":quantile(v,0.99), "raw_ms":v})
}

fn rss() -> String {
    std::fs::read_to_string("/proc/self/status")
        .unwrap_or_default()
        .lines()
        .filter(|s| s.starts_with("VmRSS:") || s.starts_with("VmHWM:"))
        .collect::<Vec<_>>()
        .join("; ")
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let input = args.get(1).ok_or("natural INPUT OUTDIR [REPS]")?;
    let out = std::path::Path::new(args.get(2).ok_or("missing OUTDIR")?);
    let reps = args
        .get(3)
        .map(|s| s.parse::<usize>())
        .transpose()?
        .unwrap_or(101);
    if reps == 0 {
        return Err("REPS must be positive".into());
    }
    std::fs::create_dir_all(out)?;
    let bytes = std::fs::read(input)?;
    let baseline_rss = rss();
    let options = DecodeOptions::audio();
    let start = Instant::now();
    let pcm = syom::decode_with(&bytes, &options)?;
    let cold_ms = start.elapsed().as_secs_f64() * 1000.0;
    let n = pcm.channels[0].len();
    let ch = pcm.channels.len();
    assert!(
        n > 0
            && pcm
                .channels
                .iter()
                .all(|p| p.len() == n && p.iter().all(|x| x.is_finite()))
    );
    let mut writer = BufWriter::new(File::create(out.join("syom.f32"))?);
    let mut block = Vec::with_capacity(65536);
    for at in (0..n).step_by(4096) {
        block.clear();
        for i in at..(at + 4096).min(n) {
            for p in &pcm.channels {
                block.extend_from_slice(&p[i].to_le_bytes());
            }
        }
        writer.write_all(&block)?;
    }
    writer.flush()?;
    drop(writer);

    // Validate seekable M4A/fMP4 or push input against the full decode.
    let mut seek_rows = Vec::new();
    let mut frame_times = Vec::with_capacity(n / 480 + 2);
    let mut first_ms = None;
    let mut consumed = 0usize;
    if syom::sniff_is_isobmff(&bytes) {
        let mut seek = M4aSeek::open(File::open(input)?, options.clone())?;
        let start = Instant::now();
        let mut previous = start;
        seek.decode(|f| {
            let now = Instant::now();
            if first_ms.is_none() {
                first_ms = Some(start.elapsed().as_secs_f64() * 1000.0);
            }
            frame_times.push(now.duration_since(previous).as_secs_f64() * 1000.0);
            previous = now;
            consumed += f.samples;
            Ok(())
        })?;
        assert_eq!(consumed, n);
        for at in [
            0,
            1,
            (pcm.sample_rate as usize * 37 + 123).min(n - 1),
            n / 2,
            n.saturating_sub(2048),
        ] {
            let start = Instant::now();
            seek.seek(at as i64)?;
            let mut position = at;
            let mut first = None;
            let mut mismatches = 0usize;
            let mut first_bad = None;
            let mut max_abs = 0.0f32;
            seek.decode(|f| {
                if first.is_none() {
                    first = Some(start.elapsed().as_secs_f64() * 1000.0);
                }
                for (got, want) in f.planar.iter().zip(&pcm.channels) {
                    for (j, (&a, &b)) in got
                        .iter()
                        .zip(&want[position..position + f.samples])
                        .enumerate()
                    {
                        if a != b {
                            mismatches += 1;
                            if first_bad.is_none() {
                                first_bad = Some(position + j);
                            }
                        }
                        max_abs = max_abs.max((a - b).abs());
                    }
                }
                position += f.samples;
                Ok(())
            })?;
            assert_eq!(position, n);
            seek_rows.push(json!({"at_sample":at,"first_output_ms":first,"exact_remaining_pcm":mismatches==0,"mismatches":mismatches,"first_bad_sample":first_bad,"max_abs_s16_lsb":max_abs*32768.0}));
        }
    } else {
        let mut dec = Decoder::new(options.clone());
        let start = Instant::now();
        let mut previous = start;
        let mut collect = |f: syom::Frame<'_>| {
            let now = Instant::now();
            if first_ms.is_none() {
                first_ms = Some(start.elapsed().as_secs_f64() * 1000.0);
            }
            frame_times.push(now.duration_since(previous).as_secs_f64() * 1000.0);
            previous = now;
            for (got, want) in f.planar.iter().zip(&pcm.channels) {
                assert_eq!(*got, &want[consumed..consumed + f.samples]);
            }
            consumed += f.samples;
            Ok(())
        };
        for part in bytes.chunks(1319) {
            dec.feed(part, &mut collect)?;
        }
        dec.finish(&mut collect)?;
        assert_eq!(consumed, n);
    }
    let mut cuts = Vec::new();
    for missing in [1, 64, bytes.len() / 2] {
        let end = bytes.len().saturating_sub(missing);
        cuts.push(match syom::decode_with(&bytes[..end], &options) {
            Ok(d) => json!({"missing_bytes":missing,"accepted":true,"samples":d.channels[0].len()}),
            Err(e) => json!({"missing_bytes":missing,"accepted":false,"error":e.to_string()}),
        });
    }
    let meta = json!({"engine":"syom", "harness_version":env!("CARGO_PKG_VERSION"),"input_bytes":bytes.len(),
        "rate":pcm.sample_rate,"channels":ch,"samples":n,"priming":pcm.priming,
        "duration_secs":n as f64/pcm.sample_rate as f64,"cold_decode_ms":cold_ms,
        "baseline_process_memory":baseline_rss,"peak_with_reference_pcm":rss(),
        "first_stream_output_ms":first_ms,"frame_interarrival_ms":timing(&frame_times),
        "seek":seek_rows,"truncation":cuts});
    drop(pcm);
    // Timed calls do not retain a reference PCM or write decoded output.
    let mut timings = Vec::with_capacity(reps);
    for i in 0..reps + 4 {
        let start = Instant::now();
        let result =
            std::hint::black_box(syom::decode_with(std::hint::black_box(&bytes), &options)?);
        assert_eq!(result.channels[0].len(), n);
        drop(result);
        if i >= 4 {
            timings.push(start.elapsed().as_secs_f64() * 1000.0);
        }
    }
    let result = json!({"validation":meta,"one_shot_decode_ms":timing(&timings),
        "process_memory_after_bench":rss()});
    std::fs::write(out.join("syom.json"), serde_json::to_vec_pretty(&result)?)?;
    println!(
        "{}",
        json!({"samples":n,"channels":ch,"median_ms":quantile(&timings,0.5)})
    );
    Ok(())
}
