//! EXT_FILL ABR stuffing: bit accounting matches the writer, payload
//! parses back as `fill_element()`s, room solver never overflows.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::{pad_fill_bits, pad_fill_for_room, write_pad_fill};
use crate::engine::bits::{BitReader, BitWriter};
use crate::engine::skip::fill_count;

#[test]
fn writer_bits_match_the_accounting() {
    for pad in [0usize, 1, 14, 15, 269, 270, 1000, 5000] {
        let mut w = BitWriter::new();
        write_pad_fill(&mut w, pad);
        assert_eq!(w.bit_len() as usize, pad_fill_bits(pad), "pad {pad}");
    }
}

#[test]
fn padding_parses_back_as_fill_elements() {
    let mut w = BitWriter::new();
    write_pad_fill(&mut w, 700); // 269 + 269 + 162
    let bytes = w.finish();
    let mut r = BitReader::new(&bytes);
    let mut total = 0u32;
    for expected in [269u32, 269, 162] {
        assert_eq!(r.read(3).unwrap(), 6, "ID_FIL");
        let cnt = fill_count(&mut r).unwrap();
        assert_eq!(cnt, expected);
        for _ in 0..cnt {
            assert_eq!(r.read(8).unwrap(), 0, "EXT_FILL payload byte");
        }
        total += cnt;
    }
    assert_eq!(total, 700);
    assert!(r.bits_remaining() < 8, "only byte-align pad may remain");
}

#[test]
fn room_solver_stays_within_room_and_is_maximal() {
    for room in [0usize, 7, 14, 15, 127, 2182, 8192, 65_000] {
        let pad = pad_fill_for_room(room);
        assert!(
            pad_fill_bits(pad) <= room,
            "room {room}: pad {pad} overflows"
        );
        assert!(
            pad_fill_bits(pad + 1) > room,
            "room {room}: pad {pad} not maximal"
        );
    }
    assert_eq!(pad_fill_for_room(14), 0, "smallest FIL needs 15 bits");
    assert_eq!(pad_fill_bits(0), 0);
}
