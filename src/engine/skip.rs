//! Skip FIL / DSE so a raw_data_block stays bit-aligned.
//! CCE is parsed completely here, then fenced by the decoder.
//! PCE is parsed (not skipped) in `channel_map::parse_pce`.

use super::bits::BitReader;
use super::error::Result;
use super::ics_body::parse_ics;
use super::section::ZERO_HCB;
use super::sf;

/// `fill_element()` count field (bytes of `extension_payload`).
pub fn fill_count(br: &mut BitReader<'_>) -> Result<u32> {
    let mut count = br.read(4)?;
    if count == 15 {
        count += br.read(8)?;
        count = count.saturating_sub(1);
    }
    Ok(count)
}

/// `fill_element()` — 4-bit count, optional 8-bit escape, then `count` bytes.
#[cfg(test)]
pub fn skip_fil(br: &mut BitReader<'_>) -> Result<()> {
    let count = fill_count(br)?;
    br.skip(count.saturating_mul(8))?;
    Ok(())
}

/// `data_stream_element()`.
pub fn skip_dse(br: &mut BitReader<'_>) -> Result<()> {
    let _tag = br.read(4)?;
    let align = br.read_bit()?;
    let mut count = br.read(8)?;
    if count == 255 {
        count += br.read(8)?;
    }
    if align {
        br.byte_align()?;
    }
    br.skip(count.saturating_mul(8))?;
    Ok(())
}

/// Parsed `coupling_channel_element()` (Table 4.4.8). Gains are consumed,
/// not applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CceSyntax {
    pub tag: u8,
    pub independent: bool,
    pub n_coupled: usize,
    pub n_gain_lists: usize,
}

/// Parse a CCE completely (FAAD2 Table 4.4.8 / FFmpeg `decode_cce`).
/// The decoder fences reconstruction with [`super::error::Error::UnsupportedCce`].
pub fn parse_cce(br: &mut BitReader<'_>, fs_index: u8, aot: u8) -> Result<CceSyntax> {
    let tag = br.read(4)? as u8;
    let independent = br.read_bit()?;
    let n_coupled = br.read(3)? as usize + 1;
    let mut n_gain_lists = 0usize;
    for _ in 0..n_coupled {
        n_gain_lists += 1;
        let is_cpe = br.read_bit()?;
        let _select = br.read(4)?;
        if is_cpe {
            let cc_l = br.read_bit()?;
            let cc_r = br.read_bit()?;
            if cc_l && cc_r {
                n_gain_lists += 1;
            }
        }
    }
    let _cc_domain = br.read_bit()?;
    let _sign = br.read_bit()?;
    let _scale = br.read(2)?;
    let body = parse_ics(br, fs_index, aot, None)?;
    for _ in 1..n_gain_lists {
        let cge = independent || br.read_bit()?;
        if cge {
            let _ = sf::decode_dpcm(br)?;
        } else {
            let groups = body.ics.num_window_groups as usize;
            let max_sfb = body.ics.max_sfb as usize;
            for g in 0..groups {
                let row = body.sections.sfb_cb.get(g);
                for sfb in 0..max_sfb {
                    let cb = row.and_then(|r| r.get(sfb)).copied().unwrap_or(ZERO_HCB);
                    if cb != ZERO_HCB {
                        let _ = sf::decode_dpcm(br)?;
                    }
                }
            }
        }
    }
    Ok(CceSyntax {
        tag,
        independent,
        n_coupled,
        n_gain_lists,
    })
}
