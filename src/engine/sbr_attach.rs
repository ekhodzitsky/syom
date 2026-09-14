//! Per-element SBR state, keyed by `(kind, tag)` like [`super::fb_pool`].
//!
//! A FIL `EXT_SBR_DATA` payload attaches to the preceding SCE/CPE/LFE.
//! Each identity keeps its own QMF/header history. Speech downmix runs
//! after HE reconstruction.
//!
//! Header lifecycle (TASK-39 / F14):
//! - Delayed first header: HE declared, no FIL → 2× upsample (no SBR data).
//! - First SBR payload must carry `bs_header_flag=1`; a clear flag with no
//!   prior header is an error, not LC success.
//! - Legal geometry change resets band tables (`SbrHeader::band_geometry_changed`).
//! - A missing FIL after HE is active upsamples and keeps QMF history.
//! - Truncated or malformed SBR/FIL is always an error, even before the
//!   first successful payload (`sbr_active` does not gate that).

use super::bits::BitReader;
use super::channel_map::{ElemKind, Element, PlaneMap, mono_mix, reorder};
use super::error::{Error, Result};
use super::extension_payload::{ExtensionPayload, ExtensionPayloadOrSbr};
use super::raw_data_block::IdSynEle;
use super::sbr_decoder::SbrDecoder;
use super::sbr_extension::SbrExtensionData;
use super::sbr_header::SbrHeader;

#[derive(Debug)]
struct Slot {
    kind: ElemKind,
    tag: u8,
    seen: bool,
    dec: SbrDecoder,
    hdr: Option<SbrHeader>,
}

#[derive(Debug, Default)]
pub(crate) struct SbrPool {
    slots: Vec<Slot>,
}

impl SbrPool {
    pub(crate) fn begin_frame(&mut self) {
        for s in &mut self.slots {
            s.seen = false;
        }
    }

    pub(crate) fn retain_seen(&mut self) {
        self.slots.retain(|s| s.seen);
    }

    pub(crate) fn prev_header(&self, kind: ElemKind, tag: u8) -> Option<SbrHeader> {
        self.slots
            .iter()
            .find(|s| s.kind == kind && s.tag == tag)
            .and_then(|s| s.hdr)
    }

    fn slot(&mut self, kind: ElemKind, tag: u8, fs_sbr: u32) -> Result<&mut Slot> {
        if let Some(i) = self
            .slots
            .iter()
            .position(|s| s.kind == kind && s.tag == tag)
        {
            self.slots[i].seen = true;
            return Ok(&mut self.slots[i]);
        }
        let n_ch = if kind == ElemKind::Cpe { 2 } else { 1 };
        self.slots.push(Slot {
            kind,
            tag,
            seen: true,
            dec: SbrDecoder::new(fs_sbr, n_ch)?,
            hdr: None,
        });
        let i = self.slots.len() - 1;
        Ok(&mut self.slots[i])
    }
}

pub(crate) fn ingest_fil(
    br: &mut BitReader<'_>,
    cnt: u32,
    last_elem: Option<(ElemKind, u8)>,
    fs_sbr: u32,
    pool: &SbrPool,
    pending: &mut Vec<((ElemKind, u8), Box<SbrExtensionData>)>,
) -> Result<()> {
    let start = br.bit_position();
    let id_aac = match last_elem {
        Some((ElemKind::Cpe, _)) => IdSynEle::Cpe,
        _ => IdSynEle::Sce,
    };
    let prev = last_elem.and_then(|(k, t)| pool.prev_header(k, t));
    let parsed = ExtensionPayload::parse_with_sbr(br, cnt, id_aac, fs_sbr, prev);
    let attach_res = match parsed {
        Ok(ExtensionPayloadOrSbr::Sbr(ext)) => attach(last_elem, pending, ext),
        Ok(_) => Ok(()),
        Err(e) => Err(e),
    };
    let used = br.bit_position().saturating_sub(start);
    let need = u64::from(cnt).saturating_mul(8);
    if used < need {
        let _ = br.skip((need - used) as u32);
    }
    attach_res
}

