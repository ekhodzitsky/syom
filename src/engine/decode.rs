//! Stream-level LC driver: `raw_data_block()` → planar PCM.

#[cfg(test)]
use super::adts::AdtsHeader;
use super::bits::BitReader;
use super::cce::{PendingChan, parse_cce};
use super::channel_map::{ElemKind, Element, PceChannelMap, map_planes_into, mono_mix, reorder};
use super::error::{Error, Result};
use super::fb_pool::{FbPool, reject_dup};
use super::filterbank::Filterbank;
use super::ics_body::parse_ics_into;
use super::pns::Lcg;
use super::raw_data_block::IdSynEle;
use super::sbr_attach::SbrPool;
use super::section::SectionData;
use super::sf::ScaleFactors;
use super::skip::{fill_count, skip_dse};

/// Filterbank scale is ±32768; public decode maps with this to ~[-1, 1].
pub(crate) const INV_S16: f32 = 1.0 / 32768.0;

/// Owned per-frame output for engine tests (product paths borrow `frame_planes`).
#[cfg(test)]
#[derive(Debug, Clone, PartialEq)]
pub struct DecodedFrame {
    pub planar: Vec<Vec<f32>>,
    pub channels: usize,
    pub sample_rate: u32,
}

#[derive(Debug, Default)]
pub struct StreamDecoder {
    pub(crate) fb_l: Filterbank,
    pub(crate) fb_r: Filterbank,
    pub(crate) rng: Lcg,
    pub mix_down_mono: bool,
    sbr_pool: SbrPool,
    pub(crate) sbr_active: bool,
    sbr_declared: bool,
    ps_declared: bool,
    sbr_out_rate: Option<u32>,
    pub(crate) pcm_l: Vec<f32>,
    pub(crate) pcm_r: Vec<f32>,
    pub(crate) frame_ch: Vec<Vec<f32>>,
    pub(crate) n_ch: usize,
    pub(crate) quant: Vec<i32>,
    pub(crate) spec_l: Vec<f32>,
    pub(crate) spec_r: Vec<f32>,
    pub(crate) sections_l: SectionData,
    pub(crate) sections_r: SectionData,
    pub(crate) sf_l: ScaleFactors,
    pub(crate) sf_r: ScaleFactors,
    pub(crate) fast_mono: bool,
    pub(crate) elems: Vec<Element>,
    pce: Option<PceChannelMap>,
    pub(crate) fb_pool: FbPool,
    pub(crate) pending: Vec<PendingChan>,
    pub(crate) cces: Vec<super::cce::CcePayload>,
    pub(crate) last_rdb_bytes: usize,
    last_meta: crate::layout::FrameMeta,
    /// Recycled spectral buffers (TASK-78). `stash_chan` swaps filled
    /// `spec_l`/`spec_r` out; finish returns them here instead of dropping.
    pub(crate) spec_pool: Vec<Vec<f32>>,
    /// Reused layout order (TASK-78); filled by `map_planes_into`.
    pub(crate) plane_order: Vec<super::channel_map::PlaneMap>,
    /// Independent-CCE IMDCT scratch (TASK-78).
    pub(crate) cce_pcm: Vec<f32>,
    pub(crate) pns_shared: Vec<f32>,
}

