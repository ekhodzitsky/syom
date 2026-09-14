//! Top-level ISOBMFF walk over `Read + Seek`: load `moov`, skip `mdat`.

use std::io::{self, Read, Seek, SeekFrom};

use crate::budgets::{BudgetKind, MemoryBudgets, check_planned};
use crate::error::{AacError, Result};

use super::{BOX_MOOF, BOX_MOOV, BOX_MVEX};

fn read_exact<R: Read>(r: &mut R, buf: &mut [u8]) -> Result<()> {
    loop {
        match r.read_exact(buf) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                return Err(AacError::truncated_at(None));
            }
            Err(e) => return Err(AacError::from(e)),
        }
    }
}

fn seek<R: Seek>(r: &mut R, pos: SeekFrom) -> Result<u64> {
    r.seek(pos).map_err(AacError::from)
}

/// Read the `moov` box (header included) without loading `mdat`. `file_len`
/// is the ISOBMFF size from `SeekFrom::End`.
pub(crate) fn load_moov<R: Read + Seek>(r: &mut R, mem: &MemoryBudgets) -> Result<(Vec<u8>, u64)> {
    let file_len = seek(r, SeekFrom::End(0))?;
    seek(r, SeekFrom::Start(0))?;
    let mut pos = 0u64;
    let mut moov = None;
    while pos.saturating_add(8) <= file_len {
        seek(r, SeekFrom::Start(pos))?;
        let mut hdr = [0u8; 8];
        read_exact(r, &mut hdr)?;
        let size32 = u32::from_be_bytes([hdr[0], hdr[1], hdr[2], hdr[3]]) as u64;
        let typ = [hdr[4], hdr[5], hdr[6], hdr[7]];
        let (box_size, header_len) = match size32 {
            1 => {
                let mut ext = [0u8; 8];
                read_exact(r, &mut ext)?;
                (u64::from_be_bytes(ext), 16u64)
            }
            0 => (file_len.saturating_sub(pos), 8u64),
            s => (s, 8u64),
        };
        if box_size < header_len {
            return Err(AacError::format(format!(
                "isomp4: box size {box_size} smaller than its header"
            )));
        }
        let end = pos
            .checked_add(box_size)
            .filter(|&e| e <= file_len)
            .ok_or_else(|| AacError::format("isomp4: box extends past end of input"))?;
        if typ == BOX_MOOF || typ == BOX_MVEX {
            return Err(AacError::format("isomp4: fragmented MP4 is not supported"));
        }
        if typ == BOX_MOOV && moov.is_none() {
            let n = check_planned(BudgetKind::Metadata, box_size, mem.max_metadata_bytes)
                .map_err(AacError::from)?;
            seek(r, SeekFrom::Start(pos))?;
            let mut buf = vec![0u8; n];
            read_exact(r, &mut buf)?;
            moov = Some(buf);
        }
        pos = end;
    }
    let moov = moov.ok_or_else(|| AacError::format("isomp4: no moov box"))?;
    Ok((moov, file_len))
}
