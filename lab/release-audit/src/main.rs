//! Matched planar-output decode audit. Not part of the published library.
use std::hint::black_box;
use std::time::Instant;
use syom::decode_cmp::{Candidate, Collect, Lane, Pcm, run_preflight, syom_candidate};

#[allow(dead_code)]
mod peers;

fn main() {
    let fast = std::env::args().any(|a| a == "--fast");
    let (warm, reps) = if fast { (8, 101) } else { (4, 21) };
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    println!("# syom 0.8.0 / rusty_aac 2.0.1 / symphonia 0.6.1 / oxideav-aac 0.1.7");
    println!(
        "# release thin-LTO cg=1; {warm} warmup + {reps} reps; planar f32 including collection and drop; fast={fast}"
    );
    let mut failed = false;
    for (name, path, m4a) in [
        ("lc_adts", "sine48.adts", false),
        ("lc_m4a", "sine441.m4a", true),
        ("he_adts", "he48.adts", false),
        ("he_m4a", "he48.m4a", true),
        ("ps_adts", "ps48.adts", false),
        ("mc_adts", "mc51.adts", false),
        ("lecture_m4a", "lecture.m4a", true),
    ] {
        let bytes = std::fs::read(root.join("src/goldens").join(path)).unwrap();
        let cands = [
            syom_candidate(),
            peers::rusty_candidate(),
            peers::oxideav_candidate(),
            if m4a {
                peers::symphonia_m4a_candidate()
            } else {
                peers::symphonia_adts_candidate()
            },
        ];
        let cands: Vec<_> = cands
            .into_iter()
            .filter(|c| !fast || c.id != "oxideav-aac")
            .collect();
        let pf = run_preflight(name, &bytes, Lane::PlanarSplit, &cands);
        print!("{}", pf.report());
        let Some(ids) = pf.timed_ids() else {
            failed = true;
            continue;
        };
        let active: Vec<_> = cands.iter().filter(|c| ids.contains(&c.id)).collect();
        let mut times = vec![Vec::new(); active.len()];
        // Rotate engine order per repetition to avoid always measuring one peer first.
        for rep in 0..warm + reps {
            for offset in 0..active.len() {
                let i = (rep + offset) % active.len();
                let start = Instant::now();
                let result = black_box((active[i].collect)(black_box(&bytes), Lane::PlanarSplit));
                let ok = matches!(&result, Collect::Pcm(_));
                drop(result);
                let ns = start.elapsed().as_nanos() as u64;
                assert!(ok, "preflight-qualified timed decode failed");
                if rep >= warm {
                    times[i].push(ns);
                }
            }
        }
        for (cand, raw) in active.iter().zip(times) {
            let mut sorted = raw.clone();
            sorted.sort_unstable();
            println!(
                "TIMING {name} {} median_ns={} p95_ns={} raw_ns={raw:?}",
                cand.id,
                sorted[reps / 2],
                sorted[(reps - 1) * 95 / 100]
            );
        }
    }
    if failed {
        std::process::exit(1);
    }
}
