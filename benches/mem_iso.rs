//! One OS process per peer/case (TASK-12). Historical `mem.rs` is in-process.
//!
//! ```sh
//! cargo bench --bench mem_iso -- --quick
//! # or the binary:
//! target/release/deps/mem_iso-* --child --peer syom --case lc_adts --phase one_shot
//! ```
//!
//! Non-syom rows are one-shot only. JSON is printed only when planar PCM is
//! 48 kHz and matches syom's channel count and sample count. Anything else is
//! a `# skip` line: a shorter buffer is not a smaller-memory win.

use std::alloc::{GlobalAlloc, Layout, System};
use std::env;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use syom::mem_iso::MemReport;
use syom::{DecodeOptions, Decoder, decode_with};

// Names `mod peers` imports via `super`.
#[allow(unused_imports)]
use syom::decode_cmp::{Candidate, Collect, Lane, Pcm};

#[allow(dead_code)]
#[path = "peers.rs"]
mod peers;

const LC_ADTS: &[u8] = include_bytes!("../src/goldens/sine48.adts");
const HE_ADTS: &[u8] = include_bytes!("../src/goldens/he48.adts");
const PS_ADTS: &[u8] = include_bytes!("../src/goldens/ps48.adts");
const MC_ADTS: &[u8] = include_bytes!("../src/goldens/mc51.adts");

struct Counter;
static ALLOCS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);
static LIVE: AtomicU64 = AtomicU64::new(0);
static PEAK: AtomicU64 = AtomicU64::new(0);

unsafe impl GlobalAlloc for Counter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let n = layout.size() as u64;
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(n, Ordering::Relaxed);
        let live = LIVE.fetch_add(n, Ordering::Relaxed) + n;
        PEAK.fetch_max(live, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size() as u64, Ordering::Relaxed);
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOC: Counter = Counter;

fn rss_peak() -> u64 {
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
    let rc = unsafe { getrusage(0, &mut u) };
    if rc != 0 {
        return 0;
    }
    #[cfg(target_os = "macos")]
    {
        u.ru_maxrss.max(0) as u64
    }
    #[cfg(not(target_os = "macos"))]
    {
        (u.ru_maxrss.max(0) as u64).saturating_mul(1024)
    }
}

fn fixture(case: &str) -> &'static [u8] {
    match case {
        "lc_adts" => LC_ADTS,
        "he_adts" => HE_ADTS,
        "ps_adts" => PS_ADTS,
        "mc_adts" => MC_ADTS,
        _ => LC_ADTS,
    }
}

