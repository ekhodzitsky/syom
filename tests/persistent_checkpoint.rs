//! Cross-process DSP history qualification; all media is deterministic synthetic PCM.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::io::{self, Cursor};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use syom::{AacError, ChannelMode, DecodeOptions, EncodeOptions, M4aSeek, encode_with};

const CHILD_DIR: &str = "SYOM_PERSISTENT_CHECKPOINT_DIR";
const CHILD_PHASE: &str = "SYOM_PERSISTENT_CHECKPOINT_PHASE";

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "syom-persistent-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn source(rate: u32, channels: usize, pns: bool) -> Vec<u8> {
    let mut state = 42u32;
    let pcm: Vec<Vec<f32>> = (0..channels)
        .map(|channel| {
            (0..rate * 2 + 13)
                .map(|i| {
                    state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    let noise = state as i32 as f32 / i32::MAX as f32;
                    let burst = if i % 4_097 < 25 { 0.3 } else { 0.0 };
                    0.2 * (i as f32 * (0.17 + channel as f32 * 0.03)).sin() + 0.18 * noise + burst
                })
                .collect()
        })
        .collect();
    encode_with(
        &pcm,
        rate,
        &EncodeOptions::m4a()
            .with_bitrate_bps((rate * channels as u32 * 4).min(64_000))
            .with_pns(pns)
            .with_lookahead(true)
            .with_short_group(true),
    )
    .unwrap()
}

fn pause() -> AacError {
    AacError::Io(io::Error::other("durable checkpoint boundary"))
}

fn save_bits(path: &Path, samples: &[u32]) {
    let bytes: Vec<_> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
    fs::write(path, bytes).unwrap();
}

fn read_bits(path: &Path) -> Vec<u32> {
    let bytes = fs::read(path).unwrap();
    assert_eq!(bytes.len() % 4, 0);
    bytes
        .chunks_exact(4)
        .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()))
        .collect()
}

fn metadata(info: &syom::StreamInfo, position: u64) -> String {
    format!(
        "rate={} core={} channels={} layout={:?} priming={:?} remainder={:?} position={position}",
        info.sample_rate, info.core_rate, info.channels, info.layout, info.priming, info.remainder
    )
}

fn child(dir: &Path, phase: &str) {
    let mut decoder = M4aSeek::open(
        fs::File::open(dir.join("source.m4a")).unwrap(),
        DecodeOptions::speech(),
    )
    .unwrap();
    let state_name = if phase == "pause" {
        "first.state"
    } else {
        "second.state"
    };
    decoder
        .restore_checkpoint(&fs::read(dir.join(state_name)).unwrap())
        .unwrap();
    let mut samples = Vec::new();
    let mut frames = 0;
    let result = decoder.decode(|frame| {
        assert_eq!(frame.planar.len(), 1);
        samples.extend(frame.planar[0].iter().map(|sample| sample.to_bits()));
        frames += 1;
        if phase == "pause" && frames == 2 {
            return Err(pause());
        }
        Ok(())
    });
    if phase == "pause" {
        assert!(result.is_err());
        assert!(decoder.checkpoint_compatible());
        fs::write(dir.join("second.state"), decoder.checkpoint().unwrap()).unwrap();
        save_bits(&dir.join("middle.pcm-bits"), &samples);
    } else {
        let info = result.unwrap();
        assert_eq!(info.samples, samples.len() as u64);
        assert_eq!(decoder.position(), decoder.presentation_len());
        fs::write(
            dir.join("tail.metadata"),
            metadata(&info, decoder.position()),
        )
        .unwrap();
        save_bits(&dir.join("tail.pcm-bits"), &samples);
    }
}

