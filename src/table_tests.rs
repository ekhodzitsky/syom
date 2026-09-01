use super::sample_locs;
use crate::error::SyomError;

#[test]
fn test_sample_locs_one_chunk_is_contiguous() -> Result<(), SyomError> {
    let sizes = [10u32, 20, 5];
    let locs = sample_locs(&sizes, &[(1, 3)], &[100])?;
    assert_eq!(locs, vec![(100, 10), (110, 20), (130, 5)]);
    Ok(())
}

#[test]
fn test_sample_locs_two_stsc_runs_jumps_chunk_offsets() -> Result<(), SyomError> {
    let sizes = [1u32, 1, 1, 2, 2];
    let locs = sample_locs(&sizes, &[(1, 3), (2, 1)], &[100, 200, 300])?;
    assert_eq!(locs, vec![(100, 1), (101, 1), (102, 1), (200, 2), (300, 2)]);
    Ok(())
}

#[test]
fn test_sample_locs_empty_sizes_is_empty() -> Result<(), SyomError> {
    let locs = sample_locs(&[], &[(1, 1)], &[0])?;
    assert!(locs.is_empty());
    Ok(())
}

#[test]
fn test_sample_locs_missing_chunk_is_error() -> Result<(), SyomError> {
    let err = sample_locs(&[1, 1], &[(1, 1)], &[50]);
    assert!(err.is_err());
    Ok(())
}
