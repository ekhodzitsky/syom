//! TASK-39: SBR/PS header delay, truncation, missing payload, reset.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::bits::BitWriter;
use super::channel_map_tests::write_sce;
use super::decode::StreamDecoder;
use super::error::Error;
use super::sbr_grid::FrameClass;
use super::sbr_header::SbrHeader;
use super::sbr_huffman::{SbrHuffContext, env_tables, noise_tables};
use crate::{DecodeOptions, Decoder, decode_with};

const FS: u32 = 48_000;
const FS_SBR: u32 = 96_000;

fn he_dec() -> Box<StreamDecoder> {
    let mut dec = StreamDecoder::new();
    dec.set_he_config(true, false, FS_SBR);
    dec
}

fn write_header(w: &mut BitWriter, amp_res: bool, start: u8) {
    w.write_bit(amp_res);
    w.write(u32::from(start), 4);
    w.write(0, 4);
    w.write(1, 3);
    w.write(0, 2);
    w.write_bit(true);
    w.write_bit(false);
    w.write(0, 2);
    w.write_bit(false);
    w.write(2, 2);
}

fn write_minimal_sce(w: &mut BitWriter, n_high: usize, n_q: usize) {
    w.write_bit(false);
    w.write(FrameClass::FixFix.to_bits(), 2);
    w.write(0, 2);
    w.write_bit(true);
    w.write_bit(false);
    w.write_bit(false);
    for _ in 0..n_q {
        w.write(1, 2);
    }
    let (_, (f_huff, f_lav)) = env_tables(SbrHuffContext {
        coupling: false,
        ch: false,
        amp_res: false,
    });
    w.write(33, 7);
    for i in 1..n_high {
        let (len, code) = f_huff[(i + f_lav as usize) % f_huff.len()];
        w.write(code, u32::from(len));
    }
    let (_, (nf, nfl)) = noise_tables(SbrHuffContext {
        coupling: false,
        ch: false,
        amp_res: false,
    });
    w.write(10, 5);
    for i in 1..n_q {
        let (len, code) = nf[(i + nfl as usize) % nf.len()];
        w.write(code, u32::from(len));
    }
    w.write_bit(false);
    w.write_bit(false);
}

fn sbr_fil_bytes(start: u8) -> Vec<u8> {
    let mut hw = BitWriter::new();
    write_header(&mut hw, true, start);
    let hdr = SbrHeader::parse(&mut super::bits::BitReader::new(&hw.finish())).unwrap();
    let bands = hdr.derive_bands(FS_SBR).unwrap();
    let mut w = BitWriter::new();
    w.write(0b1101, 4);
    w.write_bit(true);
    write_header(&mut w, true, start);
    write_minimal_sce(&mut w, bands.n_high(), bands.n_q());
    w.finish()
}

fn write_fil_payload(w: &mut BitWriter, payload: &[u8]) {
    let n = payload.len() as u32;
    w.write(6, 3);
    if n < 15 {
        w.write(n, 4);
    } else {
        w.write(15, 4);
        w.write(n - 14, 8);
    }
    for b in payload {
        w.write(u32::from(*b), 8);
    }
}

fn sce_end(extra: impl FnOnce(&mut BitWriter)) -> Vec<u8> {
    let mut w = BitWriter::new();
    write_sce(&mut w, 0, 1);
    extra(&mut w);
    w.write(7, 3);
    w.finish()
}

fn decode_cfg1(
    dec: &mut StreamDecoder,
    payload: &[u8],
) -> Result<super::decode::DecodedFrame, Error> {
    dec.decode_raw_data_block(2, 3, FS, 1, 1, payload)
}

#[test]
fn truncated_sbr_fil_is_error_before_he_active() {
    let mut cold = StreamDecoder::new();
    let bytes = sce_end(|w| {
        w.write(6, 3);
        w.write(2, 4);
        w.write(0b1101, 4);
        w.write_bit(true);
        w.write(0, 12);
    });
    let err = decode_cfg1(&mut cold, &bytes).unwrap_err();
    assert!(
        matches!(
            err,
            Error::SbrHuffInvalid
                | Error::SbrFreqBandInvalid
                | Error::SbrGridInvalid
                | Error::UnexpectedEnd
                | Error::ExtensionPayloadInvalid
        ),
        "{err:?}"
    );
}

