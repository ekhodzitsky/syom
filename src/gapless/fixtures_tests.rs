//! Independent MP4 metadata fixtures for gapless regressions.
use crate::gapless::Delay;
use crate::{AacError, EncodeOptions, Result, encode_with};

/// Drop `edts` from a one-shot M4A and append an `iTunSMPB` string in `udta`.
pub(crate) fn m4a_tag_no_elst(pcm: &[f32], delay: &Delay) -> Result<Vec<u8>> {
    let mut file = encode_with(&[&pcm], 48_000, &EncodeOptions::m4a())?;
    let (at, len, ancestors) = find_edts(&file).ok_or_else(|| AacError::format("no edts"))?;
    for anc in &ancestors {
        let sz = u32::from_be_bytes(
            file[*anc..*anc + 4]
                .try_into()
                .map_err(|_| AacError::format("box"))?,
        ) as usize;
        file[*anc..*anc + 4].copy_from_slice(&((sz - len) as u32).to_be_bytes());
    }
    file.drain(at..at + len);
    // Independent standard-layout fixture in Apple's freeform metadata boxes.
    let text = format!(
        " 00000000 {:08X} {:08X} {:016X} 00000000",
        delay.priming, delay.remainder, delay.source
    );
    append_tag(&mut file, text.as_bytes())?;
    Ok(file)
}

pub(crate) fn append_tag(file: &mut Vec<u8>, text: &[u8]) -> Result<()> {
    let mut custom = metadata_box(b"mean", b"\0\0\0\0com.apple.iTunes");
    custom.extend(metadata_box(b"name", b"\0\0\0\0iTunSMPB"));
    let mut data = b"\0\0\0\x01\0\0\0\0".to_vec();
    data.extend_from_slice(text);
    custom.extend(metadata_box(b"data", &data));
    let ilst = metadata_box(b"ilst", &metadata_box(b"----", &custom));
    let mut meta = vec![0; 4];
    meta.extend(ilst);
    append_udta(file, &metadata_box(b"meta", &meta))
}

pub(crate) fn metadata_box(typ: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut out = ((payload.len() + 8) as u32).to_be_bytes().to_vec();
    out.extend_from_slice(typ);
    out.extend_from_slice(payload);
    out
}

pub(crate) fn append_udta(file: &mut Vec<u8>, payload: &[u8]) -> Result<()> {
    let moov = top_box(file, b"moov").ok_or_else(|| AacError::format("no moov"))?;
    let sz = u32::from_be_bytes(
        file[moov..moov + 4]
            .try_into()
            .map_err(|_| AacError::format("box"))?,
    ) as usize;
    let total = 8 + payload.len();
    let mut udta = Vec::with_capacity(total);
    udta.extend_from_slice(&(total as u32).to_be_bytes());
    udta.extend_from_slice(b"udta");
    udta.extend_from_slice(payload);
    let end = moov + sz;
    file.splice(end..end, udta);
    file[moov..moov + 4].copy_from_slice(&((sz + total) as u32).to_be_bytes());
    Ok(())
}

pub(crate) fn top_box(data: &[u8], typ: &[u8; 4]) -> Option<usize> {
    let mut c = 0usize;
    while c + 8 <= data.len() {
        let sz = u32::from_be_bytes(data[c..c + 4].try_into().ok()?) as usize;
        if sz < 8 || c + sz > data.len() {
            return None;
        }
        if &data[c + 4..c + 8] == typ {
            return Some(c);
        }
        c += sz;
    }
    None
}

pub(crate) fn find_edts(data: &[u8]) -> Option<(usize, usize, Vec<usize>)> {
    fn rec(data: &[u8], pos: usize, ancestors: &mut Vec<usize>) -> Option<(usize, usize)> {
        let sz = u32::from_be_bytes(data.get(pos..pos + 4)?.try_into().ok()?) as usize;
        if sz < 8 || pos + sz > data.len() {
            return None;
        }
        let typ = &data[pos + 4..pos + 8];
        if typ == b"edts" {
            return Some((pos, sz));
        }
        if typ == b"moov" || typ == b"trak" {
            ancestors.push(pos);
            let mut c = pos + 8;
            let end = pos + sz;
            while c + 8 <= end {
                let child = u32::from_be_bytes(data.get(c..c + 4)?.try_into().ok()?) as usize;
                if child < 8 || c + child > end {
                    return None;
                }
                if let Some(found) = rec(data, c, ancestors) {
                    return Some(found);
                }
                c += child;
            }
            ancestors.pop();
        }
        None
    }
    let mut c = 0usize;
    let mut ancestors = Vec::new();
    while c + 8 <= data.len() {
        let sz = u32::from_be_bytes(data.get(c..c + 4)?.try_into().ok()?) as usize;
        if sz < 8 || c + sz > data.len() {
            return None;
        }
        if let Some(found) = rec(data, c, &mut ancestors) {
            return Some((found.0, found.1, ancestors));
        }
        c += sz;
    }
    None
}
