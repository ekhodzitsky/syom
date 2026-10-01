//! TASK-92: `ps_data()` writer — hand-derived bit vectors, decoder parse
//! and index resolution, DF/DT choice, header refresh, hold frames,
//! extension sizing at the 15-byte escape boundary, and failure cases.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::{PS_EXT_MAX_BYTES, PsBits, PsWriter, write_extended_data};
use crate::engine::bits::{BitReader, BitWriter};
use crate::engine::enc_ps_est::{PS_BANDS, PsFrameParams};
use crate::engine::error::Error;
use crate::engine::ps_data::{PsConfig, PsData, PsIndexState};

fn bit_string(b: &PsBits) -> String {
    (0..b.bits)
        .map(|i| {
            if b.bytes[i / 8] >> (7 - i % 8) & 1 == 1 {
                '1'
            } else {
                '0'
            }
        })
        .collect()
}

fn params(iid: [i8; PS_BANDS], icc: [u8; PS_BANDS]) -> PsFrameParams {
    PsFrameParams { iid, icc }
}

/// Parse with the decoder and resolve to absolute indices.
fn decode(
    b: &PsBits,
    cfg: &mut Option<PsConfig>,
    state: &mut PsIndexState,
) -> (PsData, Vec<Vec<i32>>, Vec<Vec<i32>>) {
    let mut r = BitReader::new(&b.bytes);
    let d = PsData::parse(&mut r, cfg.as_ref())
        .unwrap()
        .expect("decodable");
    assert_eq!(
        r.bit_position() as usize,
        b.bits,
        "parser consumed exactly the written bits"
    );
    *cfg = Some(d.config);
    let idx = d.resolve(state).unwrap();
    (d.clone(), idx.iid, idx.icc)
}

#[test]
fn fine_iid_header_is_mode_4_and_round_trips() {
    // enable_ps_header 1 | enable_iid 1, iid_mode 100 | enable_icc 1,
    // icc_mode 001 | enable_ext 0 | frame_class 0 | num_env 01 |
    // iid_dt 0 + 20 × fine zero ("1") | icc_dt 0 + 20 × "0".
    let want = format!("11100100100010{}0{}", "1".repeat(20), "0".repeat(20));
    let b = PsWriter::new()
        .with_fine_iid()
        .frame(Some(&PsFrameParams::default()), true)
        .unwrap();
    assert_eq!(bit_string(&b), want);
    assert_eq!(b.bits, 55);
    let mut cfg = None;
    let mut state = PsIndexState::default();
    let (d, iid, icc) = decode(&b, &mut cfg, &mut state);
    assert_eq!(d.config.iid_mode, 4);
    assert!(d.config.iid_quant_fine());
    assert_eq!(iid[0], vec![0; PS_BANDS]);
    assert_eq!(icc[0], vec![0; PS_BANDS]);
    // Ends of the fine grid, including a −30 frequency delta.
    let p = PsFrameParams {
        iid: [
            15, -15, 15, -15, 7, -7, 1, -1, 0, 8, -8, 12, -12, 4, -4, 15, -15, 2, -2, 0,
        ],
        icc: [0, 7, 1, 6, 2, 5, 3, 4, 0, 1, 2, 3, 4, 5, 6, 7, 0, 7, 3, 1],
    };
    let mut w = PsWriter::new().with_fine_iid();
    let (mut cfg, mut state) = (None, PsIndexState::default());
    for header in [true, false] {
        let bits = w.frame(Some(&p), header).unwrap();
        let (d, iid, icc) = decode(&bits, &mut cfg, &mut state);
        assert_eq!(d.config.iid_mode, 4);
        assert_eq!(iid[0], p.iid.map(i32::from).to_vec());
        assert_eq!(icc[0], p.icc.map(i32::from).to_vec());
        if header {
            assert!(!d.iid_dt[0], "header frames stay frequency-differential");
        }
    }
    let mut over = p;
    over.iid[0] = 16;
    assert!(
        PsWriter::new()
            .with_fine_iid()
            .frame(Some(&over), true)
            .is_err()
    );
    over.iid[0] = -16;
    assert!(
        PsWriter::new()
            .with_fine_iid()
            .frame(Some(&over), true)
            .is_err()
    );
}

#[test]
fn header_frame_with_neutral_parameters_is_the_hand_derived_vector() {
    // Table 8.1 by hand: enable_ps_header 1 | enable_iid 1, iid_mode 001 |
    // enable_icc 1, icc_mode 001 | enable_ext 0 | frame_class 0 |
    // num_env_idx 01 | iid_dt 0 + 20 × "0" | icc_dt 0 + 20 × "0"
    // (a zero delta is the 1-bit code "0" in every PS table).
    let want = format!(
        "1{}{}0{}{}{}",
        "1001",
        "1001",
        "001",
        "0".repeat(21),
        "0".repeat(21)
    );
    let b = PsWriter::new()
        .frame(Some(&PsFrameParams::default()), true)
        .unwrap();
    assert_eq!(bit_string(&b), want);
    assert_eq!(b.bits, 55);
}

