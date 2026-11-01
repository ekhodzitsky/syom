//! Native-sample reproduction of the interval consumer's noise/tone fixture.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use crate::{AacError, DecodeOptions, EncodeOptions, M4aSeek, decode_with, encode_with};
use std::io::Cursor;
use std::sync::OnceLock;

#[path = "exact_seek_grid_tests.rs"]
mod grids;

struct Fixture {
    rate: u32,
    pns: bool,
    observed_pns: bool,
    bytes: Vec<u8>,
    pcm: Vec<Vec<f32>>,
}

fn fixtures() -> &'static [Fixture] {
    static FIXTURES: OnceLock<Vec<Fixture>> = OnceLock::new();
    FIXTURES.get_or_init(|| {
        let mut out = Vec::new();
        for rate in [16_000, 44_100] {
            for pns in [false, true] {
                for channels in [1, 2] {
                    let mut noise = 42u32;
                    let planes: Vec<Vec<f32>> = (0..channels)
                        .map(|ch| {
                            (0..rate * 6)
                                .map(|i| {
                                    noise = noise.wrapping_mul(1664525).wrapping_add(1013904223);
                                    0.2 * (i as f32 * (0.17 + ch as f32 * 0.03)).sin()
                                        + 0.2 * (noise as i32 as f32 / i32::MAX as f32)
                                })
                                .collect()
                        })
                        .collect();
                    let refs: Vec<_> = planes.iter().map(Vec::as_slice).collect();
                    let bytes = encode_with(
                        &refs,
                        rate,
                        &EncodeOptions::m4a().with_bitrate_bps(64_000).with_pns(pns),
                    )
                    .unwrap();
                    let observed_pns = has_pns(&bytes);
                    assert!(!observed_pns || pns);
                    let pcm = decode_with(&bytes, &DecodeOptions::audio())
                        .unwrap()
                        .channels;
                    out.push(Fixture {
                        rate,
                        pns,
                        observed_pns,
                        bytes,
                        pcm,
                    });
                }
            }
        }
        assert!(
            out.iter().any(|f| f.observed_pns),
            "fixture must exercise PNS"
        );
        out
    })
}

fn has_pns(bytes: &[u8]) -> bool {
    let track = crate::isomp4::parse_aac_track(bytes).unwrap();
    let (asc, _) = crate::engine::asc::AudioSpecificConfig::parse(&track.asc).unwrap();
    let mut dec = crate::engine::decode::StreamDecoder::new();
    for (at, len) in track.frames {
        dec.decode_frame_scaled(
            asc.aot,
            asc.sampling_frequency_index,
            asc.sample_rate,
            asc.channel_configuration,
            &bytes[at as usize..at as usize + len as usize],
        )
        .unwrap();
        if dec.rng != crate::engine::pns::Lcg::new() {
            return true;
        }
    }
    false
}

fn assert_tail(src: &mut M4aSeek<Cursor<&Vec<u8>>>, f: &Fixture, at: usize) {
    let mut position = at;
    src.decode(|frame| {
        assert_eq!(frame.planar.len(), f.pcm.len());
        for (got, want) in frame.planar.iter().zip(&f.pcm) {
            for (a, b) in got.iter().zip(&want[position..position + frame.samples]) {
                assert_eq!(
                    a.to_bits(),
                    b.to_bits(),
                    "rate={} pns={} at={at} pos={position}",
                    f.rate,
                    f.pns
                );
            }
        }
        position += frame.samples;
        Ok(())
    })
    .unwrap();
    assert_eq!(position, f.pcm[0].len());
}

#[test]
fn current_pns_fix_matches_native_consumer_fixture() {
    for f in fixtures() {
        let mut src = M4aSeek::open(Cursor::new(&f.bytes), DecodeOptions::audio()).unwrap();
        for at in [f.rate as usize * 3, f.rate as usize * 4001 / 1000, 0, 1025] {
            src.seek(at as i64).unwrap();
            assert_tail(&mut src, f, at);
        }
    }
}

#[test]
fn exact_seek_replays_cold_repeated_backward_and_reopened_positions() {
    for f in fixtures() {
        let mut src = M4aSeek::open(Cursor::new(&f.bytes), DecodeOptions::audio()).unwrap();
        let checkpoint_position = f.rate as usize * 4001 / 1000;
        for at in [checkpoint_position, checkpoint_position, 0, 1025] {
            let plan = src.exact_seek_plan(at as i64, at as i64 + 1).unwrap();
            let replay = plan.replay_access_units();
            assert_eq!(plan.access_units().start, 0);
            assert_eq!(plan.sample_rate(), f.rate);
            src.seek_exact(at as i64, replay).unwrap();
            assert_eq!(src.last_seek_aus(), replay);
            assert_tail(&mut src, f, at);
        }
        // A saved logical position is not a codec checkpoint or source proof.
        let mut reopened = M4aSeek::open(Cursor::new(&f.bytes), DecodeOptions::audio()).unwrap();
        assert!(matches!(
            reopened.seek_exact(checkpoint_position as i64, 2),
            Err(AacError::SeekReplayRequired { .. })
        ));
        reopened
            .seek_exact(checkpoint_position as i64, u64::MAX)
            .unwrap();
        assert_tail(&mut reopened, f, checkpoint_position);
    }
}

