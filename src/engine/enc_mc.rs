//! Multichannel AAC-LC element orchestration (TASK-114, spec in
//! `lab/quality/MC_ENC.md`): `channel_configuration` 3–7 as one
//! [`LcEncoder`] per syntactic element, spliced into one
//! `raw_data_block` in ISO order — SCE(C), CPE(front), [CPE(side)],
//! CPE(back) or SCE(BC), LFE.
//!
//! Input planes use the decoder's public order (TASK-62): 3.0 FL FR FC,
//! 4.0 + BC, 5.0 FL FR FC BL BR, 5.1 FL FR FC LFE BL BR, 7.1 FL FR FC LFE
//! BL BR SL SR. The whole-stream bitrate splits by fixed element weights
//! (SCE 1, CPE 2, LFE 0.1); every element runs its own rate loop and
//! keeps the 6144 bits/channel cap. LFE: long windows, no TNS, 120 Hz.

use super::bits::BitWriter;
use super::enc_frame::LcEncoder;
use super::error::{Error, Result};
use super::swb::LONG_WINDOW_LEN;

/// Syntactic element kind with its 3-bit `id_syn_ele`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Sce = 0,
    Cpe = 1,
    Lfe = 3,
}

/// One element: kind, `element_instance_tag`, source plane indices.
type Route = (Kind, u8, [usize; 2]);

/// Element routing per plane count (bitstream order).
fn routes(planes: usize) -> Option<(u8, &'static [Route])> {
    use Kind::{Cpe, Lfe, Sce};
    Some(match planes {
        3 => (3, &[(Sce, 0, [2, 2]), (Cpe, 0, [0, 1])]),
        4 => (4, &[(Sce, 0, [2, 2]), (Cpe, 0, [0, 1]), (Sce, 1, [3, 3])]),
        5 => (5, &[(Sce, 0, [2, 2]), (Cpe, 0, [0, 1]), (Cpe, 1, [3, 4])]),
        6 => (
            6,
            &[
                (Sce, 0, [2, 2]),
                (Cpe, 0, [0, 1]),
                (Cpe, 1, [4, 5]),
                (Lfe, 0, [3, 3]),
            ],
        ),
        8 => (
            7,
            &[
                (Sce, 0, [2, 2]),
                (Cpe, 0, [0, 1]),
                (Cpe, 1, [6, 7]),
                (Cpe, 2, [4, 5]),
                (Lfe, 0, [3, 3]),
            ],
        ),
        _ => return None,
    })
}

/// Budget weight in tenths (SCE 1, CPE 2, LFE 0.1).
fn weight(kind: Kind) -> u64 {
    match kind {
        Kind::Sce => 10,
        Kind::Cpe => 20,
        Kind::Lfe => 1,
    }
}

/// Multichannel LC encoder: one `raw_data_block` per 1024 samples/plane.
#[cfg_attr(not(test), allow(dead_code))] // public wiring is TASK-116
pub struct McEncoder {
    config: u8,
    routes: &'static [Route],
    elems: Vec<LcEncoder>,
    fs_index: u8,
}

#[cfg_attr(not(test), allow(dead_code))]
impl McEncoder {
    /// `planes` must be 3, 4, 5, 6 or 8; `bitrate_bps` is whole-stream.
    pub fn new(sample_rate: u32, planes: usize, bitrate_bps: u32) -> Result<Self> {
        let (config, routes) =
            routes(planes).ok_or(Error::Format("LC encoder: planes must be 3, 4, 5, 6 or 8"))?;
        let total: u64 = routes.iter().map(|r| weight(r.0)).sum();
        let mut elems = Vec::with_capacity(routes.len());
        for &(kind, _, _) in routes {
            let ch = if kind == Kind::Cpe { 2 } else { 1 };
            let share = (u64::from(bitrate_bps) * weight(kind) / total).max(1);
            let cap = u64::from(super::enc_frame::max_bitrate_bps(sample_rate, ch));
            let mut e = LcEncoder::new(sample_rate, ch, share.min(cap) as u32)?;
            if kind == Kind::Lfe {
                e.set_lfe();
            }
            elems.push(e);
        }
        let fs_index = elems.first().map_or(0, LcEncoder::fs_index);
        Ok(Self {
            config,
            routes,
            elems,
            fs_index,
        })
    }

    /// `channel_configuration` (3–7) for the ADTS header / ASC.
    #[must_use]
    pub fn channel_configuration(&self) -> u8 {
        self.config
    }

    #[must_use]
    pub fn fs_index(&self) -> u8 {
        self.fs_index
    }

    /// Encode 1024 samples per plane (public plane order) into `out`.
    pub fn encode_into(&mut self, pcm: &[&[f32]], out: &mut Vec<u8>) -> Result<()> {
        let need = self.routes.iter().map(|r| r.2[1]).max().unwrap_or(0) + 1;
        if pcm.len() != need || pcm.iter().any(|p| p.len() != LONG_WINDOW_LEN) {
            return Err(Error::Format("LC encoder: bad frame shape"));
        }
        let mut w = BitWriter::from_vec(std::mem::take(out));
        let mut padded = 0usize;
        for (enc, &(kind, tag, src)) in self.elems.iter_mut().zip(self.routes.iter()) {
            if kind == Kind::Cpe {
                enc.encode_into(&[pcm[src[0]], pcm[src[1]]])?;
            } else {
                enc.encode_into(&[pcm[src[0]]])?;
            }
            let rdb = enc.payload();
            padded += rdb.len();
            // The element's own block is `id tag body END pad`: END (111)
            // holds the last set bit, so the body is bits [7, last − 2).
            let last = last_set_bit(rdb).ok_or(Error::Format("LC encoder: empty element"))?;
            w.write(kind as u32, 3);
            w.write(u32::from(tag), 4);
            append_bits(&mut w, rdb, 7, last - 2);
        }
        w.write(7, 3); // END
        let mut bytes = w.finish();
        // Keep the elements' ABR stuffing: unused bytes after END.
        let target = padded.min(super::enc_frame::MAX_PAYLOAD_BYTES);
        if bytes.len() < target {
            bytes.resize(target, 0);
        }
        *out = bytes;
        Ok(())
    }
}

/// Index (MSB-first) of the last set bit.
fn last_set_bit(bytes: &[u8]) -> Option<usize> {
    let i = bytes.iter().rposition(|&b| b != 0)?;
    Some(i * 8 + 7 - bytes[i].trailing_zeros() as usize)
}

/// Append bits `[from, to)` of `bytes` (MSB-first) to `w`.
fn append_bits(w: &mut BitWriter, bytes: &[u8], from: usize, to: usize) {
    let mut at = from;
    while at < to {
        let in_byte = 8 - at % 8;
        let n = in_byte.min(to - at);
        let v = (u32::from(bytes[at / 8]) >> (in_byte - n)) & ((1u32 << n) - 1);
        w.write(v, n as u32);
        at += n;
    }
}

#[cfg(test)]
#[path = "enc_mc_tests.rs"]
mod enc_mc_tests;
