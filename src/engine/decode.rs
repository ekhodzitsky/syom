//! Stream-level LC driver: `raw_data_block()` → planar PCM.

use super::adts::AdtsHeader;
use super::bits::BitReader;
use super::error::{Error, Result};
use super::extension_payload::ExtensionPayload;
use super::filterbank::Filterbank;
use super::ics::IcsInfo;
use super::ics_body::parse_ics_into;
use super::pns::{self, Lcg};
use super::raw_data_block::IdSynEle;
use super::sbr_decoder::SbrDecoder;
use super::sbr_extension::SbrExtensionData;
use super::sbr_header::SbrHeader;
use super::section::SectionData;
use super::sf::ScaleFactors;
use super::skip::*;
use super::stereo::{self, MsInfo};
use super::tns;

#[derive(Debug, Clone, PartialEq)]
pub struct DecodedFrame {
    pub planar: Vec<Vec<f32>>,
    pub channels: usize,
    pub sample_rate: u32,
}

#[derive(Debug, Default)]
pub struct StreamDecoder {
    fb_l: Filterbank,
    fb_r: Filterbank,
    rng: Lcg,
    pub mix_down_mono: bool,
    sbr: Option<SbrDecoder>,
    sbr_hdr: Option<SbrHeader>,
    sbr_active: bool,
    pcm_l: Vec<f32>,
    pcm_r: Vec<f32>,
    frame_ch: Vec<Vec<f32>>,
    n_ch: usize,
    quant: Vec<i32>,
    spec_l: Vec<f32>,
    spec_r: Vec<f32>,
    sections_l: SectionData,
    sections_r: SectionData,
    sf_l: ScaleFactors,
    sf_r: ScaleFactors,
    fast_mono: bool,
}

impl StreamDecoder {
    pub fn new() -> Self {
        Self::default()
    }

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

    pub fn decode_raw_data_block(
        &mut self,
        aot: u8,
        fs_index: u8,
        sample_rate: u32,
        _channel_configuration: u8,
        _num_raw_data_blocks: u8,
        payload: &[u8],
    ) -> Result<DecodedFrame> {
        let sample_rate = self.decode_into_bufs(aot, fs_index, sample_rate, payload)?;
        Ok(DecodedFrame {
            planar: self.frame_ch.clone(),
            channels: self.frame_ch.len(),
            sample_rate,
        })
    }

