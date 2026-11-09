//! Strict indexed presentation EOF and conservative unknown-scope regressions.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::stream::PresentationCompletion as Completion;
use crate::{DecodeOptions, EncodeOptions, M4aSeek};
use std::io::{self, Cursor, Read, Seek, SeekFrom};

fn fixture() -> Vec<u8> {
    crate::encode_with(&[vec![0.1f32; 1]], 48_000, &EncodeOptions::m4a()).unwrap()
}

fn tag(bytes: &[u8], name: &[u8; 4]) -> usize {
    bytes.windows(4).position(|w| w == name).unwrap()
}

fn field(bytes: &mut [u8], name: &[u8; 4], offset: usize, value: u32) {
    let at = tag(bytes, name) + offset;
    bytes[at..at + 4].copy_from_slice(&value.to_be_bytes());
}

fn available(bytes: &[u8]) -> u64 {
    let opts = DecodeOptions::unbounded();
    let track = crate::isomp4::parse_aac_track_with(bytes, &opts.memory).unwrap();
    let skip = track.edit_window(48_000).unwrap().0 as u64;
    track.frames.len() as u64 * 1024 - skip
}

fn window(bytes: &mut [u8], clock: u32, ticks: u32) {
    field(bytes, b"mvhd", 16, clock);
    field(bytes, b"elst", 12, ticks);
}

/// Uses only pre-existing public API, so it can reproduce the missing strict
/// EOF behavior on 0.9.0 without depending on the new status enum.
#[test]
fn bounded_m4a_eof_is_not_success_when_declared_presentation_is_too_long() {
    let mut bytes = crate::encode_with(&[vec![0.1f32; 1]], 48_000, &EncodeOptions::m4a()).unwrap();
    let edit = bytes.windows(4).position(|w| w == b"elst").unwrap();
    bytes[edit + 12..edit + 16].copy_from_slice(&48_000u32.to_be_bytes());
    let opts = DecodeOptions::speech();
    assert!(crate::decode_streaming(&bytes, &opts, |_| Ok(())).is_err());
    assert!(crate::decode_seek_streaming(Cursor::new(&bytes), &opts, |_| Ok(())).is_err());
    let mut source = M4aSeek::open(Cursor::new(&bytes), opts).unwrap();
    assert!(source.decode(|_| Ok(())).is_err());
}

#[test]
fn exact_and_subtick_completion_never_pad_pcm() {
    let base = fixture();
    let actual = available(&base);
    for (clock, ticks, expected) in [
        (48_000, actual as u32, Completion::Complete),
        (
            1_000,
            actual.div_ceil(48) as u32,
            Completion::Rounded {
                missing_samples: actual.div_ceil(48) * 48 - actual,
            },
        ),
        (
            2_000,
            actual.div_ceil(24) as u32,
            Completion::Rounded {
                missing_samples: actual.div_ceil(24) * 24 - actual,
            },
        ),
    ] {
        let mut bytes = base.clone();
        window(&mut bytes, clock, ticks);
        let opts = DecodeOptions::speech();
        let mut count = 0;
        let info = crate::decode_streaming(&bytes, &opts, |f| {
            count += f.samples;
            Ok(())
        })
        .unwrap();
        assert_eq!(count as u64, actual);
        assert_eq!(info.presentation_completion, expected);
        let mut seek = M4aSeek::open(Cursor::new(&bytes), opts).unwrap();
        assert!(seek.bounded_presentation());
        assert!(!seek.presentation_complete());
        assert_eq!(seek.output_sample_rate(), 48_000);
        let info = seek.decode(|_| Ok(())).unwrap();
        assert_eq!(seek.position(), actual);
        assert_eq!(info.samples, actual);
        assert_eq!(seek.presentation_completion(), expected);
        assert!(seek.presentation_complete());
    }
}

#[test]
fn one_tick_one_millisecond_and_missing_frame_are_not_rounding() {
    let base = fixture();
    let actual = available(&base);
    for (clock, ticks) in [
        (48_000, actual as u32 + 1), // only one sample, but a whole movie tick
        (48_000, actual as u32 + 48),
        (48_000, actual as u32 + 1024),
        (1_000, actual.div_ceil(48) as u32 + 1),
        (2_000, actual.div_ceil(24) as u32 + 1),
    ] {
        let mut bytes = base.clone();
        window(&mut bytes, clock, ticks);
        let opts = DecodeOptions::speech();
        assert!(crate::decode_streaming(&bytes, &opts, |_| Ok(())).is_err());
        let mut seek = M4aSeek::open(Cursor::new(&bytes), opts).unwrap();
        assert!(seek.decode(|_| Ok(())).is_err());
        assert!(!seek.presentation_complete());
        assert!(matches!(
            seek.presentation_completion(),
            Completion::Incomplete { .. }
        ));
    }
}

