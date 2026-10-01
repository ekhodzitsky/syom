//! TASK-64 lab prototype: minimal bounded fMP4 demux feeding syom's raw-AU
//! API (`Decoder::from_asc` + `decode_au`). Exists to (a) prove the bounded
//! contract is sufficient for real ffmpeg-generated fMP4 and (b) measure the
//! actual code size of the fragment semantics (moof/mfhd/traf/tfhd/tfdt/trun
//! + mvex/trex + init moov reuse) so the go/no-go rests on a measured delta.
//!
//! Deliberately one-shot (whole file in memory); the incremental-delivery
//! shape is a spec item, not needed for the measurement.
//!
//! Usage: fmp4_bridge <in.mp4> <out.s16le>

use std::fmt;

#[derive(Debug)]
struct Err_(String);
impl fmt::Display for Err_ {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Err_ {}
type Res<T> = Result<T, Box<dyn std::error::Error>>;

fn err<T>(msg: impl Into<String>) -> Res<T> {
    Err(Box::new(Err_(msg.into())))
}

fn u32be(b: &[u8], o: usize) -> Res<u32> {
    Ok(u32::from_be_bytes(
        b.get(o..o + 4).ok_or("truncated u32")?.try_into()?,
    ))
}
fn u64be(b: &[u8], o: usize) -> Res<u64> {
    Ok(u64::from_be_bytes(
        b.get(o..o + 8).ok_or("truncated u64")?.try_into()?,
    ))
}

struct Box_ {
    pos: usize,
    body: usize,
    end: usize,
    typ: [u8; 4],
}

fn read_box(d: &[u8], pos: usize, end: usize) -> Res<Option<Box_>> {
    if pos + 8 > end {
        return Ok(None);
    }
    let size32 = u32be(d, pos)? as u64;
    let typ: [u8; 4] = d[pos + 4..pos + 8].try_into()?;
    let (size, head) = match size32 {
        1 => (u64be(d, pos + 8)?, 16usize),
        0 => ((end - pos) as u64, 8usize),
        s => (s as u64, 8usize),
    };
    if (size as usize) < head || pos + size as usize > end {
        return err(format!("bad box at {pos}"));
    }
    Ok(Some(Box_ {
        pos,
        body: pos + head,
        end: pos + size as usize,
        typ,
    }))
}

fn children(d: &[u8], b: &Box_) -> Res<Vec<Box_>> {
    let mut out = Vec::new();
    let mut pos = b.body;
    while let Some(c) = read_box(d, pos, b.end)? {
        pos = c.end;
        out.push(c);
    }
    Ok(out)
}

fn find<'a>(d: &[u8], b: &Box_, want: &[u8; 4]) -> Res<Option<Box_>> {
    Ok(children(d, b)?.into_iter().find(|c| &c.typ == want))
}

/// ASC from stsd/mp4a/esds (same descriptor walk as src/isomp4.rs).
fn asc_from_stsd(d: &[u8], stsd: &Box_) -> Res<Vec<u8>> {
    let entries = u32be(d, stsd.body + 4)? as usize;
    if entries != 1 {
        return err("stsd entry count != 1 (sample-description switch unsupported)");
    }
    let e = read_box(d, stsd.body + 8, stsd.end)?.ok_or("no stsd entry")?;
    if &e.typ == b"enca" {
        return err("enca: encrypted audio unsupported");
    }
    if &e.typ != b"mp4a" {
        return err("non-mp4a entry unsupported");
    }
    let ver = u16::from_be_bytes(d[e.body + 8..e.body + 10].try_into()?) as usize;
    let fixed = 28 + if ver == 1 { 16 } else if ver == 2 { 36 } else { 0 };
    let mut pos = e.body + fixed;
    while let Some(c) = read_box(d, pos, e.end)? {
        pos = c.end;
        if &c.typ == b"esds" {
            return parse_esds(&d[c.body + 4..c.end]);
        }
    }
    err("no esds")
}

fn desc_len(d: &[u8], p: usize) -> Res<(usize, usize)> {
    let mut len = 0;
    for i in 0..4 {
        let b = *d.get(p + i).ok_or("desc len")?;
        len = (len << 7) | (b & 0x7f) as usize;
        if b & 0x80 == 0 {
            return Ok((len, p + i + 1));
        }
    }
    err("desc len > 4")
}

