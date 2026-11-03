//! Bounded persistent-state mutation campaign; no external codecs or recordings.
use std::io::Cursor;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::{Duration, Instant};
use syom::{encode_with, AacError, DecodeOptions, EncodeOptions, M4aSeek};

fn next(state: &mut u32) -> u32 {
    *state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    *state
}

fn main() {
    let mut seed = 1442u32;
    let mut limit = 5_000u64;
    let mut seconds = 30u64;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--seed" => seed = args.next().unwrap().parse().unwrap(),
            "--iters" => limit = args.next().unwrap().parse().unwrap(),
            "--seconds" => seconds = args.next().unwrap().parse().unwrap(),
            _ => panic!("unknown option {arg}"),
        }
    }
    let mut random = seed;
    let pcm: Vec<f32> = (0..24_013)
        .map(|_| ((next(&mut random) >> 8) as f32 / 16_777_216.0 - 0.5) * 0.5)
        .collect();
    let bytes = encode_with(&[pcm], 48_000, &EncodeOptions::m4a().with_pns(true)).unwrap();
    let open = || M4aSeek::open(Cursor::new(&bytes), DecodeOptions::speech()).unwrap();
    let mut source = open();
    let _ = source.decode(|_| Err(AacError::decode("intentional checkpoint boundary")));
    let original = source.checkpoint().expect("qualified interrupted state");
    let start = Instant::now();
    let mut iterations = 0;
    let mut rejected = 0;
    let mut accepted = 0;
    while iterations < limit && start.elapsed() < Duration::from_secs(seconds) {
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            let mut target = open();
            target.restore_checkpoint(&original).unwrap();
            let before = target.checkpoint().unwrap();
            let mut mutated = original.clone();
            match next(&mut random) % 5 {
                0 => {
                    let index = next(&mut random) as usize % mutated.len();
                    mutated[index] ^= (next(&mut random) as u8) | 1;
                }
                1 => mutated.truncate(next(&mut random) as usize % mutated.len()),
                2 => mutated.extend_from_slice(&next(&mut random).to_le_bytes()),
                3 => {
                    for _ in 0..8 {
                        let index = next(&mut random) as usize % mutated.len();
                        mutated[index] = next(&mut random) as u8;
                    }
                }
                _ => {} // Valid restore is exercised alongside malformed inputs.
            }
            match target.restore_checkpoint(&mutated) {
                Err(_) => {
                    assert_eq!(target.checkpoint().as_ref(), Some(&before));
                    rejected += 1;
                }
                Ok(()) => {
                    accepted += 1;
                    // Structurally valid finite state is caller-authenticated;
                    // arbitrary finite overlaps need not reproduce original PCM.
                    let _ = target.decode(|frame| {
                        assert!(frame
                            .planar
                            .iter()
                            .flat_map(|p| p.iter())
                            .all(|v| v.is_finite()));
                        Ok(())
                    });
                }
            }
        }));
        if outcome.is_err() {
            eprintln!("checkpoint mutation failed: seed={seed} iteration={iterations}");
            std::process::exit(1);
        }
        iterations += 1;
    }
    println!("syom persistent checkpoint mutation TASK-144");
    println!("seed {seed}\niters {iterations}\naccepted {accepted}\nrejected {rejected}");
    println!("wall_ms {}\npanics 0", start.elapsed().as_millis());
}