#[test]
fn unfinished_priming_is_refused_at_indexed_eof() {
    let mut bytes = fixture();
    let opts = DecodeOptions::speech();
    let track = crate::isomp4::parse_aac_track_with(&bytes, &opts.memory).unwrap();
    let coded = track.frames.len() as u32 * 1024;
    field(&mut bytes, b"elst", 16, coded + 1024);
    let err = crate::decode_seek_streaming(Cursor::new(&bytes), &opts, |_| Ok(())).unwrap_err();
    assert!(
        err.to_string().contains("incomplete M4A presentation"),
        "{err}"
    );
    let mut seek = M4aSeek::open(Cursor::new(bytes), opts).unwrap();
    assert!(seek.decode(|_| Ok(())).is_err());
    assert!(matches!(
        seek.presentation_completion(),
        Completion::Incomplete {
            priming_samples: 1024,
            ..
        }
    ));
}

#[test]
fn absent_duration_is_explicitly_unbounded_even_at_current_eof() {
    let mut bytes = fixture();
    let at = tag(&bytes, b"elst");
    bytes[at..at + 4].copy_from_slice(b"free");
    let opts = DecodeOptions::speech();
    let info = crate::decode_streaming(&bytes, &opts, |_| Ok(())).unwrap();
    assert_eq!(
        info.presentation_completion,
        Completion::Unbounded {
            indexed_end_reached: true
        }
    );
    let mut seek = M4aSeek::open(Cursor::new(bytes), opts).unwrap();
    assert!(!seek.bounded_presentation());
    seek.decode(|_| Ok(())).unwrap();
    assert_eq!(
        seek.presentation_completion(),
        Completion::Unbounded {
            indexed_end_reached: true
        }
    );
    assert!(!seek.presentation_complete());
}

struct MissingAu {
    data: Cursor<Vec<u8>>,
    absent: std::ops::Range<u64>,
}
impl Read for MissingAu {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let at = self.data.position();
        if self.absent.contains(&at) {
            return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
        }
        let n = if at < self.absent.start {
            bytes.len().min((self.absent.start - at) as usize)
        } else {
            bytes.len()
        };
        self.data.read(&mut bytes[..n])
    }
}
impl Seek for MissingAu {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.data.seek(pos)
    }
}

#[test]
fn missing_last_payload_cannot_be_accepted_as_metadata_rounding() {
    let mut bytes = fixture();
    let actual = available(&bytes);
    window(&mut bytes, 1_000, actual.div_ceil(48) as u32);
    let opts = DecodeOptions::speech();
    let track = crate::isomp4::parse_aac_track_with(&bytes, &opts.memory).unwrap();
    let &(start, len) = track.frames.last().unwrap();
    let reader = || MissingAu {
        data: Cursor::new(bytes.clone()),
        absent: start..start + u64::from(len),
    };
    assert!(crate::decode_seek_streaming(reader(), &opts, |_| Ok(())).is_err());
    let mut seek = M4aSeek::open(reader(), opts).unwrap();
    assert!(seek.decode(|_| Ok(())).is_err());
    assert!(!seek.presentation_complete());
}

fn dash() -> Vec<u8> {
    [
        include_bytes!("goldens/fmp4_dash_init.m4s").as_slice(),
        include_bytes!("goldens/fmp4_dash_seg1.m4s").as_slice(),
    ]
    .concat()
}

#[test]
fn push_fragment_completion_distinguishes_bounded_and_unknown_scope() {
    let base = dash();
    let opts = DecodeOptions::speech();
    let actual = crate::decode_streaming(&base, &opts, |_| Ok(()))
        .unwrap()
        .samples;
    for extra in [None, Some(0), Some(1024)] {
        let mut bytes = base.clone();
        if let Some(extra) = extra {
            window(&mut bytes, 48_000, (actual + extra) as u32);
        }
        let mut decoder = crate::Decoder::new(opts.clone());
        let mut count = 0;
        for chunk in bytes.chunks(97) {
            decoder
                .feed(chunk, |f| {
                    count += f.samples;
                    Ok(())
                })
                .unwrap();
        }
        let result = decoder.finish(|f| {
            count += f.samples;
            Ok(())
        });
        assert_eq!(count as u64, actual);
        match extra {
            None => assert_eq!(
                result.unwrap().presentation_completion,
                Completion::Unbounded {
                    indexed_end_reached: false
                }
            ),
            Some(0) => assert_eq!(
                result.unwrap().presentation_completion,
                Completion::Complete
            ),
            Some(_) => {
                assert!(
                    result
                        .unwrap_err()
                        .to_string()
                        .contains("incomplete M4A presentation")
                );
                assert!(matches!(
                    decoder.finish(|_| Ok(())),
                    Err(crate::AacError::Lifecycle { .. })
                ));
            }
        }
    }
}