    fn decode_into_bufs(
        &mut self,
        aot: u8,
        fs_index: u8,
        sample_rate: u32,
        payload: &[u8],
    ) -> Result<u32> {
        if aot != 2 && aot != 5 && aot != 29 {
            return Err(Error::UnsupportedAot(aot));
        }
        let core_aot = 2;
        let mut br = BitReader::new(payload);
        self.n_ch = 0;
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
                    self.finish_sce(id, tag, ics, tns.as_ref(), fs_index)?;
                    self.push_pcm(false);
                }
                ID_CPE => {
                    last_syn = IdSynEle::Cpe;
                    let tag = br.read(4)? as u8;
                    self.decode_cpe(&mut br, fs_index, core_aot, tag)?;
                    if self.mix_down_mono {
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
                other => return Err(Error::UnsupportedElement(other)),
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
        _channel_configuration: u8,
        _num_raw_data_blocks: u8,
        payload: &[u8],
        dst: &mut Vec<f32>,
    ) -> Result<u32> {
        let was = self.mix_down_mono;
        self.mix_down_mono = true;
        self.fast_mono = true;
        let rate = self.decode_into_bufs(aot, fs_index, sample_rate, payload);
        self.fast_mono = false;
        self.mix_down_mono = was;
        let rate = rate?;
        const INV_S16: f32 = 1.0 / 32768.0;
        let src = if self.sbr_active {
            self.frame_ch.first().map(Vec::as_slice).unwrap_or(&[])
        } else {
            self.pcm_l.as_slice()
        };
        dst.extend(src.iter().map(|&v| v * INV_S16));
        Ok(rate)
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
        let planar = out
            .into_iter()
            .map(|ch| ch.into_iter().map(|x| x as f32).collect())
            .collect();
        Ok((planar, fs_sbr))
    }

    fn finish_sce(
        &mut self,
        id: u8,
        tag: u8,
        ics: IcsInfo,
        tns: Option<&tns::TnsData>,
        fs_index: u8,
    ) -> Result<()> {
        pns::apply(
            &mut self.spec_l,
            &ics,
            &self.sections_l,
            &self.sf_l,
            fs_index,
            &mut self.rng,
            None,
        )?;
        if let Some(t) = tns {
            tns::apply(&mut self.spec_l, t, &ics, fs_index)?;
        }
        if id == ID_CPE && tag & 0x10 != 0 {
            self.fb_r
                .synthesize_into(&self.spec_l, &ics, &mut self.pcm_r)
        } else {
            self.fb_l
                .synthesize_into(&self.spec_l, &ics, &mut self.pcm_l)
        }
    }

    fn decode_cpe(
        &mut self,
        br: &mut BitReader<'_>,
        fs_index: u8,
        aot: u8,
        _tag: u8,
    ) -> Result<()> {
        let common_window = br.read_bit()?;
        let (ics_common, ms) = if common_window {
            let ics = super::ics::IcsInfo::parse(br, fs_index, true)?;
            let ms = MsInfo::parse(br, &ics)?;
            (Some(ics), Some(ms))
        } else {
            (None, None)
        };
        let (ics_l, tns_l) = parse_ics_into(
            br,
            fs_index,
            aot,
            ics_common.as_ref(),
            &mut self.quant,
            &mut self.spec_l,
            &mut self.sections_l,
            &mut self.sf_l,
        )?;
        let (ics_r, tns_r) = parse_ics_into(
            br,
            fs_index,
            aot,
            ics_common.as_ref(),
            &mut self.quant,
            &mut self.spec_r,
            &mut self.sections_r,
            &mut self.sf_r,
        )?;
        if let Some(ms) = ms.as_ref() {
            stereo::apply_ms(
                &mut self.spec_l,
                &mut self.spec_r,
                &ics_l,
                &self.sections_l,
                &self.sections_r,
                ms,
                fs_index,
            )?;
        }
        let ms_used = ms.as_ref().map(|m| m.used.clone());
        let mut shared = None;
        {
            let mut pair = pns::PairPns {
                ms_used: ms_used.as_deref(),
                other_cb: Some(&self.sections_r.sfb_cb),
                shared: &mut shared,
            };
            pns::apply(
                &mut self.spec_l,
                &ics_l,
                &self.sections_l,
                &self.sf_l,
                fs_index,
                &mut self.rng,
                Some(&mut pair),
            )?;
        }
        {
            let mut pair = pns::PairPns {
                ms_used: ms_used.as_deref(),
                other_cb: Some(&self.sections_l.sfb_cb),
                shared: &mut shared,
            };
            pns::apply(
                &mut self.spec_r,
                &ics_r,
                &self.sections_r,
                &self.sf_r,
                fs_index,
                &mut self.rng,
                Some(&mut pair),
            )?;
        }
        if let Some(ms) = ms.as_ref() {
            stereo::apply_intensity(
                &self.spec_l,
                &mut self.spec_r,
                &ics_r,
                &self.sections_r,
                &self.sf_r,
                ms,
                fs_index,
            )?;
        }
        if let Some(t) = tns_l.as_ref() {
            tns::apply(&mut self.spec_l, t, &ics_l, fs_index)?;
        }
        if let Some(t) = tns_r.as_ref() {
            tns::apply(&mut self.spec_r, t, &ics_r, fs_index)?;
        }
        self.fb_l
            .synthesize_into(&self.spec_l, &ics_l, &mut self.pcm_l)?;
        self.fb_r
            .synthesize_into(&self.spec_r, &ics_r, &mut self.pcm_r)?;
        Ok(())
    }
}
