//! `scale_factor_data()` — ISO/IEC 14496-3 Table 4.53 + Table 4.A.1.

use super::bits::BitReader;
use super::error::{Error, Result};
use super::ics::IcsInfo;
use super::section::{ZERO_HCB, is_intensity, is_noise};
use super::sf_tab::{SF_CODE, SF_LEN};
use std::sync::LazyLock;

const NOISE_OFFSET: i32 = 90;
const NOISE_PCM_BITS: u32 = 9;

struct SfTree {
    left: Vec<u16>,
    right: Vec<u16>,
    leaf: Vec<i16>,
}

impl SfTree {
    fn build() -> Self {
        let mut left = vec![0u16];
        let mut right = vec![0u16];
        let mut leaf = vec![-1i16];
        for (idx, (&l, &c)) in SF_LEN.iter().zip(SF_CODE.iter()).enumerate() {
            let mut node = 0u16;
            for b in (0..l).rev() {
                let bit = ((c >> b) & 1) as u16;
                let existing = if bit == 0 {
                    left[node as usize]
                } else {
                    right[node as usize]
                };
                if existing == 0 {
                    let id = left.len() as u16;
                    left.push(0);
                    right.push(0);
                    leaf.push(-1);
                    if bit == 0 {
                        left[node as usize] = id;
                    } else {
                        right[node as usize] = id;
                    }
                    node = id;
                } else {
                    node = existing;
                }
            }
            leaf[node as usize] = idx as i16;
        }
        Self { left, right, leaf }
    }

    fn decode(&self, br: &mut BitReader<'_>) -> Result<i8> {
        let mut node = 0u16;
        loop {
            let bit = br.read_bit()?;
            let next = if bit {
                self.right[node as usize]
            } else {
                self.left[node as usize]
            };
            if next == 0 {
                return Err(Error::HuffmanInvalid);
            }
            node = next;
            let sym = self.leaf[node as usize];
            if sym >= 0 {
                return Ok(sym as i8 - 60);
            }
        }
    }
}

static SF_TREE: LazyLock<SfTree> = LazyLock::new(SfTree::build);

/// Absolute scalefactors / IS positions / PNS energies per (group, sfb).
#[derive(Debug, Clone)]
pub struct ScaleFactors {
    /// Spectrum scalefactor (valid on Huffman bands).
    pub sf: Vec<Vec<i32>>,
    /// Intensity stereo position (right channel IS bands).
    pub is_pos: Vec<Vec<i32>>,
    /// PNS `noise_nrg` (NOISE_HCB bands).
    pub noise_nrg: Vec<Vec<i32>>,
}

/// Decode Table 4.53 and accumulate three DPCM tracks (§4.6.2 / §4.6.8 / §4.6.13).
pub fn parse(
    br: &mut BitReader<'_>,
    ics: &IcsInfo,
    sfb_cb: &[Vec<u8>],
    global_gain: u8,
) -> Result<ScaleFactors> {
    let groups = ics.num_window_groups as usize;
    let max_sfb = ics.max_sfb as usize;
    let mut sf = vec![vec![0i32; max_sfb]; groups];
    let mut is_pos = vec![vec![0i32; max_sfb]; groups];
    let mut noise_nrg = vec![vec![0i32; max_sfb]; groups];
    let mut last_sf = i32::from(global_gain);
    let mut last_is = 0i32;
    let mut last_nrg = i32::from(global_gain) - NOISE_OFFSET - 256;
    let mut noise_pcm_flag = true;
    for g in 0..groups {
        for sfb in 0..max_sfb {
            let cb = *sfb_cb.get(g).and_then(|v| v.get(sfb)).unwrap_or(&ZERO_HCB);
            if cb == ZERO_HCB {
                continue;
            }
            if is_intensity(cb) {
                last_is += i32::from(SF_TREE.decode(br)?);
                is_pos[g][sfb] = last_is;
            } else if is_noise(cb) {
                if noise_pcm_flag {
                    noise_pcm_flag = false;
                    last_nrg += br.read(NOISE_PCM_BITS)? as i32;
                } else {
                    last_nrg += i32::from(SF_TREE.decode(br)?);
                }
                noise_nrg[g][sfb] = last_nrg;
            } else {
                last_sf += i32::from(SF_TREE.decode(br)?);
                sf[g][sfb] = last_sf;
            }
        }
    }
    Ok(ScaleFactors {
        sf,
        is_pos,
        noise_nrg,
    })
}

/// Table 4.A.1 decode of one DPCM delta (tests / encoder helpers).
#[cfg(test)]
pub fn decode_dpcm(br: &mut BitReader<'_>) -> Result<i8> {
    SF_TREE.decode(br)
}
