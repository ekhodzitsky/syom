//! Rate/grid/edit boundaries for exact prefix replay (not resampler tests).
use super::*;

fn check(bytes: &[u8]) {
    for opts in [DecodeOptions::audio(), DecodeOptions::speech()] {
        let pcm = decode_with(bytes, &opts).unwrap().channels;
        let state = std::rc::Rc::new(std::cell::RefCell::new(Reads::default()));
        let reader = Sparse {
            inner: Cursor::new(bytes.to_vec()),
            state: state.clone(),
        };
        let mut src = M4aSeek::open(reader, opts).unwrap();
        let len = src.presentation_len() as usize;
        assert_eq!(len, pcm[0].len());
        for at in [0, 1, len / 2 + 7, len - 1] {
            *state.borrow_mut() = Reads::default();
            let end = (at + 41).min(len);
            let plan = src.exact_seek_plan(at as i64, end as i64).unwrap();
            let ranges: Vec<_> = plan.media_ranges().collect();
            let replay = plan.replay_access_units();
            state.borrow_mut().allowed = Some(ranges.clone());
            src.seek_exact(at as i64, replay).unwrap();
            interval(&mut src, &pcm, at, end);
            assert_eq!(state.borrow().reads, ranges);
            assert_eq!(src.last_seek_aus(), replay);
        }
    }
}

#[test]
fn rates_and_encoder_edits_preserve_absolute_native_coordinates() {
    for rate in [8_000, 22_050, 48_000, 64_000] {
        let pcm: Vec<_> = (0..rate / 4)
            .map(|i| 0.2 * (i as f32 * 0.13).sin())
            .collect();
        let bytes = encode_with(
            &[&pcm],
            rate,
            &EncodeOptions::m4a().with_bitrate_bps(32_000),
        )
        .unwrap();
        check(&bytes);
    }
}

#[test]
fn lc960_ld512_and_ld480_keep_exact_grid_and_edit_semantics() {
    check(include_bytes!("../lab/frame960/vectors/lc960-s48.m4a"));
    let pcm: Vec<_> = (0..8000).map(|i| 0.2 * (i as f32 * 0.13).sin()).collect();
    for channels in [1, 2] {
        let planes = vec![pcm.as_slice(); channels];
        let ld = encode_with(&planes, 48_000, &EncodeOptions::m4a().with_ld(true)).unwrap();
        check(&ld);
    }
    let mut asc = crate::engine::bits::BitWriter::new();
    asc.write(23, 5);
    asc.write(3, 4);
    asc.write(1, 4);
    asc.write_bit(true);
    asc.write(0, 4);
    let payloads = vec![crate::frame_grid_tests::ld480_tone_au(); 12];
    let ld =
        crate::m4a_write::mux_aac(&payloads, &asc.finish(), 1, 48_000, 5606, 137, 480).unwrap();
    check(&ld);
}

#[test]
fn prediction_profiles_cannot_authorize_partial_history() {
    for aot in [1, 3, 4] {
        let mut asc = crate::engine::bits::BitWriter::new();
        asc.write(aot, 5);
        asc.write(4, 4);
        asc.write(1, 4);
        asc.write(0, 3);
        let bytes = crate::m4a_write::mux_aac(
            &vec![vec![0xe0]; 4],
            &asc.finish(),
            1,
            44_100,
            4096,
            0,
            1024,
        )
        .unwrap();
        let (mut src, reads) = sparse(&bytes);
        assert!(matches!(
            src.exact_seek_plan(1025, 1026),
            Err(AacError::Unsupported(_))
        ));
        assert!(src.seek_exact(1025, u64::MAX).is_err());
        assert!(reads.borrow().reads.is_empty());
    }
}

#[test]
fn fragmented_lc_is_exact_and_inconsistent_sidx_source_remains_rejected() {
    check(include_bytes!("goldens/fmp4_lc.mp4"));
    assert!(
        M4aSeek::open(
            Cursor::new(include_bytes!("goldens/fmp4_lc_sidx.mp4")),
            DecodeOptions::audio()
        )
        .is_err()
    );
}

#[test]
fn unsupported_implicit_he_cannot_complete_exact_output() {
    for bytes in [
        include_bytes!("goldens/he48em.m4a").as_slice(),
        include_bytes!("goldens/he2_48em.m4a"),
    ] {
        let (mut src, _) = sparse(bytes);
        if src.exact_seek_plan(0, 1).is_ok() {
            let seek = src.seek_exact(0, u64::MAX);
            if seek.is_ok() {
                let mut emitted = false;
                assert!(
                    src.decode(|_| {
                        emitted = true;
                        Ok(())
                    })
                    .is_err()
                );
                assert!(!emitted);
            }
        }
    }
}
