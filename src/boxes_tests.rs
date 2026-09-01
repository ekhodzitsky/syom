use super::find;
use crate::error::SyomError;

fn wrap(tag: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let size = 8u32.saturating_add(payload.len() as u32);
    let mut out = Vec::with_capacity(size as usize);
    out.extend_from_slice(&size.to_be_bytes());
    out.extend_from_slice(tag);
    out.extend_from_slice(payload);
    out
}

fn wrap64(tag: &[u8; 4], payload: &[u8]) -> Result<Vec<u8>, SyomError> {
    let total = 16u64
        .checked_add(payload.len() as u64)
        .ok_or_else(|| crate::error::media("wrap64"))?;
    let mut out = Vec::new();
    out.extend_from_slice(&1u32.to_be_bytes());
    out.extend_from_slice(tag);
    out.extend_from_slice(&total.to_be_bytes());
    out.extend_from_slice(payload);
    Ok(out)
}

#[test]
fn test_find_moov_when_mdat_uses_64bit_largesize() -> Result<(), SyomError> {
    let mut file = wrap(b"ftyp", b"isom");
    file.extend_from_slice(&wrap(b"moov", b"TRACK"));
    file.extend_from_slice(&wrap64(b"mdat", &[0u8; 32])?);
    let moov = find(&file, *b"moov")?.ok_or_else(|| crate::error::media("moov"))?;
    assert_eq!(moov, b"TRACK");
    Ok(())
}

#[test]
fn test_find_moov_when_mdat_runs_to_eof() -> Result<(), SyomError> {
    let mut file = wrap(b"ftyp", b"isom");
    file.extend_from_slice(&wrap(b"moov", b"TRACK"));
    file.extend_from_slice(&0u32.to_be_bytes());
    file.extend_from_slice(b"mdat");
    file.extend_from_slice(&[7u8; 12]);
    let moov = find(&file, *b"moov")?.ok_or_else(|| crate::error::media("moov"))?;
    assert_eq!(moov, b"TRACK");
    Ok(())
}

#[test]
fn test_children_reject_truncated_largesize() {
    let mut bad = Vec::new();
    bad.extend_from_slice(&1u32.to_be_bytes());
    bad.extend_from_slice(b"mdat");
    bad.extend_from_slice(&100u64.to_be_bytes());
    bad.extend_from_slice(&[1, 2, 3]);
    assert!(super::children(&bad).is_err());
}
