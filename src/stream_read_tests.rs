//! TASK-56: generic `Read` ADTS/LOAS vs slice/push.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::engine::adts::AdtsHeader;
use crate::engine::decode::StreamDecoder;
use crate::{
    AacError, DecodeOptions, Decoder, Result, UnsupportedFeature, decode_read,
    decode_read_streaming, decode_read_with, decode_with,
};
use std::io::{self, Cursor, Read};

const SINE48: &[u8] = include_bytes!("goldens/sine48.adts");
const HE48: &[u8] = include_bytes!("goldens/he48.adts");
const HE48_LATM: &[u8] = include_bytes!("goldens/he48.latm");
const PS48: &[u8] = include_bytes!("goldens/ps48.adts");
const LATM48: &[u8] = include_bytes!("goldens/latm48.latm");
const LECTURE: &[u8] = include_bytes!("goldens/lecture.m4a");

struct OneByte<'a>(&'a [u8]);

impl Read for OneByte<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.0.is_empty() || buf.is_empty() {
            return Ok(0);
        }
        buf[0] = self.0[0];
        self.0 = &self.0[1..];
        Ok(1)
    }
}

struct Flaky<'a> {
    data: &'a [u8],
    interrupt: bool,
}

impl Read for Flaky<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.interrupt {
            self.interrupt = false;
            return Err(io::Error::from(io::ErrorKind::Interrupted));
        }
        self.interrupt = true;
        if self.data.is_empty() || buf.is_empty() {
            return Ok(0);
        }
        let n = self.data.len().min(buf.len()).min(3);
        buf[..n].copy_from_slice(&self.data[..n]);
        self.data = &self.data[n..];
        Ok(n)
    }
}

struct Boom;

impl Read for Boom {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::other("boom"))
    }
}

struct Repeat<'a> {
    src: &'a [u8],
    copies: usize,
    copy: usize,
    pos: usize,
}

impl Read for Repeat<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.copy >= self.copies {
            return Ok(0);
        }
        let n = (self.src.len() - self.pos).min(buf.len());
        buf[..n].copy_from_slice(&self.src[self.pos..self.pos + n]);
        self.pos += n;
        if self.pos == self.src.len() {
            self.copy += 1;
            self.pos = 0;
        }
        Ok(n)
    }
}

fn two_rdb_sine() -> Vec<u8> {
    let (h, off) = AdtsHeader::parse(SINE48).unwrap();
    let fl = usize::from(h.aac_frame_length);
    let rdb = &SINE48[off..fl];
    let mut probe = StreamDecoder::new();
    probe
        .decode_raw_data_block(2, h.sampling_frequency_index, 48_000, 1, 1, rdb)
        .unwrap();
    let tight = &rdb[..probe.last_rdb_bytes];
    let mut hdr = h;
    hdr.protection_absent = true;
    hdr.number_of_raw_data_blocks_in_frame = 2;
    hdr.aac_frame_length = u16::try_from(7 + 2 * tight.len()).unwrap();
    let mut out = hdr.write().to_vec();
    out.extend_from_slice(tight);
    out.extend_from_slice(tight);
    out
}

fn assert_pcm_eq(tag: &str, a: &crate::DecodedAac, b: &crate::DecodedAac) {
    assert_eq!(a.sample_rate, b.sample_rate, "{tag} rate");
    assert_eq!(a.channels.len(), b.channels.len(), "{tag} ch");
    assert_eq!(a.channels, b.channels, "{tag} pcm");
}

