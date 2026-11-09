//! Metadata-only AAC track selection; sample counts never allocate an index.
use super::{
    BOX_MOOF, BOX_MOOV, BOX_MVEX, BOX_TRAK, BoxHdr, BoxIter, fence_depth, parse_elst,
    parse_mdhd_duration, parse_mdhd_timescale, parse_mvhd_timescale, parse_stsd_asc, read_u32,
};
use crate::budgets::MemoryBudgets;
use crate::error::{AacError, Result};

/// ASC + container timing without a sample-table index (TASK-54 probe).
#[derive(Debug)]
pub(crate) struct AacMeta {
    pub asc: Vec<u8>,
    pub edit_start: u64,
    pub edit_duration: u64,
    pub movie_timescale: u32,
    pub media_timescale: u32,
    pub media_duration: u64,
    pub has_elst: bool,
    pub coded_frames: Option<u64>,
}

struct SoundMeta {
    asc: Vec<u8>,
    timescale: u32,
    duration: u64,
    coded_frames: Option<u64>,
}

fn parse_stbl_meta(data: &[u8], stbl: BoxHdr) -> Result<(Vec<u8>, Option<u64>)> {
    let mut asc = None;
    let mut frames = None;
    let mut size_tables = 0;
    for child in BoxIter::new(data, stbl.content_start, stbl.content_end) {
        let (hdr, typ) = child?;
        let body = &data[hdr.content_start..hdr.content_end];
        match &typ {
            b"stsd" => asc = Some(parse_stsd_asc(body)?),
            b"stsz" => {
                size_tables += 1;
                frames = stsz_count(body);
            }
            _ => {}
        }
    }
    let asc =
        asc.ok_or_else(|| AacError::format("isomp4: no AAC sample description (stsd/mp4a)"))?;
    Ok((asc, if size_tables == 1 { frames } else { None }))
}

/// Read only a structurally complete size-table declaration, never its index.
fn stsz_count(data: &[u8]) -> Option<u64> {
    if read_u32(data, 0).ok()? != 0 {
        return None;
    }
    let default_size = read_u32(data, 4).ok()?;
    let count = u64::from(read_u32(data, 8).ok()?);
    let required = if default_size == 0 {
        12 + count * 4
    } else {
        12
    };
    (count > 0 && required <= data.len() as u64).then_some(count)
}

fn parse_mdia_meta(data: &[u8], mdia: BoxHdr, mem: &MemoryBudgets) -> Result<Option<SoundMeta>> {
    fence_depth(mem, 3)?;
    let mut is_sound = false;
    let mut asc = None;
    let mut timescale = 0u32;
    let mut media_duration = 0u64;
    for child in BoxIter::new(data, mdia.content_start, mdia.content_end) {
        let (hdr, typ) = child?;
        match &typ {
            b"hdlr" => {
                let handler = data
                    .get(hdr.content_start + 8..hdr.content_start + 12)
                    .ok_or_else(|| AacError::format("isomp4: truncated hdlr"))?;
                is_sound = handler == b"soun";
            }
            b"mdhd" => {
                let body = &data[hdr.content_start..hdr.content_end];
                timescale = parse_mdhd_timescale(body)?;
                media_duration = parse_mdhd_duration(body).unwrap_or(0);
            }
            b"minf" => {
                for sub in BoxIter::new(data, hdr.content_start, hdr.content_end) {
                    let (shdr, styp) = sub?;
                    if &styp == b"stbl" {
                        fence_depth(mem, 4)?;
                        asc = Some(parse_stbl_meta(data, shdr)?);
                    }
                }
            }
            _ => {}
        }
    }
    if is_sound {
        let (asc, frames) =
            asc.ok_or_else(|| AacError::format("isomp4: sound track without stbl"))?;
        Ok(Some(SoundMeta {
            asc,
            timescale,
            duration: media_duration,
            coded_frames: frames,
        }))
    } else {
        Ok(None)
    }
}

/// ASC + `mdhd`/`elst` timing. Does not allocate `stsz`/`stco`/`stts`.
pub(crate) fn parse_aac_meta(data: &[u8], mem: &MemoryBudgets) -> Result<AacMeta> {
    fence_depth(mem, 0)?;
    let mut moov = None;
    for top in BoxIter::new(data, 0, data.len()) {
        let (hdr, typ) = top?;
        if typ == BOX_MOOV {
            moov = Some(hdr);
        } else if typ == BOX_MOOF || typ == BOX_MVEX {
            return Err(AacError::format("isomp4: fragmented MP4 is not supported"));
        }
    }
    let moov = moov.ok_or_else(|| AacError::format("isomp4: no moov box"))?;
    fence_depth(mem, 1)?;
    let mut track = None;
    let mut movie_timescale = 0u32;
    for child in BoxIter::new(data, moov.content_start, moov.content_end) {
        let (hdr, typ) = child?;
        if typ == BOX_MVEX {
            return Err(AacError::format("isomp4: fragmented MP4 is not supported"));
        } else if &typ == b"mvhd" {
            movie_timescale = parse_mvhd_timescale(&data[hdr.content_start..hdr.content_end])?;
        } else if typ == BOX_TRAK {
            if track.is_some() {
                continue;
            }
            fence_depth(mem, 2)?;
            let mut edit_start = 0u64;
            let mut edit_duration = 0u64;
            let mut has_elst = false;
            let mut mdia_track = None;
            for sub in BoxIter::new(data, hdr.content_start, hdr.content_end) {
                let (shdr, styp) = sub?;
                match &styp {
                    b"edts" => {
                        fence_depth(mem, 3)?;
                        for ed in BoxIter::new(data, shdr.content_start, shdr.content_end) {
                            let (ehdr, etyp) = ed?;
                            if &etyp == b"elst" {
                                fence_depth(mem, 4)?;
                                if let Some((start, dur)) =
                                    parse_elst(&data[ehdr.content_start..ehdr.content_end], mem)?
                                {
                                    edit_start = start;
                                    edit_duration = dur;
                                    has_elst = true;
                                }
                            }
                        }
                    }
                    b"mdia" => match parse_mdia_meta(data, shdr, mem) {
                        Ok(Some(t)) => mdia_track = Some(t),
                        Ok(None) => {}
                        Err(e @ AacError::Limit { .. }) => return Err(e),
                        Err(_) => {}
                    },
                    _ => {}
                }
            }
            if let Some(mdia) = mdia_track {
                track = Some((mdia, edit_start, edit_duration, has_elst));
            }
        }
    }
    let (mdia, edit_start, edit_duration, has_elst) =
        track.ok_or_else(|| AacError::format("isomp4: no AAC audio track found"))?;
    Ok(AacMeta {
        asc: mdia.asc,
        edit_start,
        edit_duration,
        movie_timescale,
        media_timescale: mdia.timescale,
        media_duration: mdia.duration,
        has_elst,
        coded_frames: mdia.coded_frames,
    })
}
