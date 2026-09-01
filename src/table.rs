//! stsz + stsc + stco → absolute (offset, size) for each sample.

use crate::error::{SyomError, media};

/// `stsc` entries are `(first_chunk, samples_per_chunk)` with 1-based chunks.
pub(crate) fn sample_locs(
    sizes: &[u32],
    stsc: &[(u32, u32)],
    chunk_offs: &[u64],
) -> Result<Vec<(u64, u32)>, SyomError> {
    if sizes.is_empty() {
        return Ok(Vec::new());
    }
    let first_off = chunk_offs.first().copied().ok_or_else(|| media("stco"))?;
    let mut out = Vec::with_capacity(sizes.len());
    let mut chunk_i = 0usize;
    let mut in_chunk = 0u32;
    let mut rem = samples_per_chunk(1, stsc)?;
    let mut at = first_off;
    for (i, size) in sizes.iter().copied().enumerate() {
        out.push((at, size));
        at = at
            .checked_add(u64::from(size))
            .ok_or_else(|| media("stco"))?;
        in_chunk = in_chunk.checked_add(1).ok_or_else(|| media("stsc"))?;
        if in_chunk == rem && i + 1 < sizes.len() {
            chunk_i = chunk_i.checked_add(1).ok_or_else(|| media("stco"))?;
            in_chunk = 0;
            at = chunk_offs
                .get(chunk_i)
                .copied()
                .ok_or_else(|| media("stco"))?;
            let chunk_no = u32::try_from(chunk_i.checked_add(1).ok_or_else(|| media("stco"))?)
                .map_err(|_| media("stco"))?;
            rem = samples_per_chunk(chunk_no, stsc)?;
        }
    }
    Ok(out)
}

fn samples_per_chunk(chunk: u32, stsc: &[(u32, u32)]) -> Result<u32, SyomError> {
    let mut found = None;
    for &(first, n) in stsc {
        if first == 0 {
            return Err(media("stsc"));
        }
        if chunk >= first {
            found = Some(n);
        }
    }
    match found {
        Some(n) if n > 0 => Ok(n),
        _ => Err(media("stsc")),
    }
}

#[cfg(test)]
#[path = "table_tests.rs"]
mod table_tests;