fn arg_val(args: &[String], key: &str) -> String {
    args.windows(2)
        .find(|w| w[0] == key)
        .map(|w| w[1].clone())
        .unwrap_or_default()
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Shape {
    rate: u32,
    ch: usize,
    samples: usize,
}

fn planes_shape(rate: u32, planes: &[Vec<f32>]) -> Option<Shape> {
    if planes.is_empty() {
        return None;
    }
    let n = planes[0].len();
    if rate == 0 || n == 0 || planes.iter().any(|p| p.len() != n) {
        return None;
    }
    Some(Shape {
        rate,
        ch: planes.len(),
        samples: n,
    })
}

fn case_index(case: &str) -> Option<usize> {
    match case {
        "lc_adts" => Some(0),
        "he_adts" => Some(1),
        "ps_adts" => Some(2),
        "mc_adts" => Some(3),
        _ => None,
    }
}

fn expect_shape(args: &[String]) -> Option<Shape> {
    let rate: u32 = arg_val(args, "--rate").parse().ok()?;
    let ch: usize = arg_val(args, "--ch").parse().ok()?;
    let samples: usize = arg_val(args, "--samples").parse().ok()?;
    if rate == 0 || ch == 0 || samples == 0 {
        return None;
    }
    Some(Shape { rate, ch, samples })
}

fn parse_ref(line: &str) -> Option<(String, Shape)> {
    let rest = line.strip_prefix("# ref ")?;
    let mut case: Option<String> = None;
    let mut rate: Option<u32> = None;
    let mut ch: Option<usize> = None;
    let mut samples: Option<usize> = None;
    for tok in rest.split_whitespace() {
        if let Some(v) = tok.strip_prefix("case=") {
            case = Some(v.to_string());
        } else if let Some(v) = tok.strip_prefix("rate=") {
            rate = v.parse().ok();
        } else if let Some(v) = tok.strip_prefix("ch=") {
            ch = v.parse().ok();
        } else if let Some(v) = tok.strip_prefix("samples=") {
            samples = v.parse().ok();
        }
    }
    Some((
        case?,
        Shape {
            rate: rate?,
            ch: ch?,
            samples: samples?,
        },
    ))
}

#[derive(Clone, Copy)]
enum Gate {
    Match,
    Mismatch(Shape),
    Failed,
    Unavailable,
    Empty,
}

fn peer_gate(peer: &str, bytes: &[u8], expect: Shape) -> Gate {
    let collect = match peer {
        "symphonia" => peers::symphonia_adts_candidate().collect,
        "rusty_aac" => peers::rusty_candidate().collect,
        "oxideav-aac" => peers::oxideav_candidate().collect,
        _ => return Gate::Unavailable,
    };
    let got = collect(bytes, Lane::PlanarSplit);
    let gate = match &got {
        Collect::Pcm(pcm) => match planes_shape(pcm.sample_rate, &pcm.planes) {
            Some(s) if s == expect => Gate::Match,
            Some(s) => Gate::Mismatch(s),
            None => Gate::Empty,
        },
        Collect::Failed(_) => Gate::Failed,
        Collect::Unavailable(_) => Gate::Unavailable,
    };
    drop(got);
    gate
}

struct Snap {
    peak_rss: u64,
    peak_live: u64,
    allocs: u64,
    alloc_bytes: u64,
    retained: u64,
}

/// Counter snapshot after the timed work. No heap traffic of its own.
fn snap(live0: u64, peak0: u64) -> Snap {
    let live = LIVE.load(Ordering::Relaxed).saturating_sub(live0);
    let peak = PEAK.load(Ordering::Relaxed).saturating_sub(peak0);
    Snap {
        peak_rss: rss_peak(),
        peak_live: peak.max(live),
        allocs: ALLOCS.load(Ordering::Relaxed),
        alloc_bytes: BYTES.load(Ordering::Relaxed),
        retained: LIVE.load(Ordering::Relaxed),
    }
}

fn report_from(peer: String, case: String, phase: String, baseline: u64, s: Snap) -> MemReport {
    MemReport {
        peer,
        case,
        phase,
        baseline_rss: baseline,
        peak_rss: s.peak_rss,
        peak_rss_delta: s.peak_rss.saturating_sub(baseline),
        peak_live: s.peak_live,
        allocs: s.allocs,
        alloc_bytes: s.alloc_bytes,
        retained: s.retained,
    }
}

fn child(args: &[String]) {
    let peer = arg_val(args, "--peer");
    let case = arg_val(args, "--case");
    let phase = arg_val(args, "--phase");
    let chunks: usize = arg_val(args, "--chunks").parse().unwrap_or(1);
    let bytes = fixture(&case);
    if peer != "syom" {
        peer_child(&peer, &case, &phase, bytes, args);
        return;
    }
    let baseline = rss_peak();
    ALLOCS.store(0, Ordering::Relaxed);
    BYTES.store(0, Ordering::Relaxed);
    // LIVE/PEAK keep process lifetime; snapshot before work.
    let live0 = LIVE.load(Ordering::Relaxed);
    let peak0 = PEAK.load(Ordering::Relaxed);
    let mut syom_ref = None;
    match phase.as_str() {
        "one_shot" => {
            let decoded = decode_with(bytes, &DecodeOptions::unbounded());
            if let Ok(d) = &decoded {
                syom_ref = planes_shape(d.sample_rate, &d.channels);
            }
            drop(decoded);
        }
        "stream" => {
            let mut dec = Decoder::new(DecodeOptions::unbounded());
            let n = chunks.max(1);
            let step = (bytes.len() / n).max(1);
            let mut i = 0;
            while i < bytes.len() {
                let end = (i + step).min(bytes.len());
                let _ = dec.feed(&bytes[i..end], |_| Ok(()));
                i = end;
            }
            let _ = dec.finish(|_| Ok(()));
        }
        _ => {}
    }
    let shot = phase == "one_shot";
    let s = snap(live0, peak0);
    let report = report_from(peer, case.clone(), phase, baseline, s);
    println!("{}", report.to_json());
    if shot && let Some(shape) = syom_ref {
        println!(
            "# ref case={case} rate={} ch={} samples={}",
            shape.rate, shape.ch, shape.samples
        );
    }
}

fn peer_child(peer: &str, case: &str, phase: &str, bytes: &[u8], args: &[String]) {
    if phase != "one_shot" {
        println!("# skip peer={peer} case={case} reason=phase {phase} is not one_shot");
        return;
    }
    let Some(expect) = expect_shape(args) else {
        println!("# skip peer={peer} case={case} reason=missing expect shape");
        return;
    };
    if expect.rate != 48_000 {
        println!(
            "# skip peer={peer} case={case} reason=syom rate {} is not 48000",
            expect.rate
        );
        return;
    }
    let baseline = rss_peak();
    ALLOCS.store(0, Ordering::Relaxed);
    BYTES.store(0, Ordering::Relaxed);
    let live0 = LIVE.load(Ordering::Relaxed);
    let peak0 = PEAK.load(Ordering::Relaxed);
    let gate = peer_gate(peer, bytes, expect);
    let s = snap(live0, peak0);
    match gate {
        Gate::Match => {
            let report = report_from(
                peer.to_string(),
                case.to_string(),
                phase.to_string(),
                baseline,
                s,
            );
            println!("{}", report.to_json());
        }
        Gate::Mismatch(shape) => println!(
            "# skip peer={peer} case={case} reason=shape {} Hz {} ch {} samples (syom {} Hz {} ch {} samples)",
            shape.rate, shape.ch, shape.samples, expect.rate, expect.ch, expect.samples
        ),
        Gate::Failed => println!("# skip peer={peer} case={case} reason=decode failed"),
        Gate::Unavailable => println!("# skip peer={peer} case={case} reason=decode unavailable"),
        Gate::Empty => println!("# skip peer={peer} case={case} reason=empty or uneven pcm"),
    }
}

fn child_lines(exe: &Path, label: &str, args: &[String]) -> Vec<String> {
    let out = Command::new(exe)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();
    match out {
        Ok(o) => {
            if !o.status.success() {
                eprintln!("child fail {label}: {}", String::from_utf8_lossy(&o.stderr));
            }
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter(|line| line.starts_with('{') || line.starts_with('#'))
                .map(str::to_string)
                .collect()
        }
        Err(e) => {
            eprintln!("spawn {label}: {e}");
            Vec::new()
        }
    }
}

fn syom_args(case: &str, phase: &str, chunks: &str) -> Vec<String> {
    vec![
        "--child".into(),
        "--peer".into(),
        "syom".into(),
        "--case".into(),
        case.into(),
        "--phase".into(),
        phase.into(),
        "--chunks".into(),
        chunks.into(),
    ]
}

fn peer_args(peer: &str, case: &str, shape: Shape) -> Vec<String> {
    vec![
        "--child".into(),
        "--peer".into(),
        peer.into(),
        "--case".into(),
        case.into(),
        "--phase".into(),
        "one_shot".into(),
        "--chunks".into(),
        "1".into(),
        "--rate".into(),
        shape.rate.to_string(),
        "--ch".into(),
        shape.ch.to_string(),
        "--samples".into(),
        shape.samples.to_string(),
    ]
}

fn parent() {
    let exe = match env::current_exe() {
        Ok(p) => p,
        Err(_) => {
            eprintln!("mem_iso: no current_exe");
            return;
        }
    };
    let cases = ["lc_adts", "he_adts", "ps_adts", "mc_adts"];
    let phases = [("one_shot", "1"), ("stream", "1"), ("stream", "8")];
    let mut jobs = Vec::new();
    for case in cases {
        for (phase, chunks) in phases {
            jobs.push((case, phase, chunks));
        }
    }
    let mut reverse = jobs.clone();
    reverse.reverse();
    // symphonia: every ADTS case, skipped unless the shape matches.
    // rusty / oxideav: lc_adts one-shot only (one process per order, no loop).
    let peers: &[(&str, &[&str])] = &[
        ("symphonia", &["lc_adts", "he_adts", "ps_adts", "mc_adts"]),
        ("rusty_aac", &["lc_adts"]),
        ("oxideav-aac", &["lc_adts"]),
    ];
    for (label, list) in [
        ("forward", jobs.as_slice()),
        ("reverse", reverse.as_slice()),
    ] {
        println!("# order={label}");
        let mut refs: [Option<Shape>; 4] = [None, None, None, None];
        for (case, phase, chunks) in list {
            let args = syom_args(case, phase, chunks);
            for line in child_lines(&exe, &format!("syom {case} {phase}"), &args) {
                if let Some((name, shape)) = parse_ref(&line)
                    && let Some(i) = case_index(&name)
                {
                    refs[i] = Some(shape);
                }
                println!("{line}");
            }
        }
        for (peer, peer_cases) in peers {
            for case in *peer_cases {
                let Some(i) = case_index(case) else {
                    continue;
                };
                let Some(shape) = refs[i] else {
                    println!("# skip peer={peer} case={case} reason=no syom shape");
                    continue;
                };
                if shape.rate != 48_000 {
                    println!(
                        "# skip peer={peer} case={case} reason=syom rate {} is not 48000",
                        shape.rate
                    );
                    continue;
                }
                let args = peer_args(peer, case, shape);
                let tag = format!("{peer} {case} one_shot");
                for line in child_lines(&exe, &tag, &args) {
                    println!("{line}");
                }
            }
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.iter().any(|a| a == "--child") {
        child(&args);
    } else {
        parent();
    }
}
