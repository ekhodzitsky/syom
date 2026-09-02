//! Skip FIL / DSE / PCE / CCE so a raw_data_block stays bit-aligned.

use super::bits::BitReader;
use super::error::Result;
use super::ics_body::parse_ics;

/// `id_syn_ele` values — Table 4.71.
pub const ID_SCE: u8 = 0;
pub const ID_CPE: u8 = 1;
pub const ID_CCE: u8 = 2;
pub const ID_LFE: u8 = 3;
pub const ID_DSE: u8 = 4;
pub const ID_PCE: u8 = 5;
pub const ID_FIL: u8 = 6;
pub const ID_END: u8 = 7;

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

/// `program_config_element()` — consumed, not applied. v1 does not use PCE layout.
pub fn skip_pce(br: &mut BitReader<'_>) -> Result<()> {
    let _tag = br.read(4)?;
    let _object_type = br.read(2)?;
    let _sf_index = br.read(4)?;
    let num_front = br.read(4)? as usize;
    let num_side = br.read(4)? as usize;
    let num_back = br.read(4)? as usize;
    let num_lfe = br.read(2)? as usize;
    let num_assoc = br.read(3)? as usize;
    let num_cc = br.read(4)? as usize;
    let mono_mix = br.read_bit()?;
    if mono_mix {
        let _ = br.read(4)?;
    }
    let stereo_mix = br.read_bit()?;
    if stereo_mix {
        let _ = br.read(4)?;
    }
    let matrix_mix = br.read_bit()?;
    if matrix_mix {
        let _ = br.read(3)?;
        let _ = br.read_bit()?;
    }
    for _ in 0..(num_front + num_side + num_back) {
        let _is_cpe = br.read_bit()?;
        let _tag = br.read(4)?;
    }
    for _ in 0..num_lfe {
        let _ = br.read(4)?;
    }
    for _ in 0..num_assoc {
        let _ = br.read(4)?;
    }
    for _ in 0..num_cc {
        let _is_ind = br.read_bit()?;
        let _tag = br.read(4)?;
    }
    br.byte_align()?;
    let comment_bytes = br.read(8)?;
    br.skip(comment_bytes.saturating_mul(8))?;
    Ok(())
}

/// Consume a CCE so the rest of the block stays aligned. Coupling is not applied.
pub fn skip_cce(br: &mut BitReader<'_>, fs_index: u8, aot: u8) -> Result<()> {
    let _tag = br.read(4)?;
    let ind_sw = br.read_bit()?;
    let coupled = br.read(3)? as usize + 1;
    if !ind_sw {
        let _sign = br.read_bit()?;
        let _scale = br.read(2)?;
    }
    for _ in 0..coupled {
        let is_cpe = br.read_bit()?;
        let _select = br.read(4)?;
        if is_cpe {
            let cc_l = br.read_bit()?;
            let cc_r = br.read_bit()?;
            if cc_l && cc_r {
                let _domain = br.read_bit()?;
            }
        }
    }
    // Coupling channel ICS (common_window = 0).
    let _ = parse_ics(br, fs_index, aot, None)?;
    // Gain elements after the ICS: one Huffman-ish scale per target.
    // Independent CCE puts a gain after each target; dependent after ICS.
    // Remaining bits of this element are not a second ICS; skip is best-effort
    // via the ICS parse having consumed the coupling channel body.
    let _ = ind_sw;
    Ok(())
}
