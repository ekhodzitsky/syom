//! Short-window TNS: emit/parse against `tns.rs` and analysis→inverse.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::{EncTnsShort, decide_short};
use crate::engine::bits::{BitReader, BitWriter};
use crate::engine::error::Result;
use crate::engine::ics::{IcsInfo, WindowSequence, WindowShape};
use crate::engine::swb::{LONG_WINDOW_LEN, SHORT_WINDOW_LEN, short_offsets};
use crate::engine::tns::TnsData;

fn short_ics(n_swb: u8) -> IcsInfo {
    IcsInfo {
        window_sequence: WindowSequence::EightShort,
        window_shape: WindowShape::Kbd,
        max_sfb: n_swb,
        num_windows: 8,
        num_window_groups: 8,
        window_group_length: [1; 8],
        num_swb: n_swb,
        ld: false,
    }
}

fn correlated_window() -> [f32; SHORT_WINDOW_LEN] {
    let mut spec = [0.0f32; SHORT_WINDOW_LEN];
    for (i, s) in spec.iter_mut().enumerate() {
        let t = i as f32 / SHORT_WINDOW_LEN as f32;
        *s = 400.0 * (2.0 * std::f32::consts::PI * 2.0 * t).sin()
            + 120.0 * (2.0 * std::f32::consts::PI * 5.0 * t).sin();
    }
    spec
}

#[test]
fn off_is_one_flag_bit() {
    let t = EncTnsShort::off();
    assert!(!t.is_on());
    assert_eq!(t.bits(), 1);
    let mut w = BitWriter::new();
    t.emit(&mut w);
    let out = w.finish();
    assert_eq!(out[0] & 0x80, 0);
}

#[test]
fn emit_parses_as_short_tns_data() -> Result<()> {
    let offsets = short_offsets(3)?;
    let n_swb = (offsets.len() - 1) as u8;
    let mut spec = [0.0f32; LONG_WINDOW_LEN];
    spec[..SHORT_WINDOW_LEN].copy_from_slice(&correlated_window());
    let t = decide_short(&mut spec, offsets, 3);
    assert!(t.is_on(), "correlated short window must pass the PG gate");
    let mut w = BitWriter::new();
    t.emit(&mut w);
    let nbits = w.bit_len() as usize;
    let bytes = w.finish();
    let mut br = BitReader::new(&bytes);
    assert!(br.read_bit()?, "tns_data_present");
    let parsed = TnsData::parse(&mut br, &short_ics(n_swb))?;
    assert_eq!(parsed.n_windows, 8);
    assert!(parsed.windows[0].coef_res);
    assert!(parsed.windows[0].n_filt > 0);
    let f = &parsed.windows[0].filters[0];
    assert!(f.order >= 1 && f.order <= 7);
    assert!(!f.direction);
    assert_eq!(t.bits(), nbits);
    Ok(())
}

#[test]
fn analysis_inverts_through_decoder_apply() -> Result<()> {
    let offsets = short_offsets(3)?;
    let n_swb = (offsets.len() - 1) as u8;
    let mut spec = [0.0f32; LONG_WINDOW_LEN];
    spec[..SHORT_WINDOW_LEN].copy_from_slice(&correlated_window());
    let orig = spec;
    let t = decide_short(&mut spec, offsets, 3);
    assert!(t.is_on());
    let mut w = BitWriter::new();
    t.emit(&mut w);
    let bytes = w.finish();
    let mut br = BitReader::new(&bytes);
    assert!(br.read_bit()?);
    let parsed = TnsData::parse(&mut br, &short_ics(n_swb))?;
    crate::engine::tns::apply(&mut spec, &parsed, &short_ics(n_swb), 3)?;
    let mut e = 0.0f64;
    for i in 0..SHORT_WINDOW_LEN {
        let d = f64::from(spec[i] - orig[i]);
        e += d * d;
    }
    let sig: f64 = orig[..SHORT_WINDOW_LEN]
        .iter()
        .map(|&x| f64::from(x) * f64::from(x))
        .sum();
    assert!(
        e < sig * 1e-4,
        "inverse should undo analysis (err {e:.3e} vs {sig:.3e})"
    );
    Ok(())
}
