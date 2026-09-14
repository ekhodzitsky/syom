//! TASK-57: seekable M4A without loading `mdat` in one read.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::{
    AacError, DecodeOptions, Result, decode_seek, decode_seek_streaming, decode_seek_with,
    decode_with, read, read_with,
};
use std::io::{self, Cursor, Read, Seek, SeekFrom};

const HE48: &[u8] = include_bytes!("goldens/he48.m4a");
const PS48: &[u8] = include_bytes!("goldens/ps48.m4a");
const SINE441: &[u8] = include_bytes!("goldens/sine441.m4a");
const LECTURE: &[u8] = include_bytes!("goldens/lecture.m4a");
const MC51: &[u8] = include_bytes!("goldens/mc51.m4a");

struct Probe<R> {
    inner: R,
    read_bytes: u64,
    max_read: usize,
    seeks: u32,
}

impl<R: Read> Read for Probe<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.read_bytes += n as u64;
        self.max_read = self.max_read.max(n);
        Ok(n)
    }
}

impl<R: Seek> Seek for Probe<R> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.seeks += 1;
        self.inner.seek(pos)
    }
}

fn m4a_cases() -> [(&'static [u8], &'static str); 5] {
    [
        (HE48, "he48"),
        (PS48, "ps48"),
        (SINE441, "sine441"),
        (LECTURE, "lecture"),
        (MC51, "mc51"),
    ]
}

fn top_boxes(data: &[u8]) -> Vec<([u8; 4], usize, usize)> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos + 8 <= data.len() {
        let sz = u32::from_be_bytes(data[pos..pos + 4].try_into().unwrap()) as usize;
        let mut typ = [0u8; 4];
        typ.copy_from_slice(&data[pos + 4..pos + 8]);
        if sz < 8 || pos + sz > data.len() {
            break;
        }
        out.push((typ, pos, sz));
        pos += sz;
    }
    out
}

fn moov_before_mdat(data: &[u8]) -> Vec<u8> {
    let boxes = top_boxes(data);
    let old_mdat = boxes
        .iter()
        .find(|(t, _, _)| t == b"mdat")
        .map(|(_, pos, _)| *pos)
        .unwrap();
    let mut out = Vec::new();
    for &(typ, pos, sz) in &boxes {
        if &typ == b"mdat" {
            continue;
        }
        out.extend_from_slice(&data[pos..pos + sz]);
    }
    let new_mdat = out.len();
    for &(typ, pos, sz) in &boxes {
        if &typ == b"mdat" {
            out.extend_from_slice(&data[pos..pos + sz]);
        }
    }
    let delta = new_mdat as i64 - old_mdat as i64;
    shift_chunk_offsets(&mut out, delta);
    out
}

fn shift_chunk_offsets(data: &mut [u8], delta: i64) {
    let mut pos = 0usize;
    while pos + 8 <= data.len() {
        let sz = u32::from_be_bytes(data[pos..pos + 4].try_into().unwrap()) as usize;
        if sz < 8 || pos + sz > data.len() {
            pos += 1;
            continue;
        }
        let mut typ = [0u8; 4];
        typ.copy_from_slice(&data[pos + 4..pos + 8]);
        if &typ == b"stco" && sz >= 16 {
            let n = u32::from_be_bytes(data[pos + 12..pos + 16].try_into().unwrap()) as usize;
            for i in 0..n {
                let o = pos + 16 + i * 4;
                if o + 4 > pos + sz {
                    break;
                }
                let v = u32::from_be_bytes(data[o..o + 4].try_into().unwrap());
                let n = (i64::from(v) + delta).clamp(0, i64::from(u32::MAX)) as u32;
                data[o..o + 4].copy_from_slice(&n.to_be_bytes());
            }
        }
        if &typ == b"co64" && sz >= 16 {
            let n = u32::from_be_bytes(data[pos + 12..pos + 16].try_into().unwrap()) as usize;
            for i in 0..n {
                let o = pos + 16 + i * 8;
                if o + 8 > pos + sz {
                    break;
                }
                let v = u64::from_be_bytes(data[o..o + 8].try_into().unwrap());
                let n = v.saturating_add_signed(delta);
                data[o..o + 8].copy_from_slice(&n.to_be_bytes());
            }
        }
        pos += 1;
    }
}

fn mdat_size(data: &[u8]) -> usize {
    top_boxes(data)
        .into_iter()
        .find(|(t, _, _)| t == b"mdat")
        .map(|(_, _, sz)| sz)
        .unwrap_or(0)
}

