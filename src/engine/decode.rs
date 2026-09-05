//! Stream-level LC driver: `raw_data_block()` → planar PCM.

#[cfg(test)]
use super::adts::AdtsHeader;
use super::bits::BitReader;
use super::channel_map::{ElemKind, Element, PceChannelMap, map_planes, mono_mix, reorder};
use super::error::{Error, Result};
use super::extension_payload::ExtensionPayload;
use super::filterbank::Filterbank;
use super::ics_body::parse_ics_into;
use super::pns::Lcg;
use super::raw_data_block::IdSynEle;
use super::sbr_decoder::SbrDecoder;
use super::sbr_extension::SbrExtensionData;
use super::sbr_header::SbrHeader;
use super::section::SectionData;
use super::sf::ScaleFactors;
use super::skip::{fill_count, skip_cce, skip_dse};

/// Filterbank scale is ±32768; public decode maps with this to ~[-1, 1].
pub(crate) const INV_S16: f32 = 1.0 / 32768.0;

/// Owned per-frame output; used by engine unit tests. The product paths
/// (streaming + mono fast path) borrow planes via `frame_planes` instead.
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
    sbr: Option<SbrDecoder>,
    sbr_hdr: Option<SbrHeader>,
    sbr_active: bool,
    pub(crate) pcm_l: Vec<f32>,
    pub(crate) pcm_r: Vec<f32>,
    frame_ch: Vec<Vec<f32>>,
    n_ch: usize,
    pub(crate) quant: Vec<i32>,
    pub(crate) spec_l: Vec<f32>,
    pub(crate) spec_r: Vec<f32>,
    pub(crate) sections_l: SectionData,
    pub(crate) sections_r: SectionData,
    pub(crate) sf_l: ScaleFactors,
    pub(crate) sf_r: ScaleFactors,
    fast_mono: bool,
    /// Decoded elements of the current frame (multichannel path only).
    elems: Vec<Element>,
    /// Sticky `program_config_element()` channel map; wins over
    /// `channel_configuration` once seen (§4.4.2.4).
    pce: Option<PceChannelMap>,
    /// Per-channel filterbank state for the multichannel path. Mono/stereo
    /// keep using `fb_l` / `fb_r` directly.
    fb_pool: Vec<Filterbank>,
}

