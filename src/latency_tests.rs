//! TASK-84: adapter buffering delay. Codec algorithmic delay (priming,
//! opt-in lookahead, HE analysis) is separate and unchanged; these tests
//! pin that the push / reader adapters add nothing on top of it: a frame
//! is handed to the callback on the very byte (decode) or sample (encode)
//! that completes it.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::{DecodeOptions, Decoder, EncodeOptions, Encoder};
use std::io::Read;

/// ADTS frame end offsets from the 13-bit `aac_frame_length`.
fn adts_frame_ends(adts: &[u8], n: usize) -> Vec<usize> {
    let mut ends = Vec::new();
    let mut at = 0usize;
    while ends.len() < n && at + 7 <= adts.len() {
        let len = ((usize::from(adts[at + 3]) & 3) << 11)
            | (usize::from(adts[at + 4]) << 3)
            | (usize::from(adts[at + 5]) >> 5);
        at += len;
        ends.push(at);
    }
    ends
}

/// LOAS frame end offsets (`0x2B7` sync + 13-bit length + 3 header bytes).
fn loas_frame_ends(loas: &[u8], n: usize) -> Vec<usize> {
    let mut ends = Vec::new();
    let mut at = 0usize;
    while ends.len() < n && at + 3 <= loas.len() {
        let len = ((usize::from(loas[at + 1]) & 0x1F) << 8) | usize::from(loas[at + 2]);
        at += 3 + len;
        ends.push(at);
    }
    ends
}

/// Bytes fed (one at a time) when each of the first `n` callbacks fired.
fn bytes_at_callbacks(stream: &[u8], n: usize) -> Vec<usize> {
    let mut dec = Decoder::new(DecodeOptions::audio());
    let mut seen = Vec::new();
    for (i, b) in stream.iter().enumerate() {
        dec.feed(std::slice::from_ref(b), |_| {
            seen.push(i + 1);
            Ok(())
        })
        .unwrap();
        if seen.len() >= n {
            break;
        }
    }
    seen.truncate(n);
    seen
}

#[test]
fn push_decode_emits_on_the_byte_that_completes_each_frame() {
    for (name, stream) in [
        ("lc", &include_bytes!("goldens/sine48.adts")[..]),
        ("he", &include_bytes!("goldens/he48.adts")[..]),
        ("ps", &include_bytes!("goldens/ps48.adts")[..]),
        ("mc71", &include_bytes!("goldens/mc71.adts")[..]),
    ] {
        assert_eq!(
            bytes_at_callbacks(stream, 5),
            adts_frame_ends(stream, 5),
            "{name}: no adapter buffering beyond the frame boundary"
        );
    }
    let latm = &include_bytes!("goldens/latm48.latm")[..];
    assert_eq!(
        bytes_at_callbacks(latm, 5),
        loas_frame_ends(latm, 5),
        "latm"
    );
}

/// A reader that hands out `step` bytes per call and counts what it gave.
struct Trickle<'a> {
    data: &'a [u8],
    step: usize,
    given: std::rc::Rc<std::cell::Cell<usize>>,
}
impl Read for Trickle<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.step.min(buf.len()).min(self.data.len());
        buf[..n].copy_from_slice(&self.data[..n]);
        self.data = &self.data[n..];
        self.given.set(self.given.get() + n);
        Ok(n)
    }
}

#[test]
fn reader_adapter_never_reads_past_the_packet_that_completes_a_frame() {
    let stream = &include_bytes!("goldens/sine48.adts")[..];
    let ends = adts_frame_ends(stream, 5);
    for step in [1usize, 188, 1500] {
        let given = std::rc::Rc::new(std::cell::Cell::new(0usize));
        let reader = Trickle {
            data: stream,
            step,
            given: given.clone(),
        };
        let mut at = Vec::new();
        crate::decode_read_streaming(reader, &DecodeOptions::audio(), |_| {
            at.push(given.get());
            Ok(())
        })
        .unwrap();
        for (k, &end) in ends.iter().enumerate() {
            let packet_end = end.div_ceil(step) * step;
            assert_eq!(
                at[k],
                packet_end.min(stream.len()),
                "step {step} frame {k}: emitted within the packet that completes it"
            );
        }
    }
}

/// Samples fed (one at a time) when each of the first `n` frames came out.
fn samples_at_callbacks(opts: &EncodeOptions, n: usize) -> Vec<usize> {
    let mut enc = Encoder::new(48_000, 1, opts).unwrap();
    let mut seen = Vec::new();
    for i in 0..16_384usize {
        let s = [0.25 * ((i as f32) * 0.05).sin()];
        enc.feed(&[&s[..]], |_| {
            seen.push(i + 1);
            Ok(())
        })
        .unwrap();
        if seen.len() >= n {
            break;
        }
    }
    seen.truncate(n);
    seen
}

#[test]
fn push_encode_emits_on_the_sample_that_completes_each_frame() {
    // Causal LC: frame k leaves on sample 1024·(k+1).
    assert_eq!(
        samples_at_callbacks(&EncodeOptions::adts(), 4),
        vec![1024, 2048, 3072, 4096]
    );
    assert_eq!(
        samples_at_callbacks(&EncodeOptions::adts().with_quality(5), 2),
        vec![1024, 2048]
    );
    // Opt-in lookahead: exactly one frame of declared algorithmic hold.
    assert_eq!(
        samples_at_callbacks(&EncodeOptions::adts().with_lookahead(true), 3),
        vec![2048, 3072, 4096]
    );
    // HE v1: one AU per 2048 output samples, no extra adapter hold.
    let he = samples_at_callbacks(&EncodeOptions::low_rate(), 3);
    assert_eq!(he[1] - he[0], 2048);
    assert_eq!(he[2] - he[1], 2048);
    assert!(he[0] <= 2 * 2048, "first HE AU within two AU spans: {he:?}");
}

/// Lab print (`cargo test --release --lib -- --ignored latency_report --nocapture`).
#[test]
#[ignore = "prints wall numbers for lab/baseline/LATENCY.md"]
fn latency_report() {
    println!(
        "HE first AUs at samples: {:?}",
        samples_at_callbacks(&EncodeOptions::low_rate(), 3)
    );
    let pcm: Vec<f32> = (0..480_000)
        .map(|i| 0.25 * ((i as f32) * 0.05).sin())
        .collect();
    for chunk in [1024usize, 4096, 48_000, 480_000] {
        let mut best = f64::MAX;
        for _ in 0..7 {
            let mut enc = Encoder::new(48_000, 1, &EncodeOptions::adts()).unwrap();
            let t = std::time::Instant::now();
            let mut first = None;
            enc.feed(&[&pcm[..chunk]], |_| {
                first.get_or_insert(t.elapsed().as_secs_f64() * 1e6);
                Ok(())
            })
            .unwrap();
            best = best.min(first.unwrap());
        }
        println!("encode feed chunk {chunk}: first callback after {best:.1} us");
    }
    let stream = &include_bytes!("goldens/sine48.adts")[..];
    let mut best = f64::MAX;
    for _ in 0..7 {
        let mut dec = Decoder::new(DecodeOptions::audio());
        let t = std::time::Instant::now();
        let mut first = None;
        dec.feed(stream, |_| {
            first.get_or_insert(t.elapsed().as_secs_f64() * 1e6);
            Ok(())
        })
        .unwrap();
        best = best.min(first.unwrap());
    }
    println!(
        "decode feed whole stream ({} B): first callback after {best:.1} us",
        stream.len()
    );
}
