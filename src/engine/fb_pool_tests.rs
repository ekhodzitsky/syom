//! Filterbank overlap keyed by (kind, tag), not encounter order.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::bits::BitWriter;
use super::channel_map_tests::{decode_planes, pce_frame, wrap_adts, write_cpe, write_sce};
use super::decode::StreamDecoder;
use super::error::Error;
use crate::{DecodeOptions, Decoder};

fn rdb(body: impl FnOnce(&mut BitWriter)) -> Vec<u8> {
    let mut w = BitWriter::new();
    body(&mut w);
    w.write(7, 3);
    w.finish()
}

type Planes = Vec<Vec<f32>>;

fn decode2(a: &[u8], b: &[u8], cfg: u8) -> Result<(Planes, Planes), Error> {
    let mut dec = StreamDecoder::new();
    let fa = dec.decode_raw_data_block(2, 3, 48_000, cfg, 1, a)?;
    let fb = dec.decode_raw_data_block(2, 3, 48_000, cfg, 1, b)?;
    Ok((fa.planar, fb.planar))
}

fn combined_pair() -> (Vec<u8>, Vec<u8>) {
    let a = pce_frame(&[(false, 5), (true, 2)], &[], &[], |w| {
        write_sce(w, 5, 1);
        write_cpe(w, 2, 3, 5);
    });
    let b = pce_frame(&[(false, 5), (true, 2)], &[], &[], |w| {
        write_cpe(w, 2, 6, 8);
        write_sce(w, 5, 3);
    });
    (a, b)
}

#[test]
fn reorder_preserves_per_element_overlap() -> Result<(), Error> {
    let (a, b) = combined_pair();
    let (_, got) = decode2(&a, &b, 0)?;
    let (_, sce) = decode2(
        &rdb(|w| write_sce(w, 5, 1)),
        &rdb(|w| write_sce(w, 5, 3)),
        0,
    )?;
    let (_, cpe) = decode2(
        &rdb(|w| write_cpe(w, 2, 3, 5)),
        &rdb(|w| write_cpe(w, 2, 6, 8)),
        0,
    )?;
    assert_eq!(got.len(), 3);
    assert_eq!(got[0], sce[0], "SCE overlap followed encounter order");
    assert_eq!(got[1], cpe[0], "CPE L overlap followed encounter order");
    assert_eq!(got[2], cpe[1], "CPE R overlap followed encounter order");
    Ok(())
}

#[test]
fn layout_change_starts_new_identity_cold() -> Result<(), Error> {
    let sce = pce_frame(&[(false, 0)], &[], &[], |w| write_sce(w, 0, 1));
    let cpe = pce_frame(&[(true, 0)], &[], &[], |w| write_cpe(w, 0, 3, 5));
    let mut dec = StreamDecoder::new();
    dec.decode_raw_data_block(2, 3, 48_000, 0, 1, &sce)?;
    let got = dec.decode_raw_data_block(2, 3, 48_000, 0, 1, &cpe)?;
    let fresh = decode_planes(&cpe, 0)?;
    assert_eq!(got.planar, fresh, "SCE overlap leaked into CPE tag 0");
    Ok(())
}

#[test]
fn duplicate_identity_in_one_frame_is_format() {
    let bytes = rdb(|w| {
        write_sce(w, 0, 1);
        write_sce(w, 0, 3);
    });
    let err = decode_planes(&bytes, 0).expect_err("dup SCE");
    assert!(matches!(
        err,
        Error::Format("duplicate channel element identity")
    ));
}

#[test]
fn chunked_public_decode_agrees_on_reordered_frames() {
    let (a, b) = combined_pair();
    let bytes = [wrap_adts(&a), wrap_adts(&b)].concat();
    let opts = DecodeOptions::unbounded();
    let one = crate::decode_with(&bytes, &opts).unwrap();
    let mut dec = Decoder::new(opts);
    let mut planes: Vec<Vec<f32>> = Vec::new();
    let mut acc = |f: crate::Frame<'_>| {
        if planes.is_empty() {
            planes = f.planar.iter().map(|p| p.to_vec()).collect();
        } else {
            for (dst, src) in planes.iter_mut().zip(f.planar.iter()) {
                dst.extend_from_slice(src);
            }
        }
        Ok(())
    };
    for chunk in bytes.chunks(3) {
        dec.feed(chunk, &mut acc).unwrap();
    }
    dec.finish(&mut acc).unwrap();
    assert_eq!(one.channels, planes);
}

#[test]
fn steady_second_frame_is_not_a_cold_restart() -> Result<(), Error> {
    let (a, _) = combined_pair();
    let (first, second) = decode2(&a, &a, 0)?;
    assert_eq!(first.len(), second.len());
    assert_ne!(
        first, second,
        "identical payloads must still overlap-add; a reset would match"
    );
    Ok(())
}