impl StreamDecoder {
    pub fn new() -> Self {
        Self::default()
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
        _num_raw_data_blocks: u8,
        payload: &[u8],
    ) -> Result<DecodedFrame> {
        let sample_rate =
            self.decode_into_bufs(aot, fs_index, sample_rate, channel_configuration, payload)?;
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

    /// Planes left by the last `decode_into_bufs` call, truncated to the
    /// channel count (filterbank scale unless reached via
    /// `decode_frame_scaled`).
    pub(crate) fn frame_planes(&self) -> &[Vec<f32>] {
        &self.frame_ch
    }

    fn decode_into_bufs(
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
        let core_aot = 2;
        let mut br = BitReader::new(payload);
        // Multichannel path: per-channel filterbanks, element→plane mapping.
        // Mono/stereo (cfg 1/2, no PCE) keep the fast path untouched.
        let multichannel =
            channel_configuration == 0 || channel_configuration >= 3 || self.pce.is_some();
        if multichannel {
            self.fast_mono = false;
        }
        self.n_ch = 0;
        self.elems.clear();
        let mut last_syn = IdSynEle::Sce;
        let mut pending_sbr: Option<Box<SbrExtensionData>> = None;
        loop {
            if br.bits_remaining() < 3 {
                break;
            }
            let id = IdSynEle::from_bits(br.read(3)? as u8);
            match id {
                IdSynEle::End => break,
                IdSynEle::Sce | IdSynEle::Lfe => {
                    last_syn = id;
                    let tag = br.read(4)? as u8;
                    if multichannel {
                        self.fb_swap_in(self.n_ch);
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
                    self.finish_sce(ics, tns.as_ref(), fs_index)?;
                    if multichannel {
                        self.fb_swap_in(self.n_ch);
                        let kind = if id == IdSynEle::Sce {
                            ElemKind::Sce
                        } else {
                            ElemKind::Lfe
                        };
                        self.elems.push(Element {
                            kind,
                            tag,
                            plane: self.n_ch,
                        });
                    }
                    self.push_pcm(false);
                }
                IdSynEle::Cpe => {
                    last_syn = IdSynEle::Cpe;
                    let tag = br.read(4)? as u8;
                    if multichannel {
                        self.fb_swap_pair(self.n_ch);
                    }
                    self.decode_cpe(&mut br, fs_index, core_aot, tag)?;
                    if multichannel {
                        self.fb_swap_pair(self.n_ch);
                        self.elems.push(Element {
                            kind: ElemKind::Cpe,
                            tag,
                            plane: self.n_ch,
                        });
                        self.push_pcm(false);
                        self.push_pcm(true);
                    } else if self.mix_down_mono {
                        let n = self.pcm_l.len().min(self.pcm_r.len());
                        for (l, r) in self.pcm_l.iter_mut().zip(self.pcm_r.iter()).take(n) {
                            *l = 0.5 * (*l + *r);
                        }
                        self.pcm_l.truncate(n);
                        self.push_pcm(false);
                    } else {
                        self.push_pcm(false);
                        self.push_pcm(true);
                    }
                }
                IdSynEle::Cce => skip_cce(&mut br, fs_index, core_aot)?,
                IdSynEle::Dse => skip_dse(&mut br)?,
                IdSynEle::Pce => {
                    self.pce = Some(super::channel_map::parse_pce(&mut br)?);
                }
                IdSynEle::Fil => {
                    let cnt = fill_count(&mut br)?;
                    let fs_sbr = sample_rate.saturating_mul(2);
                    let start = br.bit_position();
                    match ExtensionPayload::parse_with_sbr(
                        &mut br,
                        cnt,
                        last_syn,
                        fs_sbr,
                        self.sbr_hdr,
                    ) {
                        Ok(super::extension_payload::ExtensionPayloadOrSbr::Sbr(ext)) => {
                            self.sbr_hdr = Some(ext.header);
                            pending_sbr = Some(ext);
                        }
                        Ok(_) => {}
                        Err(e) if self.sbr_active => return Err(e),
                        Err(_) => {}
                    }
                    let used = br.bit_position().saturating_sub(start);
                    let need = u64::from(cnt).saturating_mul(8);
                    if used < need {
                        br.skip((need - used) as u32)?;
                    }
                }
            }
        }
        if self.fast_mono && pending_sbr.is_none() && !self.sbr_active {
            return Ok(sample_rate);
        }
        if self.fast_mono {
            self.fast_mono = false;
            self.n_ch = 0;
            self.push_pcm(false);
            self.fast_mono = true;
        }
        self.frame_ch.truncate(self.n_ch);
        if multichannel && !self.frame_ch.is_empty() {
            let order = map_planes(&self.elems, self.pce.as_ref(), channel_configuration);
            reorder(&mut self.frame_ch, &order);
            if self.mix_down_mono {
                // One rule: arithmetic mean of the non-LFE planes.
                let mono = mono_mix(&self.frame_ch, &order);
                self.pcm_l.clear();
                self.pcm_l.extend_from_slice(&mono);
                self.frame_ch.truncate(1);
                self.frame_ch[0] = mono;
            }
        }
        let planar = std::mem::take(&mut self.frame_ch);
        let (planar, sample_rate) = self.apply_sbr(planar, sample_rate, pending_sbr)?;
        self.frame_ch = planar;
        Ok(sample_rate)
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
        let rate =
            self.decode_into_bufs(aot, fs_index, sample_rate, channel_configuration, payload);
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

    /// Multichannel: swap per-channel filterbank state for decode-order
    /// channel `ch` into `fb_l` (call again after the element to swap back).
    fn fb_swap_in(&mut self, ch: usize) {
        while self.fb_pool.len() <= ch {
            self.fb_pool.push(Filterbank::new());
        }
        std::mem::swap(&mut self.fb_l, &mut self.fb_pool[ch]);
    }

    /// Same for a CPE: channels `ch`/`ch+1` into `fb_l`/`fb_r`.
    fn fb_swap_pair(&mut self, ch: usize) {
        while self.fb_pool.len() <= ch + 1 {
            self.fb_pool.push(Filterbank::new());
        }
        std::mem::swap(&mut self.fb_l, &mut self.fb_pool[ch]);
        std::mem::swap(&mut self.fb_r, &mut self.fb_pool[ch + 1]);
    }

    fn push_pcm(&mut self, right: bool) {
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

    fn apply_sbr(
        &mut self,
        planar: Vec<Vec<f32>>,
        sample_rate: u32,
        pending: Option<Box<SbrExtensionData>>,
    ) -> Result<(Vec<Vec<f32>>, u32)> {
        if pending.is_some() {
            self.sbr_active = true;
        }
        if !self.sbr_active || planar.is_empty() {
            return Ok((planar, sample_rate));
        }
        let n_ch = planar.len();
        let fs_sbr = sample_rate.saturating_mul(2);
        if self.sbr.is_none() {
            self.sbr = Some(SbrDecoder::new(fs_sbr, n_ch)?);
        }
        let dec = self.sbr.as_mut().ok_or(Error::SbrQmfInvalid)?;
        let core_f64: Vec<Vec<f64>> = planar
            .iter()
            .map(|ch| ch.iter().copied().map(f64::from).collect())
            .collect();
        let core: Vec<&[f64]> = core_f64.iter().map(Vec::as_slice).collect();
        let out = match pending.as_deref() {
            Some(ext) => dec.process_frame(ext, &core)?,
            None => dec.upsample_frame(&core)?,
        };
        Ok((
            super::sbr_decoder::planes_f32(out, self.mix_down_mono),
            fs_sbr,
        ))
    }
}