fn spawn(worker: &Path, dir: &Path, phase: &str) {
    let output = Command::new(worker)
        .args([
            "--exact",
            "fresh_process_restore_matches_uninterrupted_native_bits",
            "--nocapture",
        ])
        .env(CHILD_DIR, dir)
        .env(CHILD_PHASE, phase)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "fresh process {phase} failed: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn fresh_process_restore_matches_uninterrupted_native_bits() {
    if let Ok(dir) = std::env::var(CHILD_DIR) {
        child(Path::new(&dir), &std::env::var(CHILD_PHASE).unwrap());
        return;
    }
    // A concurrent Cargo build may replace the running test binary on Unix.
    // Retain one identical executable in owned temporary storage for all workers.
    let suite = Temp::new();
    let worker = suite
        .0
        .join(format!("checkpoint-worker{}", std::env::consts::EXE_SUFFIX));
    fs::copy(std::env::current_exe().unwrap(), &worker).unwrap();
    for rate in [8_000, 16_000, 22_050, 24_000, 32_000, 44_100, 48_000] {
        for channels in [1, 2] {
            for pns in [false, true] {
                let temp = Temp::new();
                let bytes = source(rate, channels, pns);
                fs::write(temp.0.join("source.m4a"), &bytes).unwrap();
                let mut full = M4aSeek::open(Cursor::new(&bytes), DecodeOptions::speech()).unwrap();
                let mut expected = Vec::new();
                let full_info = full
                    .decode(|frame| {
                        assert_eq!(frame.sample_rate, rate);
                        assert_eq!(frame.planar.len(), 1);
                        expected.extend(frame.planar[0].iter().map(|sample| sample.to_bits()));
                        Ok(())
                    })
                    .unwrap();
                assert_eq!(expected.len(), rate as usize * 2 + 13);
                assert!(
                    expected
                        .iter()
                        .all(|bits| f32::from_bits(*bits).is_finite())
                );
                let mut first =
                    M4aSeek::open(Cursor::new(&bytes), DecodeOptions::speech()).unwrap();
                assert!(first.supports_checkpoint());
                assert!(first.checkpoint().is_none(), "priming must finish first");
                let mut got = Vec::new();
                assert!(
                    first
                        .decode(|frame| {
                            got.extend(frame.planar[0].iter().map(|sample| sample.to_bits()));
                            Err(pause())
                        })
                        .is_err()
                );
                assert!(first.checkpoint_compatible());
                let state = first.checkpoint().unwrap();
                fs::write(temp.0.join("first.state"), state).unwrap();
                drop(first);
                spawn(&worker, &temp.0, "pause");
                spawn(&worker, &temp.0, "finish");
                got.extend(read_bits(&temp.0.join("middle.pcm-bits")));
                got.extend(read_bits(&temp.0.join("tail.pcm-bits")));
                assert_eq!(
                    fs::read_to_string(temp.0.join("tail.metadata")).unwrap(),
                    metadata(&full_info, full.position())
                );
                assert_eq!(got, expected, "rate={rate} channels={channels} pns={pns}");
                eprintln!(
                    "persistent-restart rate={rate} channels={channels} pns={pns}: {} samples, two fresh restores, exact native bits",
                    got.len()
                );
            }
        }
    }
}

