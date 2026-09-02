//! Stream-level LC driver: `raw_data_block()` → planar PCM.

use super::adts::AdtsHeader;
use super::bits::BitReader;
use super::error::{Error, Result};
use super::extension_payload::ExtensionPayload;
use super::filterbank::Filterbank;
use super::ics_body::{ChannelBody, parse_ics};
use super::pns::{self, Lcg};
use super::raw_data_block::IdSynEle;
use super::sbr_decoder::SbrDecoder;
use super::sbr_extension::SbrExtensionData;
use super::sbr_header::SbrHeader;
use super::skip::{
    ID_CCE, ID_CPE, ID_DSE, ID_END, ID_FIL, ID_LFE, ID_PCE, ID_SCE, fill_count, skip_cce, skip_dse,
    skip_pce,
};
use super::stereo::{self, MsInfo};
use super::tns;
use std::collections::HashMap;

/// One decoded frame at filterbank scale (±32768).
#[derive(Debug, Clone, PartialEq)]
pub struct DecodedFrame {
    /// Unused on the product path (planar f64 is consumed).
    pub pcm: Vec<i16>,
    /// Per-channel time-domain samples.
    pub planar: Vec<Vec<f64>>,
    /// Channel count of `planar`.
    pub channels: usize,
    /// Core sample rate in Hz.
    pub sample_rate: u32,
}

/// Stateful LC decoder. One instance per stream.
#[derive(Debug, Default)]
pub struct StreamDecoder {
    banks: HashMap<(u8, u8), Filterbank>,
    rng: Lcg,
    /// Mix CPE to one channel before returning (product STT).
    pub mix_down_mono: bool,
    sbr: Option<SbrDecoder>,
    sbr_hdr: Option<SbrHeader>,
    sbr_active: bool,
}

impl StreamDecoder {
    /// Fresh overlap state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Decode one ADTS `raw_data_block` payload.
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

