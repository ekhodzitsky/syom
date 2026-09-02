//! Spectral Huffman walk, pulse, inverse quant, scalefactor gain — §4.6.1–4.6.3.

use super::bits::BitReader;
use super::error::{Error, Result};
use super::huff;
use super::ics::IcsInfo;
use super::section::{SectionData, has_spectral};
use super::sf::ScaleFactors;
use super::swb::{LONG_WINDOW_LEN, long_offsets, short_offsets};
use std::sync::LazyLock;

/// `SF_OFFSET` in §4.6.2.3.3 — scalefactor 100 is unit gain.
pub const SF_OFFSET: i32 = 100;

static POW43: LazyLock<[f64; 8192]> = LazyLock::new(|| {
    let mut t = [0.0f64; 8192];
    for (i, slot) in t.iter_mut().enumerate() {
        let a = i as f64;
        *slot = a * a.cbrt();
    }
    t
});

/// Inverse-quantise one coefficient (§4.6.1.3).
#[must_use]
pub fn invquant(q: i32) -> f64 {
    let a = q.unsigned_abs() as usize;
    let mag = if a < POW43.len() {
        POW43[a]
    } else {
        let x = q.abs() as f64;
        x * x.cbrt()
    };
    if q < 0 { -mag } else { mag }
}

/// `2^(0.25 * (sf - 100))`.
#[must_use]
pub fn sf_gain(sf: i32) -> f64 {
    (0.25 * f64::from(sf - SF_OFFSET)).exp2()
}

/// Pulse record after Table 4.7.
#[derive(Debug, Clone)]
pub struct PulseData {
    /// `pulse_start_sfb`.
    pub start_sfb: u8,
    /// `(offset, amp)` pairs, 1..=4.
    pub pulses: Vec<(u8, u8)>,
}

impl PulseData {
    /// Table 4.7 `pulse_data()`.
    pub fn parse(br: &mut BitReader<'_>) -> Result<Self> {
        let n = br.read(2)? as usize + 1;
        let start_sfb = br.read(6)? as u8;
        let mut pulses = Vec::with_capacity(n);
        for _ in 0..n {
            let offset = br.read(5)? as u8;
            let amp = br.read(4)? as u8;
            pulses.push((offset, amp));
        }
        Ok(PulseData { start_sfb, pulses })
    }
}

/// Huffman `spectral_data()` into a 1024-bin window-major quantised spectrum.
pub fn parse_quant(
    br: &mut BitReader<'_>,
    ics: &IcsInfo,
    sections: &SectionData,
    fs_index: u8,
) -> Result<Vec<i32>> {
    let win_len = ics.window_len();
    let nwin = ics.num_windows as usize;
    let mut quant = vec![0i32; nwin * win_len];
    let offsets = if ics.window_sequence.is_eight_short() {
        short_offsets(fs_index)?
    } else {
        long_offsets(fs_index)?
    };
    let mut wbase = 0usize;
    let mut tuple = [0i32; 4];
    for g in 0..ics.num_window_groups as usize {
        let glen = ics.window_group_length[g] as usize;
        let cbs = sections.sfb_cb.get(g).ok_or(Error::SpectrumInvalid)?;
        for sfb in 0..ics.max_sfb as usize {
            let cb = *cbs.get(sfb).ok_or(Error::SpectrumInvalid)?;
            if !has_spectral(cb) {
                continue;
            }
            let start = *offsets.get(sfb).ok_or(Error::SpectrumInvalid)? as usize;
            let end = *offsets.get(sfb + 1).ok_or(Error::SpectrumInvalid)? as usize;
            if end < start {
                return Err(Error::SpectrumInvalid);
            }
            let width = end - start;
            let bins = width * glen;
            let mut filled = 0usize;
            while filled < bins {
                let n = huff::decode_tuple(br, cb, &mut tuple)?;
                for &sample in tuple.iter().take(n) {
                    if filled >= bins {
                        break;
                    }
                    let b = filled / width;
                    let i = filled % width;
                    let w = wbase + b;
                    let idx = w * win_len + start + i;
                    *quant.get_mut(idx).ok_or(Error::SpectrumInvalid)? = sample;
                    filled += 1;
                }
            }
        }
        wbase += glen;
    }
    Ok(quant)
}

/// §4.6.13 pulse reconstruction on a long-window quantised spectrum.
pub fn apply_pulse(quant: &mut [i32], fs_index: u8, pulse: &PulseData) -> Result<()> {
    let offsets = long_offsets(fs_index)?;
    let start = pulse.start_sfb as usize;
    if start >= offsets.len().saturating_sub(1) {
        return Err(Error::SpectrumInvalid);
    }
    let mut k = offsets[start] as usize;
    for &(off, amp) in &pulse.pulses {
        k += off as usize;
        if k >= quant.len().min(LONG_WINDOW_LEN) {
            return Err(Error::SpectrumInvalid);
        }
        let a = amp as i32;
        if quant[k] > 0 {
            quant[k] += a;
        } else {
            quant[k] -= a;
        }
    }
    Ok(())
}

/// Inverse quant + scalefactor gain into a window-major f64 spectrum.
pub fn rescale(
    quant: &[i32],
    ics: &IcsInfo,
    sections: &SectionData,
    sf: &ScaleFactors,
    fs_index: u8,
) -> Result<Vec<f64>> {
    let win_len = ics.window_len();
    let nwin = ics.num_windows as usize;
    let mut spec = vec![0.0f64; nwin * win_len];
    let offsets = if ics.window_sequence.is_eight_short() {
        short_offsets(fs_index)?
    } else {
        long_offsets(fs_index)?
    };
    let mut wbase = 0usize;
    for g in 0..ics.num_window_groups as usize {
        let glen = ics.window_group_length[g] as usize;
        let cbs = sections.sfb_cb.get(g).ok_or(Error::SpectrumInvalid)?;
        for sfb in 0..ics.max_sfb as usize {
            let cb = *cbs.get(sfb).unwrap_or(&0);
            if !has_spectral(cb) {
                continue;
            }
            let gain = sf_gain(*sf.sf.get(g).and_then(|v| v.get(sfb)).unwrap_or(&0));
            let start = *offsets.get(sfb).ok_or(Error::SpectrumInvalid)? as usize;
            let end = *offsets.get(sfb + 1).ok_or(Error::SpectrumInvalid)? as usize;
            for b in 0..glen {
                let w = wbase + b;
                for i in start..end {
                    let idx = w * win_len + i;
                    spec[idx] = invquant(*quant.get(idx).unwrap_or(&0)) * gain;
                }
            }
        }
        wbase += glen;
    }
    Ok(spec)
}
