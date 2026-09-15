//! TASK-59: `encode_write` — byte parity with one-shot encode on any sink,
//! short and failing writers, exact-prefix lifecycle, metadata.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::{
    AacError, EncodeContainer, EncodeOptions, Encoder, UnsupportedFeature, encode_with,
    encode_write,
};
use std::io::{self, Write};

fn material(n: usize) -> Vec<Vec<f32>> {
    let mut s = 0x1234_5678u32;
    (0..2)
        .map(|c| {
            (0..n)
                .map(|i| {
                    s ^= s << 13;
                    s ^= s >> 17;
                    s ^= s << 5;
                    0.1 * ((s as f32 / u32::MAX as f32) - 0.5)
                        + 0.3
                            * (2.0 * std::f32::consts::PI * (440.0 + 110.0 * c as f32) * i as f32
                                / 48_000.0)
                                .sin()
                })
                .collect()
        })
        .collect()
}

/// Accepts one byte per `write` call (exercises `write_all` loops).
struct OneByte(Vec<u8>);
impl Write for OneByte {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        self.0.push(buf[0]);
        Ok(1)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Accepts `left` bytes, then fails every write.
struct Failing {
    out: Vec<u8>,
    left: usize,
}
impl Write for Failing {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.left == 0 {
            return Err(io::Error::other("sink full"));
        }
        let n = buf.len().min(self.left);
        self.out.extend_from_slice(&buf[..n]);
        self.left -= n;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn sink_bytes_and_info_match_one_shot_and_push_for_every_mode() {
    let pcm = material(48_000 / 2 + 777);
    let planes: Vec<&[f32]> = pcm.iter().map(Vec::as_slice).collect();
    for opts in [
        EncodeOptions::adts(),
        EncodeOptions::adts().with_lookahead(true),
        EncodeOptions::low_rate(),
        EncodeOptions::adts().with_quality(4),
        EncodeOptions::raw(),
    ] {
        let mut sink = Vec::new();
        let info = encode_write(&mut sink, &planes, 48_000, &opts).unwrap();
        assert_eq!(info.bytes as usize, sink.len());
        assert_eq!(info.samples, pcm[0].len() as u64);
        if opts.container == EncodeContainer::Raw {
            let mut enc = Encoder::new(48_000, 2, &opts).unwrap();
            let mut raw = Vec::new();
            enc.feed(&planes, |f| {
                raw.extend_from_slice(f.au);
                Ok(())
            })
            .unwrap();
            let pinfo = enc
                .finish(|f| {
                    raw.extend_from_slice(f.au);
                    Ok(())
                })
                .unwrap();
            assert_eq!(sink, raw, "raw AUs match the push encoder");
            assert_eq!(info, pinfo);
        } else {
            assert_eq!(sink, encode_with(&pcm, 48_000, &opts).unwrap(), "{opts:?}");
        }
        let mut slow = OneByte(Vec::new());
        encode_write(&mut slow, &planes, 48_000, &opts).unwrap();
        assert_eq!(slow.0, sink, "one-byte writer sees the same stream");
    }
}

#[test]
fn failing_sink_leaves_an_exact_prefix_and_no_duplicate_frames() {
    let pcm = material(48_000);
    let full = encode_with(&pcm, 48_000, &EncodeOptions::adts()).unwrap();
    for left in [0usize, 1, 100, 2_000, full.len() - 1] {
        let mut sink = Failing {
            out: Vec::new(),
            left,
        };
        let e = encode_write(&mut sink, &pcm, 48_000, &EncodeOptions::adts()).unwrap_err();
        assert!(matches!(e, AacError::Io(_)), "{e}");
        assert_eq!(
            sink.out.len(),
            left,
            "sink received exactly what it accepted"
        );
        assert_eq!(sink.out, full[..left], "prefix, nothing repeated");
    }
    let mut sink = Failing {
        out: Vec::new(),
        left: usize::MAX,
    };
    encode_write(&mut sink, &pcm, 48_000, &EncodeOptions::adts()).unwrap();
    assert_eq!(sink.out, full);
}

#[test]
fn m4a_sink_and_bad_pcm_are_typed_before_any_write() {
    let pcm = material(4096);
    let mut sink = Vec::new();
    let e = encode_write(&mut sink, &pcm, 48_000, &EncodeOptions::m4a()).unwrap_err();
    assert!(matches!(
        e,
        AacError::Unsupported(UnsupportedFeature::EncodeM4aStreaming)
    ));
    assert!(sink.is_empty());
    let bad = vec![vec![2.0f32; 4096]];
    assert!(matches!(
        encode_write(&mut sink, &bad, 48_000, &EncodeOptions::adts()).unwrap_err(),
        AacError::InvalidPcm(_)
    ));
    assert!(sink.is_empty());
}