impl StreamDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Seed mapping from an ASC-embedded PCE (M4A/LATM). In-band PCE still wins.
    pub(crate) fn set_config_pce(&mut self, pce: PceChannelMap) {
        self.pce = Some(pce);
    }

    /// Seed HE/PS from ASC so output rate does not wait on a FIL payload.
    pub(crate) fn set_he_config(&mut self, sbr: bool, ps: bool, output_rate: u32) {
        self.sbr_declared = sbr;
        self.ps_declared = ps;
        if sbr && output_rate != 0 {
            self.sbr_active = true;
            self.sbr_out_rate = Some(output_rate);
        }
    }

    #[cfg(test)]
    pub(crate) fn he_config(&self) -> (bool, bool, Option<u32>) {
        (self.sbr_declared, self.ps_declared, self.sbr_out_rate)
    }

    #[cfg(test)]
    pub fn decode_frame(&mut self, header: &AdtsHeader, payload: &[u8]) -> Result<DecodedFrame> {
        self.decode_raw_data_block(
            header.audio_object_type(),
            header.sampling_frequency_index,
            header.sample_rate(),
            header.channel_configuration,
            header.number_of_raw_data_blocks_in_frame,
            payload,
        )
    }

    #[cfg(test)]
    pub fn decode_raw_data_block(
        &mut self,
        aot: u8,
        fs_index: u8,
        sample_rate: u32,
        channel_configuration: u8,
        num_raw_data_blocks: u8,
        payload: &[u8],
    ) -> Result<DecodedFrame> {
        let sample_rate = if num_raw_data_blocks <= 1 {
            self.decode_into_bufs(aot, fs_index, sample_rate, channel_configuration, payload)?
        } else {
            self.decode_adts_blocks(
                aot,
                fs_index,
                sample_rate,
                channel_configuration,
                num_raw_data_blocks,
                true,
                payload,
            )?
        };
        Ok(DecodedFrame {
            planar: self.frame_ch.clone(),
            channels: self.frame_ch.len(),
            sample_rate,
        })
    }

    /// Streaming seam: decode one `raw_data_block()` and scale the planes
    /// to ~[-1, 1] **in place** — borrow them with [`Self::frame_planes`]
    /// instead of cloning a [`DecodedFrame`].
    pub(crate) fn decode_frame_scaled(
        &mut self,
        aot: u8,
        fs_index: u8,
        sample_rate: u32,
        channel_configuration: u8,
        payload: &[u8],
    ) -> Result<u32> {
        let rate =
            self.decode_into_bufs(aot, fs_index, sample_rate, channel_configuration, payload)?;
        for ch in &mut self.frame_ch {
            for v in ch.iter_mut() {
                *v *= INV_S16;
            }
        }
        Ok(rate)
    }

    /// Last `decode_into_bufs` planes (filterbank scale unless scaled).
    pub(crate) fn frame_planes(&self) -> &[Vec<f32>] {
        &self.frame_ch
    }

    pub(crate) fn decode_into_bufs(
        &mut self,
        aot: u8,
        fs_index: u8,
        sample_rate: u32,
        channel_configuration: u8,
        payload: &[u8],
    ) -> Result<u32> {
        if aot != 2 && aot != 5 && aot != 29 {
            return Err(Error::UnsupportedAot(aot));
        }
        self.refill_spec_bufs();
        let core_aot = 2;
        let mut br = BitReader::new(payload);
        // cfg 0 / ≥3 / PCE: per-channel FBs. cfg 1/2 keep the fast path.
        let multichannel =
            channel_configuration == 0 || channel_configuration >= 3 || self.pce.is_some();
        if multichannel {
            self.fast_mono = false;
        }
        self.n_ch = 0;
        self.elems.clear();
        self.pending.clear();
        self.cces.clear();
        self.fb_pool.begin_frame();
        self.sbr_pool.begin_frame();
        let mut last_elem = None;
        let mut pending_sbrs = Vec::new();
        loop {
            if br.bits_remaining() < 3 {
                break;
            }
            let id = IdSynEle::from_bits(br.read(3)? as u8);
            match id {
                IdSynEle::End => break,
                IdSynEle::Sce | IdSynEle::Lfe => {
                    let tag = br.read(4)? as u8;
                    let kind = if id == IdSynEle::Sce {
                        ElemKind::Sce
                    } else {
                        ElemKind::Lfe
                    };
                    last_elem = Some((kind, tag));
                    if multichannel {
                        reject_dup(&self.elems, kind, tag)?;
                    }
                    let (ics, tns) = parse_ics_into(
                        &mut br,
                        fs_index,
                        core_aot,
                        None,
                        &mut self.quant,
                        &mut self.spec_l,
                        &mut self.sections_l,
                        &mut self.sf_l,
                    )?;
                    self.finish_sce_pns(&ics, fs_index)?;
                    self.stash_chan(kind, tag, 0, ics, tns, true, true);
                }
                IdSynEle::Cpe => {
                    let tag = br.read(4)? as u8;
                    last_elem = Some((ElemKind::Cpe, tag));
                    if multichannel {
                        reject_dup(&self.elems, ElemKind::Cpe, tag)?;
                    }
                    let (ics_l, tns_l, ics_r, tns_r) =
                        self.decode_cpe(&mut br, fs_index, core_aot, tag)?;
                    self.stash_chan(ElemKind::Cpe, tag, 0, ics_l, tns_l, true, true);
                    self.stash_chan(ElemKind::Cpe, tag, 1, ics_r, tns_r, false, true);
                }
                IdSynEle::Cce => {
                    self.cces.push(parse_cce(&mut br, fs_index, core_aot)?);
                }
                IdSynEle::Dse => skip_dse(&mut br)?,
                IdSynEle::Pce => {
                    self.pce = Some(super::channel_map::parse_pce(&mut br)?);
                }
                IdSynEle::Fil => {
                    let cnt = fill_count(&mut br)?;
                    let fs_sbr = self.sbr_out_rate.unwrap_or(sample_rate.saturating_mul(2));
                    super::sbr_attach::ingest_fil(
                        &mut br,
                        cnt,
                        last_elem,
                        fs_sbr,
                        &self.sbr_pool,
                        &mut pending_sbrs,
                    )?;
                }
            }
        }
        br.byte_align()?;
        self.last_rdb_bytes = (br.bit_position() / 8) as usize;
        let he = self.sbr_active || !pending_sbrs.is_empty();
        let was_fast = self.fast_mono;
        if he {
            self.fast_mono = false;
        }
        self.finish_pending(fs_index, multichannel, he)?;
        if was_fast && !he {
            self.fb_pool.retain_seen();
            self.fast_mono = was_fast;
            self.store_meta(channel_configuration, sample_rate, sample_rate, None, 1);
            return Ok(sample_rate);
        }
        self.frame_ch.truncate(self.n_ch);
        let mut order_buf = [super::channel_map::PlaneMap::default(); 8];
        let mut n_order = 0usize;
        let has_order = self.pce.is_some() || (multichannel && !self.frame_ch.is_empty());
        if has_order {
            map_planes_into(
                &mut self.plane_order,
                &self.elems,
                self.pce.as_ref(),
                channel_configuration,
                fs_index,
            )?;
            n_order = self.plane_order.len().min(8);
            order_buf[..n_order].copy_from_slice(&self.plane_order[..n_order]);
        }
        let order = has_order.then_some(&order_buf[..n_order]);
        if !he {
            if let Some(order) = order {
                reorder(&mut self.frame_ch, order);
                if self.mix_down_mono {
                    let mono = mono_mix(&self.frame_ch, order);
                    self.pcm_l.clear();
                    self.pcm_l.extend_from_slice(&mono);
                    self.frame_ch.truncate(1);
                    self.frame_ch[0] = mono;
                }
            }
            self.fb_pool.retain_seen();
            self.fast_mono = was_fast;
            self.store_meta(
                channel_configuration,
                sample_rate,
                sample_rate,
                order,
                self.frame_ch.len(),
            );
            return Ok(sample_rate);
        }
        let core = sample_rate;
        let planar = std::mem::take(&mut self.frame_ch);
        let (planar, out_rate) = super::sbr_attach::apply_and_layout(
            &mut self.sbr_pool,
            &self.elems,
            planar,
            sample_rate,
            self.sbr_out_rate,
            &mut pending_sbrs,
            &mut self.sbr_active,
            self.mix_down_mono,
            order,
        )?;
        self.frame_ch = planar;
        self.fb_pool.retain_seen();
        self.sbr_pool.retain_seen();
        self.fast_mono = was_fast;
        self.store_meta(
            channel_configuration,
            core,
            out_rate,
            order,
            self.frame_ch.len(),
        );
        Ok(out_rate)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn decode_raw_mono_f32(
        &mut self,
        aot: u8,
        fs_index: u8,
        sample_rate: u32,
        channel_configuration: u8,
        _num_raw_data_blocks: u8,
        payload: &[u8],
        dst: &mut Vec<f32>,
    ) -> Result<u32> {
        let was = self.mix_down_mono;
        self.mix_down_mono = true;
        self.fast_mono = true;
        let rate = self.decode_adts_blocks(
            aot,
            fs_index,
            sample_rate,
            channel_configuration,
            _num_raw_data_blocks,
            true,
            payload,
        );
        self.fast_mono = false;
        self.mix_down_mono = was;
        let rate = rate?;
        let src = if self.sbr_active {
            self.frame_ch.first().map(Vec::as_slice).unwrap_or(&[])
        } else {
            self.pcm_l.as_slice()
        };
        dst.extend(src.iter().map(|&v| v * INV_S16));
        Ok(rate)
    }

    pub(crate) fn push_pcm(&mut self, right: bool) {
        if self.fast_mono {
            self.n_ch = 1;
            return;
        }
        let i = self.n_ch;
        let src = if right { &self.pcm_r } else { &self.pcm_l };
        if i < self.frame_ch.len() {
            self.frame_ch[i].clear();
            self.frame_ch[i].extend_from_slice(src);
        } else {
            self.frame_ch.push(src.to_vec());
        }
        self.n_ch += 1;
    }
}

#[path = "decode_meta.rs"]
mod meta;

#[path = "decode_reset.rs"]
mod reset;