#[test]
fn push_unfinished_priming_and_unproven_rounded_end_are_refused() {
    let base = dash();
    let opts = DecodeOptions::speech();
    let actual = crate::decode_streaming(&base, &opts, |_| Ok(()))
        .unwrap()
        .samples;
    assert_ne!(actual % 48, 0);
    let mut rounded = base.clone();
    window(&mut rounded, 1_000, actual.div_ceil(48) as u32);
    // The complete indexed slice can prove all AUs. The push decoder cannot
    // infer a complete lifetime index from the last fragment received.
    let info = crate::decode_streaming(&rounded, &opts, |_| Ok(())).unwrap();
    assert!(matches!(
        info.presentation_completion,
        Completion::Rounded { .. }
    ));
    let mut unfinished = base.clone();
    field(&mut unfinished, b"elst", 16, (actual + 2048) as u32);
    for bytes in [&rounded, &unfinished] {
        let mut decoder = crate::Decoder::new(opts.clone());
        decoder.feed(bytes, |_| Ok(())).unwrap();
        let err = decoder.finish(|_| Ok(())).unwrap_err();
        assert!(
            err.to_string().contains("incomplete M4A presentation"),
            "{err}"
        );
    }
}

#[test]
fn open_fragment_tail_after_seek_and_interruption_uses_absolute_position() {
    let bytes = dash();
    let opts = DecodeOptions::speech();
    let mut full = M4aSeek::open(Cursor::new(&bytes), opts.clone()).unwrap();
    let reference = full.decode(|_| Ok(())).unwrap();
    assert_eq!(reference.remainder, Some(0));
    let mut seek = M4aSeek::open(Cursor::new(&bytes), opts.clone()).unwrap();
    seek.seek(1024).unwrap();
    let info = seek.decode(|_| Ok(())).unwrap();
    assert_eq!(info.samples + 1024, reference.samples);
    assert_eq!(info.remainder, reference.remainder);
    let mut interrupted = M4aSeek::open(Cursor::new(&bytes), opts).unwrap();
    assert!(
        interrupted
            .decode(|_| Err(crate::AacError::format("pause")))
            .is_err()
    );
    let at = interrupted.position();
    assert!(at > 0 && at < reference.samples);
    let info = interrupted.decode(|_| Ok(())).unwrap();
    assert_eq!(at + info.samples, reference.samples);
    assert_eq!(info.remainder, reference.remainder);
}

fn tagged_without_edit(source_samples: usize, stale: bool) -> Vec<u8> {
    let mut bytes = crate::encode_with(
        &[vec![0.1f32; source_samples]],
        48_000,
        &EncodeOptions::m4a(),
    )
    .unwrap();
    let edit = tag(&bytes, b"elst");
    bytes[edit..edit + 4].copy_from_slice(b"free");
    let mut delay = crate::gapless::delay_lc(source_samples as u64);
    if stale {
        delay.source += 1;
    }
    crate::gapless::test_support::append_tag(
        &mut bytes,
        format!(
            " 00000000 {:08X} {:08X} {:016X}",
            delay.priming, delay.remainder, delay.source
        )
        .as_bytes(),
    )
    .unwrap();
    bytes
}

#[test]
fn matching_gapless_tag_is_bounded_but_stale_tag_is_not_completion_proof() {
    let opts = DecodeOptions::speech();
    let valid = tagged_without_edit(5000, false);
    let info = crate::decode_streaming(&valid, &opts, |_| Ok(())).unwrap();
    assert_eq!(info.samples, 5000);
    assert_eq!(info.presentation_completion, Completion::Complete);
    let stale = tagged_without_edit(5000, true);
    let info = crate::decode_streaming(&stale, &opts, |_| Ok(())).unwrap();
    assert!(info.samples > 5000);
    assert_eq!(
        info.presentation_completion,
        Completion::Unbounded {
            indexed_end_reached: true
        }
    );
    // The same count-validated window bounds seekable playback.
    let mut seek = M4aSeek::open(Cursor::new(valid), opts.clone()).unwrap();
    let info = seek.decode(|_| Ok(())).unwrap();
    assert_eq!(info.samples, 5000);
    assert!(seek.bounded_presentation());
    assert!(seek.presentation_complete());
    let mut seek = M4aSeek::open(Cursor::new(stale), opts).unwrap();
    seek.decode(|_| Ok(())).unwrap();
    assert!(!seek.bounded_presentation());
    assert!(!seek.presentation_complete());
}
