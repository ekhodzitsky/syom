fn append_mono_planes(out: &mut Vec<f32>, planes: &[&[f32]]) {
    if let [mono] = planes {
        out.extend_from_slice(mono);
        return;
    }
    if let [left, right] = planes
        && left.len() == right.len()
    {
        out.extend(left.iter().zip(*right).map(|(a, b)| (*a + *b) / 2.0));
        return;
    }
    let Some(first) = planes.first() else { return };
    let start = out.len();
    out.extend_from_slice(first);
    for plane in planes.iter().skip(1) {
        for (sample, value) in out[start..].iter_mut().zip(plane.iter()) {
            *sample += value;
        }
    }
    let count = planes.len() as f32;
    for sample in &mut out[start..] {
        *sample /= count;
    }
}

#[cfg(test)]
mod mono_plane_tests {
    use super::append_mono_planes;

    fn original(out: &mut Vec<f32>, planes: &[&[f32]]) {
        let Some(first) = planes.first() else { return };
        let start = out.len();
        out.extend_from_slice(first);
        for plane in planes.iter().skip(1) {
            for (sample, value) in out[start..].iter_mut().zip(plane.iter()) {
                *sample += value;
            }
        }
        for sample in &mut out[start..] {
            *sample /= planes.len() as f32;
        }
    }

    #[test]
    fn test_aac_mono_planes_preserve_add_then_divide_bits() {
        let left = [
            0.0,
            -0.0,
            1.0,
            -1.0,
            f32::MIN_POSITIVE,
            f32::from_bits(1),
            f32::MAX,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NAN,
            f32::from_bits(0x7fc0_0001),
            f32::from_bits(0xffc1_2345),
        ];
        let right = [
            -0.0,
            -0.0,
            -0.5,
            0.5,
            -f32::MIN_POSITIVE,
            f32::from_bits(1),
            -f32::MAX,
            f32::NEG_INFINITY,
            f32::INFINITY,
            1.0,
            f32::from_bits(0x7fc0_0001),
            f32::from_bits(0xffc1_2345),
        ];
        for planes in [
            vec![],
            vec![&left[..]],
            vec![&left[..], &right[..]],
            vec![&left[..], &right[..], &left[..]],
            vec![&left[..], &right[..3]],
        ] {
            let mut actual = vec![17.0, -0.0];
            let mut expected = actual.clone();
            original(&mut expected, &planes);
            append_mono_planes(&mut actual, &planes);
            assert_eq!(
                actual.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                expected.iter().map(|v| v.to_bits()).collect::<Vec<_>>()
            );
        }
    }
}
