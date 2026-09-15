//! TASK-60: `encode_write_m4a` — incremental M4A on a `Write + Seek` sink,
//! byte parity with one-shot M4A, relative offsets, failure lifecycle,
//! preflight ceiling and index budget.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::encode_tests::lavc_fixture;
use crate::m4a_write::{MAX_M4A_AUS, MAX_M4A_V0_BYTES, preflight};
use crate::{
    AacError, DecodeOptions, EncodeOptions, decode_with, encode_with, encode_write_m4a, probe,
};
use std::io::{self, Cursor, Seek, SeekFrom, Write};

/// A cursor that fails every write once `budget` bytes were accepted, and
/// optionally every seek.
struct Flaky {
    inner: Cursor<Vec<u8>>,
    budget: usize,
    seek_fails: bool,
}
impl Write for Flaky {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.budget == 0 {
            return Err(io::Error::other("disk full"));
        }
        let n = buf.len().min(self.budget);
        self.budget -= n;
        self.inner.write(&buf[..n])
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl Seek for Flaky {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        if self.seek_fails {
            return Err(io::Error::other("unseekable"));
        }
        self.inner.seek(pos)
    }
}

#[test]
fn sink_bytes_match_one_shot_m4a_for_lc_lookahead_and_he() {
    let pcm = lavc_fixture();
    for opts in [
        EncodeOptions::m4a(),
        EncodeOptions::m4a().with_lookahead(true),
        EncodeOptions::low_rate().with_container(crate::EncodeContainer::M4a),
        EncodeOptions::adts().with_quality(6), // container ignored: always M4A
    ] {
        let mut sink = Cursor::new(Vec::new());
        let info = encode_write_m4a(&mut sink, &pcm, 48_000, &opts).unwrap();
        let bytes = sink.into_inner();
        let want = encode_with(
            &pcm,
            48_000,
            &opts.clone().with_container(crate::EncodeContainer::M4a),
        )
        .unwrap();
        assert_eq!(bytes, want, "{opts:?}");
        assert_eq!(info.samples, pcm[0].len() as u64);
        let dec = decode_with(&bytes, &DecodeOptions::audio()).unwrap();
        assert_eq!(dec.channels[0].len(), pcm[0].len(), "presentation is N");
        assert_eq!(dec.priming, Some(info.priming));
    }
    let mut sink = Cursor::new(Vec::new());
    encode_write_m4a(&mut sink, &pcm, 48_000, &EncodeOptions::low_rate()).unwrap();
    assert_eq!(
        sink.into_inner(),
        &include_bytes!("goldens/he48em.m4a")[..],
        "lavc-decoded golden"
    );
}

#[test]
fn offsets_are_relative_to_the_entry_position_and_chunking_is_invisible() {
    let pcm = lavc_fixture();
    let want = encode_with(&pcm, 48_000, &EncodeOptions::m4a()).unwrap();
    let mut sink = Cursor::new(vec![0xEEu8; 123]);
    sink.seek(SeekFrom::End(0)).unwrap();
    encode_write_m4a(&mut sink, &pcm, 48_000, &EncodeOptions::m4a()).unwrap();
    let all = sink.into_inner();
    assert_eq!(&all[..123], &[0xEEu8; 123][..], "prefix untouched");
    assert_eq!(
        &all[123..],
        &want[..],
        "relative offsets: the M4A starts at the entry position"
    );
    assert_eq!(probe(&all[123..]).unwrap().meta.core_rate, 48_000);
}

#[test]
fn write_and_seek_failures_are_io_errors_with_a_partial_unplayable_prefix() {
    let pcm = lavc_fixture();
    let want = encode_with(&pcm, 48_000, &EncodeOptions::m4a()).unwrap();
    for budget in [0usize, 20, 40, 1_000, want.len() - 40] {
        let mut sink = Flaky {
            inner: Cursor::new(Vec::new()),
            budget,
            seek_fails: false,
        };
        let e = encode_write_m4a(&mut sink, &pcm, 48_000, &EncodeOptions::m4a()).unwrap_err();
        assert!(matches!(e, AacError::Io(_)), "{e}");
        let got = sink.inner.into_inner();
        assert!(got.len() <= budget, "never more than the sink accepted");
        if budget >= 40 {
            assert_eq!(&got[..28], &want[..28], "ftyp first (28 bytes)");
            assert_eq!(&got[32..36], b"mdat");
            // Either the mdat size is still the placeholder (write failed
            // inside mdat) or it was patched and moov never arrived: both
            // are unplayable, nothing is written twice.
            let size = u32::from_be_bytes([got[28], got[29], got[30], got[31]]);
            assert!(
                size == 0 || got.len() >= size as usize + 28,
                "mdat size {size}"
            );
            assert!(
                decode_with(&got, &DecodeOptions::audio()).is_err(),
                "not a valid M4A before Ok"
            );
        }
    }
    let mut sink = Flaky {
        inner: Cursor::new(Vec::new()),
        budget: usize::MAX,
        seek_fails: true,
    };
    let e = encode_write_m4a(&mut sink, &pcm, 48_000, &EncodeOptions::m4a()).unwrap_err();
    assert!(matches!(e, AacError::Io(_)), "{e}");
}

#[test]
fn preflight_rejects_the_version_0_ceiling_and_the_index_budget_without_wrapping() {
    assert!(preflight(40, 1 << 20, 100).is_ok());
    assert!(
        preflight(40, MAX_M4A_V0_BYTES - 100, 100).is_err(),
        "file size over u32"
    );
    assert!(
        preflight(40, u64::MAX - 10, 1).is_err(),
        "no wrap on overflow"
    );
    assert!(preflight(40, 1 << 20, MAX_M4A_AUS).is_ok());
    assert!(
        preflight(40, 1 << 20, MAX_M4A_AUS + 1).is_err(),
        "index budget"
    );
    // The one-shot muxer shares the same checks.
    let big = vec![vec![0u8; 1 << 20]; 2];
    let refs: Vec<&[u8]> = big.iter().map(Vec::as_slice).collect();
    assert!(crate::mux_raw_lc_m4a(&refs, 48_000, 1, 1024).is_ok());
}

#[test]
fn m4a_and_pcm_errors_are_typed_before_the_first_write() {
    let mut sink = Cursor::new(Vec::new());
    let bad = vec![vec![2.0f32; 4096]];
    assert!(matches!(
        encode_write_m4a(&mut sink, &bad, 48_000, &EncodeOptions::m4a()).unwrap_err(),
        AacError::InvalidPcm(_)
    ));
    assert!(sink.get_ref().is_empty());
    let e = encode_write_m4a(
        &mut sink,
        &[vec![0.1f32; 100]],
        96_000,
        &EncodeOptions::low_rate(),
    )
    .unwrap_err();
    assert!(matches!(e, AacError::Unsupported(_)));
    assert!(sink.get_ref().is_empty());
}
