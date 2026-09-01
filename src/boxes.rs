//! ISO-BMFF box walk. Sound extract only. No libav.

use crate::error::{SyomError, media};

pub(crate) fn be_u16(data: &[u8], at: usize) -> Result<u16, SyomError> {
    let end = at.checked_add(2).ok_or_else(|| media("u16"))?;
    let slice = data.get(at..end).ok_or_else(|| media("u16"))?;
    let bytes: [u8; 2] = slice.try_into().map_err(|_| media("u16"))?;
    Ok(u16::from_be_bytes(bytes))
}

pub(crate) fn be_u32(data: &[u8], at: usize) -> Result<u32, SyomError> {
    let end = at.checked_add(4).ok_or_else(|| media("u32"))?;
    let slice = data.get(at..end).ok_or_else(|| media("u32"))?;
    let bytes: [u8; 4] = slice.try_into().map_err(|_| media("u32"))?;
    Ok(u32::from_be_bytes(bytes))
}

pub(crate) fn fourcc(data: &[u8], at: usize) -> Result<[u8; 4], SyomError> {
    let end = at.checked_add(4).ok_or_else(|| media("fourcc"))?;
    let slice = data.get(at..end).ok_or_else(|| media("fourcc"))?;
    slice.try_into().map_err(|_| media("fourcc"))
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct RawBox<'a> {
    pub tag: [u8; 4],
    pub payload: &'a [u8],
}

pub(crate) fn children(data: &[u8]) -> Result<Vec<RawBox<'_>>, SyomError> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i.saturating_add(8) <= data.len() {
        let tag = fourcc(data, i.saturating_add(4))?;
        let (hdr, size) = box_span(data, i)?;
        let start = i.checked_add(hdr).ok_or_else(|| media("box"))?;
        let end = i.checked_add(size).ok_or_else(|| media("box"))?;
        let payload = data.get(start..end).ok_or_else(|| media("box"))?;
        out.push(RawBox { tag, payload });
        i = end;
    }
    Ok(out)
}

/// ISO-BMFF: size 1 means a 64-bit largesize follows the type; size 0
/// means the box runs to the end of `data`.
fn box_span(data: &[u8], at: usize) -> Result<(usize, usize), SyomError> {
    let size32 = u64::from(be_u32(data, at)?);
    if size32 == 1 {
        let hdr_end = at.checked_add(16).ok_or_else(|| media("largesize"))?;
        let slice_at = at.checked_add(8).ok_or_else(|| media("largesize"))?;
        let slice = data
            .get(slice_at..hdr_end)
            .ok_or_else(|| media("largesize"))?;
        let bytes: [u8; 8] = slice.try_into().map_err(|_| media("largesize"))?;
        let size = usize::try_from(u64::from_be_bytes(bytes)).map_err(|_| media("largesize"))?;
        let end = at.checked_add(size).ok_or_else(|| media("largesize"))?;
        if size < 16 || end > data.len() {
            return Err(media("largesize"));
        }
        return Ok((16, size));
    }
    if size32 == 0 {
        let size = data.len().checked_sub(at).ok_or_else(|| media("box"))?;
        if size < 8 {
            return Err(media("box"));
        }
        return Ok((8, size));
    }
    let size = usize::try_from(size32).map_err(|_| media("box"))?;
    let end = at.checked_add(size).ok_or_else(|| media("box"))?;
    if size < 8 || end > data.len() {
        return Err(media("box"));
    }
    Ok((8, size))
}

pub(crate) fn find(data: &[u8], tag: [u8; 4]) -> Result<Option<&[u8]>, SyomError> {
    for child in children(data)? {
        if child.tag == tag {
            return Ok(Some(child.payload));
        }
        if is_container(child.tag)
            && let Some(hit) = find(child.payload, tag)?
        {
            return Ok(Some(hit));
        }
    }
    Ok(None)
}

pub(crate) fn find_all(data: &[u8], tag: [u8; 4]) -> Result<Vec<&[u8]>, SyomError> {
    let mut hits = Vec::new();
    collect(data, tag, &mut hits)?;
    Ok(hits)
}

fn collect<'a>(data: &'a [u8], tag: [u8; 4], hits: &mut Vec<&'a [u8]>) -> Result<(), SyomError> {
    for child in children(data)? {
        if child.tag == tag {
            hits.push(child.payload);
        }
        if is_container(child.tag) {
            collect(child.payload, tag, hits)?;
        }
    }
    Ok(())
}

fn is_container(tag: [u8; 4]) -> bool {
    matches!(
        &tag,
        b"moov" | b"trak" | b"mdia" | b"minf" | b"stbl" | b"dinf" | b"edts"
    )
}

#[cfg(test)]
#[path = "boxes_tests.rs"]
mod boxes_tests;