#[test]
fn headerless_hold_and_repeat_frames_are_the_hand_derived_vectors() {
    let mut w = PsWriter::new();
    let p = params([3; PS_BANDS], [2; PS_BANDS]);
    w.frame(Some(&p), true).unwrap();
    // Same parameters again: no header, frame_class 0, one envelope, both
    // rows time-differential (dt = 1) with twenty zero deltas.
    let again = w.frame(Some(&p), false).unwrap();
    let row = format!("1{}", "0".repeat(20));
    assert_eq!(bit_string(&again), format!("0001{row}{row}"));
    // Hold: no header, frame_class 0, num_env_idx 00.
    let hold = w.frame(None, false).unwrap();
    assert_eq!(bit_string(&hold), "0000");
}

#[test]
fn decoder_recovers_every_index_across_df_dt_header_refresh_and_hold() {
    let mut s = 0x1234_5678u32;
    let mut next = move |m: u32| {
        s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (s >> 8) % m
    };
    let mut w = PsWriter::new();
    let (mut cfg, mut state) = (None, PsIndexState::default());
    let mut last = PsFrameParams::default();
    let (mut used_dt, mut used_df) = (false, false);
    for frame in 0..200 {
        let header = frame % 25 == 0;
        if frame % 17 == 5 {
            let b = w.frame(None, false).unwrap();
            let (d, iid, _) = decode(&b, &mut cfg, &mut state);
            assert_eq!((d.num_env, iid.len()), (0, 0), "hold frame");
            continue;
        }
        // Slow drift most frames (DT wins), a jump now and then (DF wins).
        let mut p = last;
        for b in 0..PS_BANDS {
            if frame % 9 == 0 {
                p.iid[b] = next(15) as i8 - 7;
                p.icc[b] = next(8) as u8;
            } else if next(4) == 0 {
                p.iid[b] = (p.iid[b] + next(3) as i8 - 1).clamp(-7, 7);
                p.icc[b] = (p.icc[b] as i8 + next(3) as i8 - 1).clamp(0, 7) as u8;
            }
        }
        let bits = w.frame(Some(&p), header).unwrap();
        let (d, iid, icc) = decode(&bits, &mut cfg, &mut state);
        assert_eq!(d.header_present, header);
        assert_eq!(
            (d.config.iid_mode, d.config.icc_mode, d.config.enable_ext),
            (1, 1, false)
        );
        assert_eq!(iid[0], p.iid.map(i32::from).to_vec(), "frame {frame} IID");
        assert_eq!(icc[0], p.icc.map(i32::from).to_vec(), "frame {frame} ICC");
        if header {
            assert!(!d.iid_dt[0] && !d.icc_dt[0], "header frames are joinable");
        }
        used_dt |= d.iid_dt[0];
        used_df |= !d.iid_dt[0] && !header;
        last = p;
    }
    assert!(
        used_dt && used_df,
        "both differential directions were exercised"
    );
}

#[test]
fn extended_data_block_sizes_exactly_across_the_escape_boundary() {
    // Payload bit counts around cnt = 14 / 15 / 16 bytes (2 id bits + payload).
    for bits in [1usize, 6, 7, 110, 111, 112, 118, 119, 120, 126, 127, 500] {
        let ps = PsBits {
            bytes: vec![0xA5; bits.div_ceil(8)],
            bits,
        };
        let mut w = BitWriter::new();
        w.write(0b101, 3); // the block starts unaligned inside sbr_data
        write_extended_data(&mut w, &ps).unwrap();
        let total = w.bit_len() as usize - 3;
        let cnt = (2 + bits).div_ceil(8);
        let size_field = if cnt < 15 { 4 } else { 12 };
        assert_eq!(total, 1 + size_field + cnt * 8, "bits {bits}");
        // Independent read-back of the header fields.
        let bytes = w.finish();
        let mut r = BitReader::new(&bytes);
        assert_eq!(r.read(3).unwrap(), 0b101);
        assert_eq!(r.read(1).unwrap(), 1, "bs_extended_data");
        let mut got = r.read(4).unwrap() as usize;
        if got == 15 {
            got += r.read(8).unwrap() as usize;
        }
        assert_eq!(got, cnt, "bs_extension_size for {bits} payload bits");
        assert_eq!(r.read(2).unwrap(), 2, "EXTENSION_ID_PS");
        for i in 0..bits {
            let want = u32::from(0xA5u8 >> (7 - i % 8) & 1);
            assert_eq!(r.read(1).unwrap(), want, "payload bit {i}");
        }
        for _ in 0..cnt * 8 - 2 - bits {
            assert_eq!(r.read(1).unwrap(), 0, "fill bits are zero");
        }
    }
}