#[test]
fn seek_matches_slice_mdat_first_and_moov_first() -> Result<()> {
    for (data, label) in m4a_cases() {
        for opts in [DecodeOptions::speech(), DecodeOptions::audio()] {
            let slice = decode_with(data, &opts)?;
            let seek = decode_seek_with(Cursor::new(data), &opts)?;
            assert_eq!(slice.sample_rate, seek.sample_rate, "{label} rate");
            assert_eq!(slice.channels, seek.channels, "{label} pcm");
            let reordered = moov_before_mdat(data);
            let first = decode_seek_with(Cursor::new(&reordered), &opts)?;
            assert_eq!(slice.channels, first.channels, "{label} moov-first pcm");
        }
    }
    Ok(())
}

#[test]
fn probe_skips_mdat_as_one_read_and_plateaus() -> Result<()> {
    let data = LECTURE;
    let mdat = mdat_size(data);
    let mut probe = Probe {
        inner: Cursor::new(data),
        read_bytes: 0,
        max_read: 0,
        seeks: 0,
    };
    let got = decode_seek_with(&mut probe, &DecodeOptions::speech())?;
    let slice = decode_with(data, &DecodeOptions::speech())?;
    assert_eq!(got.channels, slice.channels);
    assert!(
        probe.max_read < mdat,
        "one read {0} slurped mdat {mdat}",
        probe.max_read
    );
    assert!(
        probe.max_read < data.len(),
        "one read loaded the whole file"
    );
    assert!(
        probe.seeks >= 3,
        "expected size + skip + samples, got {}",
        probe.seeks
    );
    Ok(())
}

#[test]
fn out_of_file_sample_is_rejected_before_decode() {
    let mut data = moov_before_mdat(SINE441);
    let track = crate::isomp4::parse_aac_track(&data).expect("index");
    let (off, len) = track.frames[0];
    let cut = (off + u64::from(len) / 2) as usize;
    data.truncate(cut.max(8));
    let e = decode_seek_with(Cursor::new(&data), &DecodeOptions::speech()).unwrap_err();
    assert!(
        matches!(
            e,
            AacError::NotAac | AacError::Format(_) | AacError::Truncated { .. }
        ),
        "{e:?}"
    );
}

#[test]
fn huge_moov_hits_metadata_budget() {
    let tiny = DecodeOptions::speech().with_memory(crate::MemoryBudgets {
        max_metadata_bytes: 32,
        ..crate::MemoryBudgets::default()
    });
    let e = decode_seek_with(Cursor::new(LECTURE), &tiny).unwrap_err();
    assert!(matches!(e, AacError::Limit { .. }), "{e:?}");
}

#[test]
fn reader_io_keeps_source() {
    struct Boom;
    impl Read for Boom {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("boom"))
        }
    }
    impl Seek for Boom {
        fn seek(&mut self, _: SeekFrom) -> io::Result<u64> {
            Err(io::Error::other("boom"))
        }
    }
    let e = decode_seek(Boom).unwrap_err();
    assert!(matches!(e, AacError::Io(_)), "{e:?}");
    assert!(std::error::Error::source(&e).is_some());
}

#[test]
fn read_path_uses_seek_for_m4a() -> Result<()> {
    let p = std::env::temp_dir().join(format!(
        "syom-m4a-seek-{}-{}",
        std::process::id(),
        SINE441.len()
    ));
    std::fs::write(&p, SINE441)?;
    let a = read(&p)?;
    let b = read_with(&p, &DecodeOptions::speech())?;
    let c = decode_with(SINE441, &DecodeOptions::speech())?;
    assert_eq!(a.channels, c.channels);
    assert_eq!(b.channels, c.channels);
    let _ = std::fs::remove_file(&p);
    Ok(())
}

#[test]
fn decode_seek_stays_speech_mono() -> Result<()> {
    let a = decode_seek(Cursor::new(SINE441))?;
    let b = decode_with(SINE441, &DecodeOptions::speech())?;
    assert_eq!(a.channels.len(), 1);
    assert_eq!(a.channels, b.channels);
    let mut n = 0usize;
    let info = decode_seek_streaming(Cursor::new(SINE441), &DecodeOptions::speech(), |f| {
        n += f.samples;
        Ok(())
    })?;
    assert_eq!(n as u64, info.samples);
    Ok(())
}
