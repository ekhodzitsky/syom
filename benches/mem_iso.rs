//! One OS process per peer/case (TASK-12). Historical `mem.rs` is in-process.
//!
//! ```sh
//! cargo bench --bench mem_iso -- --quick
//! # or the binary:
//! target/release/deps/mem_iso-* --child --peer syom --case lc_adts --phase one_shot
//! ```

use std::alloc::{GlobalAlloc, Layout, System};
use std::env;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use syom::mem_iso::MemReport;
use syom::{DecodeOptions, Decoder, decode_with};

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

fn child(args: &[String]) {
    let peer = arg_val(args, "--peer");
    let case = arg_val(args, "--case");
    let phase = arg_val(args, "--phase");
    let chunks: usize = arg_val(args, "--chunks").parse().unwrap_or(1);
    let bytes = fixture(&case);
    let baseline = rss_peak();
    ALLOCS.store(0, Ordering::Relaxed);
    BYTES.store(0, Ordering::Relaxed);
    // LIVE/PEAK keep process lifetime; snapshot before work.
    let live0 = LIVE.load(Ordering::Relaxed);
    let peak0 = PEAK.load(Ordering::Relaxed);
    match phase.as_str() {
        "one_shot" => {
            let _ = decode_with(bytes, &DecodeOptions::unbounded());
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
    let live = LIVE.load(Ordering::Relaxed).saturating_sub(live0);
    let peak = PEAK.load(Ordering::Relaxed).saturating_sub(peak0);
    let peak_rss = rss_peak();
    let report = MemReport {
        peer,
        case,
        phase,
        baseline_rss: baseline,
        peak_rss,
        peak_rss_delta: peak_rss.saturating_sub(baseline),
        peak_live: peak.max(live),
        allocs: ALLOCS.load(Ordering::Relaxed),
        alloc_bytes: BYTES.load(Ordering::Relaxed),
        retained: LIVE.load(Ordering::Relaxed),
    };
    println!("{}", report.to_json());
}

fn parent() {
    let exe = match env::current_exe() {
        Ok(p) => p,
        Err(_) => {
            eprintln!("mem_iso: no current_exe");
            return;
        }
    };
    let peers = ["syom"];
    let cases = ["lc_adts", "he_adts", "ps_adts", "mc_adts"];
    let phases = [("one_shot", "1"), ("stream", "1"), ("stream", "8")];
    let mut jobs = Vec::new();
    for peer in peers {
        for case in cases {
            for (phase, chunks) in phases {
                jobs.push((peer, case, phase, chunks));
            }
        }
    }
    let mut reverse = jobs.clone();
    reverse.reverse();
    for (label, list) in [
        ("forward", jobs.as_slice()),
        ("reverse", reverse.as_slice()),
    ] {
        println!("# order={label}");
        for (peer, case, phase, chunks) in list {
            let out = Command::new(&exe)
                .args([
                    "--child", "--peer", peer, "--case", case, "--phase", phase, "--chunks", chunks,
                ])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output();
            match out {
                Ok(o) => {
                    let s = String::from_utf8_lossy(&o.stdout);
                    for line in s.lines() {
                        if line.starts_with('{') {
                            println!("{line}");
                        }
                    }
                    if !o.status.success() {
                        eprintln!(
                            "child fail {peer} {case} {phase}: {}",
                            String::from_utf8_lossy(&o.stderr)
                        );
                    }
                }
                Err(e) => eprintln!("spawn {e}"),
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