fn parse_esds(d: &[u8]) -> Res<Vec<u8>> {
    if d[0] != 0x03 {
        return err("no ES_Descriptor");
    }
    let (ln, mut p) = desc_len(d, 1)?;
    let es_end = p + ln;
    let flags = d[p + 2];
    p += 3;
    if flags & 0x80 != 0 {
        p += 2;
    }
    if flags & 0x40 != 0 {
        p += 1 + d[p] as usize;
    }
    if flags & 0x20 != 0 {
        p += 2;
    }
    if d[p] != 0x04 {
        return err("no DecoderConfigDescriptor");
    }
    let (ln, p2) = desc_len(d, p + 1)?;
    let dc_end = p2 + ln;
    let mut p = p2 + 13;
    while p < dc_end {
        let tag = d[p];
        let (ln, q) = desc_len(d, p + 1)?;
        if tag == 0x05 {
            return Ok(d[q..q + ln].to_vec());
        }
        p = q + ln;
    }
    let _ = es_end;
    err("no DecoderSpecificInfo")
}

#[derive(Default, Clone, Copy)]
struct Defaults {
    duration: u32,
    size: u32,
    flags: u32,
    desc: u32,
}

fn main() -> Res<()> {
    let path = std::env::args().nth(1).ok_or("usage: fmp4_bridge in.mp4 out.s16")?;
    let out = std::env::args().nth(2).ok_or("usage")?;
    let d = std::fs::read(path)?;

    // --- init: moov (empty sample table) + mvex/trex defaults --------------
    let mut asc = None;
    let mut timescale = 0u32;
    let mut media_time = 0i64;
    let mut trex = Defaults::default();
    let mut track_id = 0u32;
    let mut sound_tracks = 0u32;
    let mut pos = 0usize;
    let mut moofs = Vec::new();
    while let Some(b) = read_box(&d, pos, d.len())? {
        pos = b.end;
        match &b.typ {
            b"moov" => {
                for c in children(&d, &b)? {
                    match &c.typ {
                        b"mvex" => {
                            for t in children(&d, &c)? {
                                if &t.typ == b"trex" {
                                    trex = Defaults {
                                        desc: u32be(&d, t.body + 8)?,
                                        duration: u32be(&d, t.body + 12)?,
                                        size: u32be(&d, t.body + 16)?,
                                        flags: u32be(&d, t.body + 20)?,
                                    };
                                }
                            }
                        }
                        b"trak" => {
                            let mdia = find(&d, &c, b"mdia")?.ok_or("no mdia")?;
                            let hdlr = find(&d, &mdia, b"hdlr")?.ok_or("no hdlr")?;
                            if &d[hdlr.body + 8..hdlr.body + 12] != b"soun" {
                                continue;
                            }
                            sound_tracks += 1;
                            if sound_tracks > 1 {
                                return err("more than one sound track: unsupported");
                            }
                            track_id = u32be(
                                &d,
                                find(&d, &c, b"tkhd")?.ok_or("no tkhd")?.body + 12,
                            )?;
                            let mdhd = find(&d, &mdia, b"mdhd")?.ok_or("no mdhd")?;
                            let v = d[mdhd.body];
                            timescale = u32be(&d, mdhd.body + if v == 1 { 20 } else { 12 })?;
                            let minf = find(&d, &mdia, b"minf")?.ok_or("no minf")?;
                            let stbl = find(&d, &minf, b"stbl")?.ok_or("no stbl")?;
                            asc = Some(asc_from_stsd(
                                &d,
                                &find(&d, &stbl, b"stsd")?.ok_or("no stsd")?,
                            )?);
                            if let Some(edts) = find(&d, &c, b"edts")? {
                                if let Some(elst) = find(&d, &edts, b"elst")? {
                                    let v = d[elst.body];
                                    let n = u32be(&d, elst.body + 4)?;
                                    if n > 1 {
                                        return err("multiple elst entries unsupported");
                                    }
                                    if n == 1 {
                                        media_time = if v == 1 {
                                            u64be(&d, elst.body + 16)? as i64
                                        } else {
                                            u32be(&d, elst.body + 12)? as i32 as i64
                                        };
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            b"moof" => moofs.push(b),
            _ => {}
        }
    }
    let asc = asc.ok_or("no AAC sound track")?;
    if moofs.is_empty() {
        return err("no fragments (not an fMP4?)");
    }

    // --- fragments: resolve each trun sample to (offset, size, dts) --------
    let mut aus: Vec<(usize, usize)> = Vec::new();
    let mut prev_end: Option<usize> = None;
    let mut dts: u64 = 0;
    let mut last_seq = 0u32;
    for moof in &moofs {
        let mut tfhd = None;
        for traf in children(&d, moof)? {
            match &traf.typ {
                b"mfhd" => {
                    let seq = u32be(&d, traf.body + 4)?;
                    if seq != last_seq + 1 {
                        return err(format!("mfhd sequence gap: {last_seq} -> {seq}"));
                    }
                    last_seq = seq;
                }
                b"traf" => {
                    let t = find(&d, &traf, b"tfhd")?.ok_or("traf without tfhd")?;
                    if tfhd.is_some() {
                        return err("multiple traf per moof: unsupported");
                    }
                    tfhd = Some(t);
                }
                _ => {}
            }
        }
        let t = tfhd.ok_or("moof without traf")?;
        let mut o = t.body + 4;
        let tid = u32be(&d, o)?;
        o += 4;
        if tid != track_id {
            return err("traf for a foreign track id");
        }
        let flags = u32be(&d, t.body)? & 0xFF_FFFF;
        let mut base = None;
        let mut def = trex;
        if flags & 0x1 != 0 {
            base = Some(u64be(&d, o)? as usize);
            o += 8;
        }
        if flags & 0x2 != 0 {
            def.desc = u32be(&d, o)?;
            o += 4;
        }
        if flags & 0x8 != 0 {
            def.duration = u32be(&d, o)?;
            o += 4;
        }
        if flags & 0x10 != 0 {
            def.size = u32be(&d, o)?;
            o += 4;
        }
        if flags & 0x20 != 0 {
            def.flags = u32be(&d, o)?;
            o += 4;
        }
        let base = match (base, flags & 0x020000 != 0, prev_end) {
            (Some(b), _, _) => b,
            (None, true, _) => moof.pos,
            (None, false, Some(e)) => e,
            (None, false, None) => moof.pos,
        };
        let traf = find(&d, moof, b"traf")?.ok_or("no traf")?;
        if let Some(tfdt) = find(&d, &traf, b"tfdt")? {
            let v = d[tfdt.body];
            dts = if v == 1 {
                u64be(&d, tfdt.body + 4)?
            } else {
                u32be(&d, tfdt.body + 4)? as u64
            };
        }
        for trun in children(&d, &traf)? {
            if &trun.typ != b"trun" {
                continue;
            }
            let tflags = u32be(&d, trun.body)? & 0xFF_FFFF;
            let count = u32be(&d, trun.body + 4)? as usize;
            let mut o = trun.body + 8;
            let data_off = if tflags & 1 != 0 {
                let v = i32::from_be_bytes(d[o..o + 4].try_into()?) as i64;
                o += 4;
                v
            } else {
                0
            };
            if tflags & 4 != 0 {
                o += 4; // first_sample_flags
            }
            let mut off = (base as i64 + data_off) as usize;
            for _ in 0..count {
                let dur = if tflags & 0x100 != 0 {
                    let v = u32be(&d, o)?;
                    o += 4;
                    v
                } else {
                    def.duration
                };
                let size = if tflags & 0x200 != 0 {
                    let v = u32be(&d, o)?;
                    o += 4;
                    v
                } else {
                    def.size
                };
                if tflags & 0x400 != 0 {
                    o += 4;
                }
                if tflags & 0x800 != 0 {
                    o += 4;
                }
                let end = off.checked_add(size as usize).ok_or("sample overflow")?;
                if end > d.len() {
                    return err("sample out of file");
                }
                aus.push((off, size as usize));
                off = end;
                dts += dur as u64;
            }
        }
        if let Some((o, s)) = aus.last() {
            prev_end = Some(o + s);
        }
    }

    // --- decode through the existing raw-AU API -----------------------------
    let mut dec = syom::Decoder::from_asc(&asc, syom::DecodeOptions::unbounded())?;
    // interleaved s16 output, appended frame by frame
    let mut s16: Vec<u8> = Vec::new();
    let mut ch = 0usize;
    let mut frames = 0usize;
    for (off, size) in &aus {
        dec.decode_au(&d[*off..*off + *size], |f| {
            if ch == 0 {
                ch = f.planar.len();
            }
            frames += f.samples;
            for i in 0..f.samples {
                for p in f.planar {
                    let v = (p[i] * 32767.0).clamp(-32768.0, 32767.0);
                    s16.extend_from_slice(&(v as i16).to_le_bytes());
                }
            }
            Ok(())
        })?;
    }
    std::fs::write(out, &s16)?;
    eprintln!(
        "decoded {} AUs, {} frames, {} ch, timescale {}, elst media_time {}",
        aus.len(),
        frames,
        ch,
        timescale,
        media_time
    );
    Ok(())
}