#[derive(Default)]
struct Reads {
    allowed: Option<Vec<std::ops::Range<u64>>>,
    reads: Vec<std::ops::Range<u64>>,
    seeks: Vec<u64>,
    denied: usize,
}

struct Sparse {
    inner: Cursor<Vec<u8>>,
    state: std::rc::Rc<std::cell::RefCell<Reads>>,
}
impl std::io::Read for Sparse {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        use std::io::{Error, ErrorKind};
        let at = self.inner.position();
        let len = out
            .len()
            .min(self.inner.get_ref().len().saturating_sub(at as usize));
        let range = at..at + len as u64;
        let mut s = self.state.borrow_mut();
        if let Some(allowed) = &s.allowed
            && !allowed
                .iter()
                .any(|r| r.start <= range.start && r.end >= range.end)
        {
            s.denied += 1;
            return Err(Error::new(
                ErrorKind::PermissionDenied,
                "media range not acquired",
            ));
        }
        let n = self.inner.read(out)?;
        s.reads.push(at..at + n as u64);
        Ok(n)
    }
}
impl std::io::Seek for Sparse {
    fn seek(&mut self, at: std::io::SeekFrom) -> std::io::Result<u64> {
        let pos = self.inner.seek(at)?;
        self.state.borrow_mut().seeks.push(pos);
        Ok(pos)
    }
}

fn sparse(bytes: &[u8]) -> (M4aSeek<Sparse>, std::rc::Rc<std::cell::RefCell<Reads>>) {
    let state = std::rc::Rc::new(std::cell::RefCell::new(Reads::default()));
    let reader = Sparse {
        inner: Cursor::new(bytes.to_vec()),
        state: state.clone(),
    };
    let src = M4aSeek::open(reader, DecodeOptions::audio()).unwrap();
    *state.borrow_mut() = Reads::default(); // Metadata reads belong to open.
    (src, state)
}

fn interval(src: &mut M4aSeek<Sparse>, pcm: &[Vec<f32>], start: usize, end: usize) {
    let mut at = start;
    let result = src.decode(|frame| {
        assert_eq!(frame.planar.len(), pcm.len());
        let take = frame.samples.min(end - at);
        for (plane, reference) in frame.planar.iter().zip(pcm) {
            for (a, b) in plane[..take].iter().zip(&reference[at..at + take]) {
                assert_eq!(a.to_bits(), b.to_bits());
            }
        }
        at += take;
        if at == end {
            return Err(AacError::decode("interval complete"));
        }
        Ok(())
    });
    assert!(matches!(result, Err(AacError::Decode(ref s)) if s == "interval complete"));
    assert_eq!(at, end);
}

#[test]
fn planned_payload_ranges_match_observed_reads_and_au_counts() {
    for f in fixtures() {
        let (mut src, state) = sparse(&f.bytes);
        let start = f.rate as usize * 4001 / 1000;
        let end = start + f.rate as usize / 2;
        let plan = src.exact_seek_plan(start as i64, end as i64).unwrap();
        assert_eq!(plan.history(), crate::ExactSeekHistory::PrefixReplay);
        assert_eq!(plan.presentation_samples(), start as u64..end as u64);
        let ranges: Vec<_> = plan.media_ranges().collect();
        let replay = plan.replay_access_units();
        assert!(state.borrow().reads.is_empty() && state.borrow().seeks.is_empty());
        state.borrow_mut().allowed = Some(ranges.clone());
        src.seek_exact(start as i64, replay).unwrap();
        assert_eq!(src.last_seek_aus(), replay);
        interval(&mut src, &f.pcm, start, end);
        let seen = state.borrow();
        assert_eq!(seen.reads, ranges);
        assert_eq!(
            seen.seeks,
            ranges.iter().map(|r| r.start).collect::<Vec<_>>()
        );
        assert_eq!(seen.denied, 0);
        eprintln!(
            "exact_seek_read {{\"rate\":{},\"pns\":{},\"observed_pns\":{},\"channels\":{},\"start\":{},\"end\":{},\"replay_aus\":{},\"total_aus\":{},\"payload_bytes\":{},\"native_differences\":0}}",
            f.rate,
            f.pns,
            f.observed_pns,
            f.pcm.len(),
            start,
            end,
            replay,
            ranges.len(),
            seen.reads.iter().map(|r| r.end - r.start).sum::<u64>()
        );
    }
}

