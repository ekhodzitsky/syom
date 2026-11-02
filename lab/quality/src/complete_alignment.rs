//! Full-reference alignment for the speech2 qualification lane.
//! Historical exploratory rows retain their original overlap-only scoring.

pub(super) fn aligned<'a>(
    reference: &'a [f32],
    decoded: &'a [f32],
    lag: i32,
) -> Result<(&'a [f32], &'a [f32]), String> {
    let offset = usize::try_from(lag)
        .map_err(|_| format!("negative lag {lag} would discard reference samples"))?;
    let end = offset
        .checked_add(reference.len())
        .ok_or("alignment overflow")?;
    if end > decoded.len() {
        return Err(format!(
            "incomplete decode: source={} decoded={} lag={} missing={}",
            reference.len(),
            decoded.len(),
            lag,
            end - decoded.len()
        ));
    }
    Ok((reference, &decoded[offset..end]))
}

#[cfg(test)]
mod tests {
    use super::aligned;

    #[test]
    fn complete_padded_decode_preserves_reference_and_tail() {
        let reference = [1.0, 2.0, 9.0];
        let decoded = [0.0, 0.0, 1.0, 2.0, 9.0, 0.0];
        let (source, scored) = aligned(&reference, &decoded, 2).unwrap();
        assert_eq!(source, &reference);
        assert_eq!(scored, &reference);
    }

    #[test]
    fn missing_tail_is_rejected_instead_of_scoring_overlap() {
        let error = aligned(&[1.0, 2.0, 9.0], &[0.0, 0.0, 1.0, 2.0], 2).unwrap_err();
        assert!(error.contains("missing=1"), "{error}");
    }

    #[test]
    fn negative_lag_cannot_discard_reference_prefix() {
        let error = aligned(&[9.0, 1.0, 2.0], &[1.0, 2.0, 0.0], -1).unwrap_err();
        assert!(error.contains("negative lag -1"), "{error}");
    }
}