#[test]
fn reader_matches_slice_lc_he_ps_latm() -> Result<()> {
    let speech = DecodeOptions::speech();
    let audio = DecodeOptions::audio();
    for (tag, bytes, opts) in [
        ("sine", SINE48, &speech),
        ("he", HE48, &audio),
        ("he-latm", HE48_LATM, &audio),
        ("ps", PS48, &audio),
        ("latm", LATM48, &speech),
    ] {
        let slice = decode_with(bytes, opts)?;
        let cur = decode_read_with(Cursor::new(bytes), opts)?;
        assert_pcm_eq(tag, &slice, &cur);
        let one = decode_read_with(OneByte(bytes), opts)?;
        assert_pcm_eq(&format!("{tag} 1-byte"), &slice, &one);
        let mut push_planes: Vec<Vec<f32>> = Vec::new();
        let mut dec = Decoder::new(opts.clone());
        dec.feed(bytes, |f| {
            if push_planes.is_empty() {
                push_planes = f.planar.iter().map(|p| p.to_vec()).collect();
            } else {
                for (d, s) in push_planes.iter_mut().zip(f.planar.iter()) {
                    d.extend_from_slice(s);
                }
            }
            Ok(())
        })?;
        let info = dec.finish(|_| Ok(()))?;
        assert_eq!(info.sample_rate, slice.sample_rate, "{tag} push rate");
        assert_eq!(push_planes, slice.channels, "{tag} push pcm");
    }
    Ok(())
}

#[test]
fn two_rdb_reader_matches_slice() -> Result<()> {
    let bytes = two_rdb_sine();
    let opts = DecodeOptions::speech();
    let slice = decode_with(&bytes, &opts)?;
    let got = decode_read_with(Cursor::new(&bytes), &opts)?;
    assert_pcm_eq("2rdb", &slice, &got);
    Ok(())
}

#[test]
fn interrupted_and_io_and_truncated() {
    let opts = DecodeOptions::speech();
    let slice = decode_with(SINE48, &opts).unwrap();
    let got = decode_read_with(
        Flaky {
            data: SINE48,
            interrupt: true,
        },
        &opts,
    )
    .unwrap();
    assert_pcm_eq("flaky", &slice, &got);
    let e = decode_read(Boom).unwrap_err();
    assert!(matches!(e, AacError::Io(_)));
    assert!(std::error::Error::source(&e).is_some());
    let mut trunc = SINE48[..7].to_vec();
    trunc[3] = (trunc[3] & 0xFC) | 0x01;
    trunc[4] = 0x00;
    trunc[5] &= 0x1F;
    let e = decode_read_with(Cursor::new(&trunc), &DecodeOptions::unbounded()).unwrap_err();
    assert!(matches!(e, AacError::Truncated { .. }), "{e:?}");
}

#[test]
fn m4a_reader_is_unsupported() {
    let e = decode_read_with(Cursor::new(LECTURE), &DecodeOptions::speech()).unwrap_err();
    assert!(matches!(
        e,
        AacError::Unsupported(UnsupportedFeature::M4aPush)
    ));
}

#[test]
fn throttled_reader_does_not_collect_input_and_cap_plateaus() -> Result<()> {
    let opts = DecodeOptions::speech();
    let mut d3 = Decoder::new(opts.clone());
    d3.feed_read(
        &mut Repeat {
            src: SINE48,
            copies: 3,
            copy: 0,
            pos: 0,
        },
        |_| Ok(()),
    )?;
    let c3 = d3.input_capacity();
    let mut d30 = Decoder::new(opts);
    d30.feed_read(
        &mut Repeat {
            src: SINE48,
            copies: 30,
            copy: 0,
            pos: 0,
        },
        |_| Ok(()),
    )?;
    let c30 = d30.input_capacity();
    assert!(
        c30 <= c3.saturating_add(8192),
        "resident cap grew with duration: 3→{c3} 30→{c30}"
    );
    assert!(c30 < 64 * 1024, "cap {c30} held far more than one frame");
    Ok(())
}

#[test]
fn decode_read_stays_speech_mono() -> Result<()> {
    let a = decode_read(Cursor::new(SINE48))?;
    let b = decode_with(SINE48, &DecodeOptions::speech())?;
    assert_eq!(a.channels.len(), 1);
    assert_pcm_eq("speech", &a, &b);
    let mut n = 0usize;
    let info = decode_read_streaming(Cursor::new(SINE48), &DecodeOptions::speech(), |f| {
        n += f.samples;
        Ok(())
    })?;
    assert_eq!(n as u64, info.samples);
    Ok(())
}