#[test]
fn budget_is_pre_io_and_unavailable_prefix_fails_even_after_warm_decode() {
    for f in fixtures() {
        let (mut src, state) = sparse(&f.bytes);
        src.decode(|_| Ok(())).unwrap();
        let original_position = src.position();
        let start = f.rate as usize * 3;
        let ranges: Vec<_> = src
            .exact_seek_plan(start as i64, start as i64 + 1)
            .unwrap()
            .media_ranges()
            .collect();
        *state.borrow_mut() = Reads {
            allowed: Some(vec![ranges.last().unwrap().clone()]),
            ..Reads::default()
        };
        assert!(matches!(
            src.seek_exact(start as i64, 2),
            Err(AacError::SeekReplayRequired { .. })
        ));
        assert_eq!(src.position(), original_position);
        assert!(state.borrow().reads.is_empty() && state.borrow().seeks.is_empty());
        assert!(matches!(
            src.seek_exact(start as i64, u64::MAX),
            Err(AacError::Io(_))
        ));
        assert_eq!(state.borrow().denied, 1);
        assert!(matches!(
            src.decode(|_| panic!("failed exact state emitted PCM")),
            Err(AacError::Lifecycle { .. })
        ));
        state.borrow_mut().allowed = None;
        src.seek_exact(start as i64, u64::MAX).unwrap();
        interval(&mut src, &f.pcm, start, start + 17);
    }
}

#[test]
fn unsupported_histories_empty_requests_and_invalid_coordinates_are_explicit() {
    for data in [
        include_bytes!("goldens/he48.m4a").as_slice(),
        include_bytes!("goldens/ps48.m4a"),
        include_bytes!("goldens/mc51.m4a"),
    ] {
        let (mut src, reads) = sparse(data);
        assert!(matches!(
            src.exact_seek_plan(0, 1),
            Err(AacError::Unsupported(
                crate::UnsupportedFeature::ExactM4aSeek
            ))
        ));
        assert!(src.seek_exact(1025, u64::MAX).is_err());
        assert!(reads.borrow().reads.is_empty() && reads.borrow().seeks.is_empty());
    }
    let (mut src, reads) = sparse(include_bytes!("goldens/sine441.m4a"));
    for (a, b) in [(-1, 0), (2, 1), (0, i64::MAX)] {
        assert!(src.exact_seek_plan(a, b).is_err());
    }
    let end = src.presentation_len() as i64;
    assert_eq!(
        src.exact_seek_plan(end, end).unwrap().history(),
        crate::ExactSeekHistory::NoMedia
    );
    src.seek_exact(end, 0).unwrap();
    assert_eq!(
        src.decode(|_| panic!("EOF emitted PCM")).unwrap().samples,
        0
    );
    assert!(reads.borrow().reads.is_empty() && reads.borrow().seeks.is_empty());
}

#[test]
fn logical_checkpoints_restore_with_replay_and_reject_other_sources() {
    for f in fixtures() {
        let (mut src, reads) = sparse(&f.bytes);
        let at = f.rate as usize * 4001 / 1000;
        src.seek_exact(at as i64, u64::MAX).unwrap();
        let saved = src.save_exact_position().unwrap();
        assert_eq!(saved.presentation_sample(), at as u64);
        src.seek_exact(0, u64::MAX).unwrap();
        *reads.borrow_mut() = Reads::default();
        assert!(matches!(
            src.restore_exact_position(&saved, 2),
            Err(AacError::SeekReplayRequired { .. })
        ));
        assert!(reads.borrow().reads.is_empty());
        src.restore_exact_position(&saved.clone(), u64::MAX)
            .unwrap();
        interval(&mut src, &f.pcm, at, at + 17);
        let (mut other, other_reads) = sparse(&f.bytes);
        assert!(matches!(
            other.restore_exact_position(&saved, u64::MAX),
            Err(AacError::ForeignSeekPosition)
        ));
        assert_eq!(other.position(), 0);
        assert!(other_reads.borrow().reads.is_empty() && other_reads.borrow().seeks.is_empty());
    }
}

#[test]
fn corrupt_prefix_invalidates_exact_state_and_cannot_yield_approximate_pcm() {
    let f = &fixtures()[0];
    let src = M4aSeek::open(Cursor::new(&f.bytes), DecodeOptions::audio()).unwrap();
    let first = src
        .exact_seek_plan(0, 1)
        .unwrap()
        .media_ranges()
        .next()
        .unwrap();
    let mut corrupt = f.bytes.clone();
    corrupt[first.start as usize..first.end as usize].fill(0xff);
    let (mut src, _) = sparse(&corrupt);
    assert!(src.seek_exact(f.rate as i64 * 3, u64::MAX).is_err());
    assert!(src.save_exact_position().is_err());
    assert!(matches!(
        src.decode(|_| panic!("corrupt state emitted PCM")),
        Err(AacError::Lifecycle { .. })
    ));
}
