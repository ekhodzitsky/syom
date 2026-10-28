//! Seeking must preserve the PNS generator history of a linear decode.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use crate::{DecodeOptions, M4aSeek, Result, decode_with};
use std::io::Cursor;

fn pns_m4a() -> Result<Vec<u8>> {
    let bytes = include_bytes!("goldens/pns48.adts");
    let mut at = 0;
    let mut payloads = Vec::new();
    while at < bytes.len() {
        let (header, off) = crate::engine::adts::AdtsHeader::parse(&bytes[at..])?;
        let len = usize::from(header.aac_frame_length);
        payloads.push(bytes[at + off..at + len].to_vec());
        at += len;
    }
    crate::m4a_write::mux_aac_lc(&payloads, 3, 1, 48_000, payloads.len() as u64 * 1024, 0)
}

#[test]
fn pns_seek_matches_linear_on_cold_and_reused_source() -> Result<()> {
    let bytes = pns_m4a()?;
    for opts in [DecodeOptions::speech(), DecodeOptions::audio()] {
        let linear = decode_with(&bytes, &opts)?;
        let n = linear.channels[0].len();
        let mut src = M4aSeek::open(Cursor::new(&bytes), opts)?;
        for at in [n / 2 + 17, 0, n / 2 + 17, n - 1024, 1025] {
            src.seek(at as i64)?;
            let mut position = at;
            src.decode(|f| {
                for (got, want) in f.planar.iter().zip(&linear.channels) {
                    assert!(
                        *got == &want[position..position + f.samples],
                        "PNS history differs after seek to {at} at sample {position}"
                    );
                }
                position += f.samples;
                Ok(())
            })?;
            assert_eq!(position, n);
        }
    }
    Ok(())
}