#[test]
fn unrepresentable_state_and_oversize_fail_without_output() {
    let mut w = PsWriter::new();
    // Parameters before any header cannot be decoded.
    assert!(matches!(
        w.frame(Some(&PsFrameParams::default()), false),
        Err(Error::PsDataInvalid)
    ));
    let mut bad = PsFrameParams::default();
    bad.iid[4] = 8;
    assert!(w.frame(Some(&bad), true).is_err());
    bad.iid[4] = 0;
    bad.icc[19] = 8;
    assert!(w.frame(Some(&bad), true).is_err());
    // A failed frame leaves the differential state untouched.
    let ok = w.frame(Some(&PsFrameParams::default()), true).unwrap();
    assert_eq!(ok.bits, 55);
    let mut out = BitWriter::new();
    let huge = PsBits {
        bytes: vec![0; PS_EXT_MAX_BYTES + 1],
        bits: PS_EXT_MAX_BYTES * 8,
    };
    assert!(write_extended_data(&mut out, &huge).is_err());
    assert!(write_extended_data(&mut out, &PsBits::default()).is_err());
    assert!(
        write_extended_data(
            &mut out,
            &PsBits {
                bytes: vec![0],
                bits: 9
            }
        )
        .is_err()
    );
    assert_eq!(out.bit_len(), 0, "nothing written on failure");
    // Deterministic bytes.
    let a = PsWriter::new()
        .frame(Some(&params([-7; PS_BANDS], [7; PS_BANDS])), true)
        .unwrap();
    let b = PsWriter::new()
        .frame(Some(&params([-7; PS_BANDS], [7; PS_BANDS])), true)
        .unwrap();
    assert_eq!(a, b);
}

/// The PS payload inside a real `sbr_extension_data()`: the SBR parser
/// must find extension id 2, its body must parse as the written
/// `ps_data()`, and `num_sbr_bits` must grow by exactly the block size.
#[test]
fn ps_rides_in_the_sbr_extended_data_block_of_a_mono_element() {
    use crate::engine::enc_sbr_bits::write_sbr_extension;
    use crate::engine::enc_sbr_est::SbrEstimator;
    use crate::engine::raw_data_block::IdSynEle;
    use crate::engine::sbr_element::EXTENSION_ID_PS;
    use crate::engine::sbr_extension::SbrExtensionData;
    let est = SbrEstimator::new(48_000, 24).unwrap();
    let (h, bands) = (est.header(), est.bands());
    let silent = crate::engine::enc_sbr_bits::enc_sbr_bits_tests::minimal(12, 2, 33, 10);
    let p = params(
        [
            -2, -1, 0, 1, 2, 3, 4, 5, 6, 7, -7, -6, -5, -4, -3, 0, 0, 1, 1, 2,
        ],
        [0, 1, 2, 3, 4, 5, 6, 7, 0, 1, 2, 3, 4, 5, 6, 7, 0, 0, 7, 7],
    );
    let ps = PsWriter::new().frame(Some(&p), true).unwrap();
    let mut w = BitWriter::new();
    let plain = write_sbr_extension(&mut w, h, true, bands, &[&silent], None).unwrap();
    let mut w = BitWriter::new();
    let n = write_sbr_extension(&mut w, h, true, bands, &[&silent], Some(&ps)).unwrap();
    let cnt = (2 + ps.bits).div_ceil(8);
    let size_field = if cnt < 15 { 4 } else { 12 };
    assert_eq!(n, plain + size_field as u64 + 8 * cnt as u64);
    let bytes = w.finish();
    let ext = SbrExtensionData::parse(
        &mut BitReader::new(&bytes),
        IdSynEle::Sce,
        false,
        48_000,
        None,
        None,
    )
    .unwrap();
    assert_eq!(ext.num_sbr_bits, n);
    let got = ext
        .element
        .extension
        .as_ref()
        .expect("extended data present");
    assert_eq!(got.id, EXTENSION_ID_PS);
    let d = PsData::parse(&mut BitReader::new(&got.data), None)
        .unwrap()
        .unwrap();
    let idx = d.resolve(&mut PsIndexState::default()).unwrap();
    assert_eq!(idx.iid[0], p.iid.map(i32::from).to_vec());
    assert_eq!(idx.icc[0], p.icc.map(i32::from).to_vec());
    // A channel pair cannot carry PS.
    assert!(
        write_sbr_extension(
            &mut BitWriter::new(),
            h,
            true,
            bands,
            &[&silent, &silent],
            Some(&ps)
        )
        .is_err()
    );
}
