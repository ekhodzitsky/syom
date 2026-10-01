//! TASK-130: AAC-LD LOAS streaming, failure/reset, and M4A `elst` trim.
//!
//! The M4A is muxed here from the FDK LOAS payloads. One-shot, seek and
//! probe are compared to the LOAS decode of the same access units, so the
//! container cannot invent a sample.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::Cursor;

use super::stream_tests::{assert_eq_collected, collect_into, collect_push};
use super::{
    AacError, DecodeOptions, Decoder, M4aSeek, ProbeContainer, ProbeDuration, ProbeProfile,
    ProbeTrim, Result, decode_with, probe,
};

const FRAME: u64 = 512;
const LD_M: &[u8] = include_bytes!("goldens/ld64m.loas");
const LD_S: &[u8] = include_bytes!("goldens/ld64mus.loas");

/// AOT 23, 48 kHz, GA flags 0, `epConfig` 0.
fn ld_asc(channels: u8) -> Vec<u8> {
    let mut w = crate::engine::bits::BitWriter::new();
    w.write(23, 5);
    w.write(3, 4);
    w.write(u32::from(channels), 4);
    w.write(0, 3);
    w.write(0, 2);
    w.finish()
}

fn loas_payloads(loas: &[u8]) -> Vec<Vec<u8>> {
    let mut pos = 0usize;
    let mut mux = None;
    let mut out = Vec::new();
    while pos + 3 <= loas.len() {
        let v = (u32::from(loas[pos]) << 16)
            | (u32::from(loas[pos + 1]) << 8)
            | u32::from(loas[pos + 2]);
        let ln = (v & 0x1FFF) as usize;
        let mut br = crate::engine::bits::BitReader::new(&loas[pos + 3..pos + 3 + ln]);
        if !br.read_bit().unwrap() {
            mux = Some(crate::engine::latm::MuxCfg::parse(&mut br).unwrap());
        }
        out.push(crate::engine::latm::read_payload(&mut br, mux.as_ref().unwrap()).unwrap());
        pos += 3 + ln;
    }
    out
}

fn mux_ld(loas: &[u8], channels: usize, priming: u64, valid: u64) -> Vec<u8> {
    let payloads = loas_payloads(loas);
    crate::m4a_write::mux_aac(
        &payloads,
        &ld_asc(u8::try_from(channels).unwrap()),
        channels,
        48_000,
        valid,
        priming,
        FRAME as u32,
    )
    .unwrap()
}

#[test]
fn ld_loas_chunked_matches_one_shot_and_reset_repeats_it() -> Result<()> {
    let opts = DecodeOptions::unbounded();
    let one = decode_with(LD_M, &opts)?;
    let chunked = collect_push(LD_M, &opts, 1)?;
    assert_eq_collected(&one, &chunked, "1-byte chunks");

    let mut dec = Decoder::new(opts.clone());
    assert!(dec.feed(LD_M, |_| Err(AacError::decode("boom"))).is_err());
    assert!(dec.is_failed());
    let sticky = dec.feed(LD_M, |_| Ok(())).unwrap_err();
    assert!(sticky.to_string().contains("reset"), "{sticky}");
    dec.reset();
    assert!(!dec.is_failed());
    let again = collect_into(&mut dec, LD_M)?;
    assert_eq_collected(&one, &again, "reset after callback failure");

    let mut dec = Decoder::new(opts);
    let _ = collect_into(&mut dec, LD_S)?;
    assert!(dec.is_finished());
    assert!(dec.feed(LD_M, |_| Ok(())).is_err());
    dec.reset();
    let switched = collect_into(&mut dec, LD_M)?;
    assert_eq_collected(&one, &switched, "stereo session then reset to mono");
    Ok(())
}

#[test]
fn ld_m4a_duration_trim_and_seek_match_loas() -> Result<()> {
    let opts = DecodeOptions::unbounded();
    let loas = decode_with(LD_M, &opts)?;
    let n = loas.channels[0].len() as u64;
    assert_eq!(n % FRAME, 0);
    let frames = n / FRAME;

    let whole = mux_ld(LD_M, 1, 0, n);
    let got = decode_with(&whole, &opts)?;
    assert_eq!(got.channels, loas.channels);
    assert_eq!(got.priming, Some(0));
    assert_eq!(got.remainder, Some(0));
    let p = probe(&whole)?;
    assert_eq!(p.container, ProbeContainer::M4a);
    assert_eq!(p.profile, ProbeProfile::Ld);
    assert_eq!(p.meta.core_rate, 48_000);
    assert!(matches!(p.duration, ProbeDuration::Exact { samples } if samples == n));
    assert!(matches!(
        p.trim,
        ProbeTrim::Exact {
            priming: 0,
            remainder: 0
        }
    ));

    // Drop the encoder-delay frame and leave one unplayed frame at the tail.
    let priming = FRAME;
    let valid = n - 2 * FRAME;
    let trimmed = mux_ld(LD_M, 1, priming, valid);
    let got = decode_with(&trimmed, &opts)?;
    assert_eq!(got.channels[0].len() as u64, valid);
    assert_eq!(
        got.channels[0],
        loas.channels[0][priming as usize..][..valid as usize]
    );
    assert_eq!(got.priming, Some(priming));
    assert_eq!(got.remainder, Some(FRAME));
    let p = probe(&trimmed)?;
    assert!(matches!(p.duration, ProbeDuration::Exact { samples } if samples == valid));
    assert!(matches!(
        p.trim,
        ProbeTrim::Exact {
            priming: 512,
            remainder: 512
        }
    ));

    let mut src = M4aSeek::open(Cursor::new(whole), opts)?;
    assert_eq!(src.presentation_len(), n);
    assert_eq!(src.preroll_aus(), 1);
    let at = 4 * FRAME;
    assert_eq!(src.seek(at as i64)?, at);
    assert_eq!(src.last_seek_aus(), 1, "one overlap frame, not the LC pair");
    let mut pcm = Vec::new();
    src.decode(|f| {
        assert_eq!(f.samples, FRAME as usize);
        pcm.extend_from_slice(f.planar[0]);
        Ok(())
    })?;
    assert_eq!(pcm, loas.channels[0][at as usize..]);

    let stereo = decode_with(LD_S, &DecodeOptions::unbounded())?;
    let m4a = mux_ld(LD_S, 2, 0, stereo.channels[0].len() as u64);
    let got = decode_with(&m4a, &DecodeOptions::unbounded())?;
    assert_eq!(got.channels, stereo.channels);
    assert!(frames > 2, "trim needs more than the priming and the tail");
    Ok(())
}