#[test]
fn checkpoint_exclusions_and_rejected_restore_leave_live_state_unchanged() {
    let bytes = source(16_000, 2, true);
    let mut decoder = M4aSeek::open(Cursor::new(&bytes), DecodeOptions::speech()).unwrap();
    assert!(decoder.decode(|_| Err(pause())).is_err());
    let state = decoder.checkpoint().unwrap();
    let position = decoder.position();
    for len in [0, 7, 40, state.len() - 1] {
        assert!(decoder.restore_checkpoint(&state[..len]).is_err());
        assert_eq!(decoder.position(), position);
        assert_eq!(decoder.checkpoint().unwrap(), state);
    }
    let mut bad = state.clone();
    bad[0] ^= 1;
    assert!(decoder.restore_checkpoint(&bad).is_err());
    assert_eq!(decoder.checkpoint().unwrap(), state);
    assert_eq!(&state[..8], b"SYOMLC01");
    // Version-1 layout: f32 overlap starts after each saved window-shape byte.
    for offset in [229, 4_326] {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut nonfinite = state.clone();
            nonfinite[offset..offset + 4].copy_from_slice(&value.to_bits().to_le_bytes());
            assert!(decoder.restore_checkpoint(&nonfinite).is_err());
            assert_eq!(decoder.checkpoint().unwrap(), state);
        }
    }
    for offset in [8, 200, 208, 216] {
        let mut incompatible = state.clone();
        incompatible[offset] ^= 0xff;
        assert!(decoder.restore_checkpoint(&incompatible).is_err());
        assert_eq!(decoder.checkpoint().unwrap(), state);
    }
    let mut different_options = M4aSeek::open(
        Cursor::new(&bytes),
        DecodeOptions::speech().with_max_duration_secs(60.0),
    )
    .unwrap();
    assert!(different_options.restore_checkpoint(&state).is_err());
    assert_eq!(different_options.position(), 0);
    let different_rate = source(24_000, 2, true);
    let mut wrong = M4aSeek::open(Cursor::new(&different_rate), DecodeOptions::speech()).unwrap();
    assert!(wrong.restore_checkpoint(&state).is_err());
    assert_eq!(wrong.position(), 0);
    let mut planar = M4aSeek::open(
        Cursor::new(&bytes),
        DecodeOptions::speech().with_channel_mode(ChannelMode::Split),
    )
    .unwrap();
    assert!(!planar.supports_checkpoint());
    assert!(planar.restore_checkpoint(&state).is_err());
    let mut exact = M4aSeek::open(Cursor::new(&bytes), DecodeOptions::speech()).unwrap();
    exact.seek_exact(1_025, u64::MAX).unwrap();
    assert!(!exact.supports_checkpoint());
    assert!(exact.checkpoint().is_none());
    assert!(exact.restore_checkpoint(&state).is_err());
    let start = {
        let plan = decoder.exact_seek_plan(1_025, 1_026).unwrap();
        let mut ranges = plan.media_ranges();
        ranges.next().unwrap()
    };
    let mut corrupt = bytes.clone();
    corrupt[start.start as usize..start.end as usize].fill(0xff);
    let mut failed = M4aSeek::open(Cursor::new(corrupt), DecodeOptions::speech()).unwrap();
    assert!(failed.seek_exact(1_025, u64::MAX).is_err());
    assert!(!failed.supports_checkpoint());
    assert!(failed.checkpoint().is_none());
    assert!(failed.restore_checkpoint(&state).is_err());
    let he = include_bytes!("../src/goldens/he48.m4a");
    let mut unsupported = M4aSeek::open(Cursor::new(he), DecodeOptions::speech()).unwrap();
    assert!(!unsupported.supports_checkpoint());
    assert!(unsupported.checkpoint().is_none());
    assert!(unsupported.restore_checkpoint(&state).is_err());
    let mut tail = Vec::new();
    decoder
        .decode(|frame| {
            tail.extend(frame.planar[0].iter().map(|sample| sample.to_bits()));
            Ok(())
        })
        .unwrap();
    let mut reference = M4aSeek::open(Cursor::new(&bytes), DecodeOptions::speech()).unwrap();
    reference.restore_checkpoint(&state).unwrap();
    let mut expected = Vec::new();
    reference
        .decode(|frame| {
            expected.extend(frame.planar[0].iter().map(|sample| sample.to_bits()));
            Ok(())
        })
        .unwrap();
    assert_eq!(
        tail, expected,
        "bad restore must preserve usable DSP history"
    );
}

#[test]
fn restore_and_decode_on_qualified_small_stack() {
    let bytes = source(16_000, 2, true);
    let mut first = M4aSeek::open(Cursor::new(&bytes), DecodeOptions::speech()).unwrap();
    assert!(first.decode(|_| Err(pause())).is_err());
    let state = first.checkpoint().unwrap();
    let mut expected = Vec::new();
    first
        .decode(|frame| {
            expected.extend(frame.planar[0].iter().map(|sample| sample.to_bits()));
            Ok(())
        })
        .unwrap();
    drop(first);
    let stack_kib = if cfg!(debug_assertions) { 512 } else { 128 };
    let restored = std::thread::Builder::new()
        .stack_size(stack_kib * 1024)
        .spawn(move || {
            let mut decoder = M4aSeek::open(Cursor::new(bytes), DecodeOptions::speech()).unwrap();
            decoder.restore_checkpoint(&state).unwrap();
            let mut samples = Vec::new();
            decoder
                .decode(|frame| {
                    samples.extend(frame.planar[0].iter().map(|sample| sample.to_bits()));
                    Ok(())
                })
                .unwrap();
            samples
        })
        .unwrap()
        .join()
        .unwrap();
    assert_eq!(restored, expected);
    eprintln!("persistent-restore small-stack: {stack_kib} KiB; exact native tail bits");
}