    /// Decode one `raw_data_block` given explicit ASC/ADTS geometry.
    pub fn decode_raw_data_block(
        &mut self,
        aot: u8,
        fs_index: u8,
        sample_rate: u32,
        _channel_configuration: u8,
        _num_raw_data_blocks: u8,
        payload: &[u8],
    ) -> Result<DecodedFrame> {
        if aot != 2 && aot != 5 && aot != 29 {
            return Err(Error::UnsupportedAot(aot));
        }
        let core_aot = 2;
        let mut br = BitReader::new(payload);
        let mut planar = Vec::new();
        let mut last_syn = IdSynEle::Sce;
        let mut pending_sbr: Option<Box<SbrExtensionData>> = None;
        loop {
            if br.bits_remaining() < 3 {
                break;
            }
            let id = br.read(3)? as u8;
            match id {
                ID_END => break,
                ID_SCE | ID_LFE => {
                    last_syn = if id == ID_SCE {
                        IdSynEle::Sce
                    } else {
                        IdSynEle::Lfe
                    };
                    let tag = br.read(4)? as u8;
                    let body = parse_ics(&mut br, fs_index, core_aot, None)?;
                    let pcm = self.finish_sce(id, tag, body, fs_index)?;
                    planar.push(pcm);
                }
                ID_CPE => {
                    last_syn = IdSynEle::Cpe;
                    let tag = br.read(4)? as u8;
                    let (left, right) = self.decode_cpe(&mut br, fs_index, core_aot, tag)?;
                    if self.mix_down_mono {
                        let n = left.len().min(right.len());
                        let mut mix = Vec::with_capacity(n);
                        for i in 0..n {
                            mix.push(0.5 * (left[i] + right[i]));
                        }
                        planar.push(mix);
                    } else {
                        planar.push(left);
                        planar.push(right);
                    }
                }
                ID_CCE => skip_cce(&mut br, fs_index, core_aot)?,
                ID_DSE => skip_dse(&mut br)?,
                ID_PCE => skip_pce(&mut br)?,
                ID_FIL => {
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
                        Ok(_) | Err(_) => {}
                    }
                    let used = br.bit_position().saturating_sub(start);
                    let need = u64::from(cnt).saturating_mul(8);
                    if used < need {
                        br.skip((need - used) as u32)?;
                    }
                }
                other => return Err(Error::UnsupportedElement(other)),
            }
        }
        let (planar, sample_rate) = self.apply_sbr(planar, sample_rate, pending_sbr)?;
        let channels = planar.len();
        Ok(DecodedFrame {
            pcm: Vec::new(),
            planar,
            channels,
            sample_rate,
        })
    }

    /// Product mono sink: append mix-down f32 (filterbank / 32768) into `dst`.
    #[allow(clippy::too_many_arguments)]
    pub fn decode_raw_mono_f32(
        &mut self,
        aot: u8,
        fs_index: u8,
        sample_rate: u32,
        channel_configuration: u8,
        num_raw_data_blocks: u8,
        payload: &[u8],
        dst: &mut Vec<f32>,
    ) -> Result<u32> {
        let was = self.mix_down_mono;
        self.mix_down_mono = true;
        let frame = self.decode_raw_data_block(
            aot,
            fs_index,
            sample_rate,
            channel_configuration,
            num_raw_data_blocks,
            payload,
        );
        self.mix_down_mono = was;
        let frame = frame?;
        const INV_S16: f32 = 1.0 / 32768.0;
        if let Some(ch) = frame.planar.first() {
            dst.extend(ch.iter().map(|&v| v as f32 * INV_S16));
        }
        Ok(frame.sample_rate)
    }

    fn apply_sbr(
        &mut self,
        planar: Vec<Vec<f64>>,
        sample_rate: u32,
        pending: Option<Box<SbrExtensionData>>,
    ) -> Result<(Vec<Vec<f64>>, u32)> {
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
        let core: Vec<&[f64]> = planar.iter().map(Vec::as_slice).collect();
        let out = match pending.as_deref() {
            Some(ext) => dec.process_frame(ext, &core)?,
            None => dec.upsample_frame(&core)?,
        };
        Ok((out, fs_sbr))
    }

    fn finish_sce(
        &mut self,
        id: u8,
        tag: u8,
        mut body: ChannelBody,
        fs_index: u8,
    ) -> Result<Vec<f64>> {
        pns::apply(
            &mut body.spec,
            &body.ics,
            &body.sections,
            &body.sf,
            fs_index,
            &mut self.rng,
            None,
        )?;
        if let Some(t) = body.tns.as_ref() {
            tns::apply(&mut body.spec, t, &body.ics, fs_index)?;
        }
        let fb = self.banks.entry((id, tag)).or_default();
        fb.synthesize(&body.spec, &body.ics)
    }

    fn decode_cpe(
        &mut self,
        br: &mut BitReader<'_>,
        fs_index: u8,
        aot: u8,
        tag: u8,
    ) -> Result<(Vec<f64>, Vec<f64>)> {
        let common_window = br.read_bit()?;
        let (ics_common, ms) = if common_window {
            let ics = super::ics::IcsInfo::parse(br, fs_index, true)?;
            let ms = MsInfo::parse(br, &ics)?;
            (Some(ics), Some(ms))
        } else {
            (None, None)
        };
        let mut left = parse_ics(br, fs_index, aot, ics_common.as_ref())?;
        let mut right = parse_ics(br, fs_index, aot, ics_common.as_ref())?;
        if let Some(ms) = ms.as_ref() {
            stereo::apply_ms(
                &mut left.spec,
                &mut right.spec,
                &left.ics,
                &left.sections,
                &right.sections,
                ms,
                fs_index,
            )?;
        }
        let ms_used = ms.as_ref().map(|m| m.used.clone());
        let mut shared = None;
        {
            let mut pair = pns::PairPns {
                ms_used: ms_used.as_deref(),
                other_cb: Some(&right.sections.sfb_cb),
                shared: &mut shared,
            };
            pns::apply(
                &mut left.spec,
                &left.ics,
                &left.sections,
                &left.sf,
                fs_index,
                &mut self.rng,
                Some(&mut pair),
            )?;
        }
        {
            let mut pair = pns::PairPns {
                ms_used: ms_used.as_deref(),
                other_cb: Some(&left.sections.sfb_cb),
                shared: &mut shared,
            };
            pns::apply(
                &mut right.spec,
                &right.ics,
                &right.sections,
                &right.sf,
                fs_index,
                &mut self.rng,
                Some(&mut pair),
            )?;
        }
        if let Some(ms) = ms.as_ref() {
            stereo::apply_intensity(
                &left.spec,
                &mut right.spec,
                &right.ics,
                &right.sections,
                &right.sf,
                ms,
                fs_index,
            )?;
        }
        if let Some(t) = left.tns.as_ref() {
            tns::apply(&mut left.spec, t, &left.ics, fs_index)?;
        }
        if let Some(t) = right.tns.as_ref() {
            tns::apply(&mut right.spec, t, &right.ics, fs_index)?;
        }
        let left_pcm = {
            let fb = self.banks.entry((ID_CPE, tag)).or_default();
            fb.synthesize(&left.spec, &left.ics)?
        };
        let right_pcm = {
            let fb = self.banks.entry((ID_CPE, tag | 0x10)).or_default();
            fb.synthesize(&right.spec, &right.ics)?
        };
        Ok((left_pcm, right_pcm))
    }
}