fn attach(
    last: Option<(ElemKind, u8)>,
    pending: &mut Vec<((ElemKind, u8), Box<SbrExtensionData>)>,
    ext: Box<SbrExtensionData>,
) -> Result<()> {
    let key = last.ok_or(Error::Format("SBR without channel element"))?;
    if pending.iter().any(|(k, _)| *k == key) {
        return Err(Error::Format("duplicate SBR extension"));
    }
    pending.push((key, ext));
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_and_layout(
    pool: &mut SbrPool,
    elems: &[Element],
    planar: Vec<Vec<f32>>,
    sample_rate: u32,
    sbr_out_rate: Option<u32>,
    pending: &mut Vec<((ElemKind, u8), Box<SbrExtensionData>)>,
    sbr_active: &mut bool,
    mix_down_mono: bool,
    order: Option<&[PlaneMap]>,
) -> Result<(Vec<Vec<f32>>, u32)> {
    if !pending.is_empty() {
        *sbr_active = true;
    }
    if !*sbr_active || planar.is_empty() {
        return Ok((planar, sample_rate));
    }
    let fs_sbr = match sbr_out_rate {
        Some(out) if out == sample_rate => {
            let mut planar = planar;
            layout(&mut planar, order, mix_down_mono, false);
            return Ok((planar, sample_rate));
        }
        Some(out) if out == sample_rate.saturating_mul(2) => out,
        Some(_) => return Err(Error::Format("SBR output rate must be 1x or 2x core")),
        None => sample_rate.saturating_mul(2),
    };
    let mut out = Vec::new();
    for e in elems {
        let n = e.planes();
        let end = e.plane.saturating_add(n);
        if end > planar.len() {
            return Err(Error::Format("SBR element plane missing"));
        }
        let ext = take_pending(pending, (e.kind, e.tag));
        let core_f64: Vec<Vec<f64>> = planar[e.plane..end]
            .iter()
            .map(|ch| ch.iter().copied().map(f64::from).collect())
            .collect();
        let core: Vec<&[f64]> = core_f64.iter().map(Vec::as_slice).collect();
        let slot = pool.slot(e.kind, e.tag, fs_sbr)?;
        let rendered = match ext.as_deref() {
            Some(ext) => {
                slot.hdr = Some(ext.header);
                slot.dec.process_frame(ext, &core)?
            }
            None => slot.dec.upsample_frame(&core)?,
        };
        if rendered.len() != n && !(elems.len() == 1 && n == 1 && rendered.len() == 2) {
            return Err(Error::Format("SBR plane count mismatch"));
        }
        out.extend(
            rendered
                .into_iter()
                .map(|ch| ch.into_iter().map(|x| x as f32).collect::<Vec<f32>>()),
        );
    }
    let lone = out.len() == 1;
    layout(&mut out, order, mix_down_mono, lone);
    Ok((out, fs_sbr))
}

fn take_pending(
    pending: &mut Vec<((ElemKind, u8), Box<SbrExtensionData>)>,
    key: (ElemKind, u8),
) -> Option<Box<SbrExtensionData>> {
    pending
        .iter()
        .position(|(k, _)| *k == key)
        .map(|i| pending.remove(i).1)
}

fn layout(
    planar: &mut Vec<Vec<f32>>,
    order: Option<&[PlaneMap]>,
    mix_down_mono: bool,
    duplicate_mono: bool,
) {
    if let Some(order) = order {
        reorder(planar, order);
    }
    if mix_down_mono {
        if let Some(order) = order {
            let mono = mono_mix(planar, order);
            planar.clear();
            planar.push(mono);
        } else if let Some((left, rest)) = planar.split_first_mut() {
            if let Some(right) = rest.first() {
                for (a, b) in left.iter_mut().zip(right.iter()) {
                    *a = 0.5 * (*a + *b);
                }
            }
            planar.truncate(1);
        }
    } else if duplicate_mono && planar.len() == 1 {
        planar.push(planar[0].clone());
    }
}