#[test]
fn headerless_first_sbr_is_error_not_lc_success() {
    let mut cold = StreamDecoder::new();
    let bytes = sce_end(|w| {
        w.write(6, 3);
        w.write(1, 4);
        w.write(0b1101, 4);
        w.write_bit(false);
        w.write(0, 4);
    });
    assert!(matches!(
        decode_cfg1(&mut cold, &bytes).unwrap_err(),
        Error::SbrFreqBandInvalid
    ));
}

#[test]
fn delayed_first_header_upsamples_without_fil() {
    let bytes = sce_end(|_| {});
    let f = decode_cfg1(&mut he_dec(), &bytes).unwrap();
    assert_eq!(f.sample_rate, FS_SBR);
    assert_eq!(f.planar[0].len(), 2048);
}

#[test]
fn missing_payload_after_sbr_still_upsamples() {
    let with = sce_end(|w| write_fil_payload(w, &sbr_fil_bytes(5)));
    let without = sce_end(|_| {});
    let mut dec = he_dec();
    let a = decode_cfg1(&mut dec, &with).unwrap();
    let b = decode_cfg1(&mut dec, &without).unwrap();
    assert_eq!(a.sample_rate, FS_SBR);
    assert_eq!(b.sample_rate, FS_SBR);
    assert_eq!(a.planar.len(), b.planar.len());
    assert_eq!(b.planar[0].len(), 2048);
}

#[test]
fn geometry_change_keeps_rate_and_length() {
    let a = sce_end(|w| write_fil_payload(w, &sbr_fil_bytes(5)));
    let b = sce_end(|w| write_fil_payload(w, &sbr_fil_bytes(6)));
    let mut dec = he_dec();
    let fa = decode_cfg1(&mut dec, &a).unwrap();
    let fb = decode_cfg1(&mut dec, &b).unwrap();
    let mut fresh = he_dec();
    let want = decode_cfg1(&mut fresh, &b).unwrap();
    assert_eq!(fb.sample_rate, fa.sample_rate);
    assert_eq!(fb.sample_rate, want.sample_rate);
    assert_eq!(fb.planar.len(), want.planar.len());
    assert_eq!(fb.planar[0].len(), want.planar[0].len());
}

#[test]
fn lc_fill_still_decodes_at_core_rate() {
    let bytes = sce_end(|w| {
        w.write(6, 3);
        w.write(1, 4);
        w.write(0, 4);
        w.write(0, 4);
    });
    let f = decode_cfg1(&mut StreamDecoder::new(), &bytes).unwrap();
    assert_eq!(f.sample_rate, FS);
    assert_eq!(f.planar[0].len(), 1024);
}

#[test]
fn he48_and_ps48_chunked_match_oneshot() {
    for (name, data) in [
        (
            "he48.adts",
            include_bytes!("../goldens/he48.adts").as_slice(),
        ),
        (
            "ps48.adts",
            include_bytes!("../goldens/ps48.adts").as_slice(),
        ),
    ] {
        let oneshot = decode_with(data, &DecodeOptions::unbounded()).unwrap();
        for n in [1usize, 9, 64, data.len()] {
            let mut dec = Decoder::new(DecodeOptions::unbounded());
            let mut samples = 0u64;
            for chunk in data.chunks(n.max(1)) {
                dec.feed(chunk, |f| {
                    samples += f.samples as u64;
                    Ok(())
                })
                .unwrap();
            }
            let info = dec
                .finish(|f| {
                    samples += f.samples as u64;
                    Ok(())
                })
                .unwrap();
            assert_eq!(info.sample_rate, oneshot.sample_rate, "{name} n={n}");
            assert_eq!(info.channels, oneshot.channels.len(), "{name} n={n}");
            assert_eq!(
                info.samples as usize,
                oneshot.channels[0].len(),
                "{name} n={n}"
            );
            assert_eq!(samples, info.samples, "{name} n={n}");
        }
    }
}
