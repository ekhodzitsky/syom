//! SCE finish and CPE pair for [`super::decode::StreamDecoder`].

use super::bits::BitReader;
use super::cce::{PendingChan, apply_cce_to_pending, apply_independent_pcm, parse_cce};
use super::channel_map::{ElemKind, Element};
use super::decode::StreamDecoder;
use super::error::{Error, Result};
use super::ics::IcsInfo;
use super::ics_body::parse_ics_into;
use super::pns;
use super::stereo::{self, MsInfo};
use super::tns;

impl StreamDecoder {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn stash_chan(
        &mut self,
        kind: ElemKind,
        tag: u8,
        part: u8,
        ics: IcsInfo,
        tns: Option<tns::TnsData>,
        take_l: bool,
        push_elem: bool,
    ) {
        let spec = self.take_spec(!take_l);
        if push_elem && part == 0 {
            self.elems.push(Element {
                kind,
                tag,
                plane: self.n_ch,
            });
        }
        self.pending.push(PendingChan {
            kind,
            tag,
            part,
            ics,
            tns,
            spec,
        });
        self.n_ch += 1;
    }

    fn take_spec(&mut self, right: bool) -> Vec<f32> {
        let buf = if right {
            &mut self.spec_r
        } else {
            &mut self.spec_l
        };
        let filled = std::mem::take(buf);
        *buf = self.spec_pool.pop().unwrap_or_default();
        filled
    }

    fn recycle_spec(&mut self, mut spec: Vec<f32>) {
        spec.clear();
        self.spec_pool.push(spec);
    }

    pub(crate) fn finish_sce_pns(&mut self, ics: &IcsInfo, fs_index: u8) -> Result<()> {
        pns::apply(
            &mut self.spec_l,
            ics,
            &self.sections_l,
            &self.sf_l,
            fs_index,
            &mut self.rng,
            None,
        )
    }

    pub(crate) fn decode_cpe(
        &mut self,
        br: &mut BitReader<'_>,
        fs_index: u8,
        aot: u8,
        _tag: u8,
    ) -> Result<(IcsInfo, Option<tns::TnsData>, IcsInfo, Option<tns::TnsData>)> {
        let common_window = br.read_bit()?;
        let (ics_common, ms) = if common_window {
            let ics =
                super::ics::IcsInfo::parse_mode(br, fs_index, true, aot == super::asc::AOT_LD)?;
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
        let mut have_shared = false;
        {
            let mut pair = pns::PairPns {
                ms: ms.as_ref(),
                other_cb: Some(&self.sections_r.sfb_cb),
                shared: &mut self.pns_shared,
                have_shared: &mut have_shared,
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
                ms: ms.as_ref(),
                other_cb: Some(&self.sections_l.sfb_cb),
                shared: &mut self.pns_shared,
                have_shared: &mut have_shared,
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
        Ok((ics_l, tns_l, ics_r, tns_r))
    }

    pub(crate) fn finish_pending(
        &mut self,
        fs_index: u8,
        multichannel: bool,
        skip_downmix: bool,
    ) -> Result<()> {
        for cce in &mut self.cces {
            pns::apply(
                &mut cce.spec,
                &cce.ics,
                &cce.sections,
                &cce.sf,
                fs_index,
                &mut self.rng,
                None,
            )?;
        }
        for i in 0..self.cces.len() {
            if !self.cces[i].independent && self.cces[i].domain == 0 {
                apply_cce_to_pending(&mut self.pending, &self.cces[i], fs_index)?;
            }
        }
        for p in &mut self.pending {
            if let Some(t) = p.tns.as_ref() {
                tns::apply(&mut p.spec, t, &p.ics, fs_index)?;
            }
        }
        for cce in &mut self.cces {
            if let Some(t) = cce.tns {
                tns::apply(&mut cce.spec, &t, &cce.ics, fs_index)?;
            }
        }
        for i in 0..self.cces.len() {
            if !self.cces[i].independent && self.cces[i].domain == 1 {
                apply_cce_to_pending(&mut self.pending, &self.cces[i], fs_index)?;
            }
        }
        let mut pending = std::mem::take(&mut self.pending);
        self.n_ch = 0;
        let mut ids = [(ElemKind::Sce, 0u8, 0u8); 8];
        let mut n_ids = 0usize;
        let mut i = 0usize;
        while i < pending.len() {
            if pending[i].kind == ElemKind::Cpe && pending[i].part == 0 {
                if i + 1 >= pending.len() {
                    self.pending = pending;
                    return Err(Error::Format("CPE missing right"));
                }
                if multichannel {
                    self.fb_pool
                        .swap_pair(pending[i].tag, &mut self.fb_l, &mut self.fb_r);
                }
                self.fb_l
                    .synthesize_into(&pending[i].spec, &pending[i].ics, &mut self.pcm_l)?;
                self.fb_r.synthesize_into(
                    &pending[i + 1].spec,
                    &pending[i + 1].ics,
                    &mut self.pcm_r,
                )?;
                if self.fast_mono {
                    for (l, r) in self.pcm_l.iter_mut().zip(self.pcm_r.iter()) {
                        *l = 0.5 * (*l + *r);
                    }
                }
                if multichannel {
                    self.fb_pool
                        .swap_pair(pending[i].tag, &mut self.fb_l, &mut self.fb_r);
                }
                self.push_pcm(false);
                if n_ids < 8 {
                    ids[n_ids] = (pending[i].kind, pending[i].tag, 0);
                    n_ids += 1;
                }
                self.push_pcm(true);
                if n_ids < 8 {
                    ids[n_ids] = (pending[i + 1].kind, pending[i + 1].tag, 1);
                    n_ids += 1;
                }
                i += 2;
            } else {
                if multichannel {
                    self.fb_pool
                        .swap_in(pending[i].kind, pending[i].tag, &mut self.fb_l);
                }
                self.fb_l
                    .synthesize_into(&pending[i].spec, &pending[i].ics, &mut self.pcm_l)?;
                if multichannel {
                    self.fb_pool
                        .swap_in(pending[i].kind, pending[i].tag, &mut self.fb_l);
                }
                self.push_pcm(false);
                if n_ids < 8 {
                    ids[n_ids] = (pending[i].kind, pending[i].tag, pending[i].part);
                    n_ids += 1;
                }
                i += 1;
            }
        }
        if !self.fast_mono {
            let cces = std::mem::take(&mut self.cces);
            for cce in &cces {
                if !cce.independent {
                    continue;
                }
                self.fb_pool.swap_in(ElemKind::Cce, cce.tag, &mut self.fb_l);
                self.cce_pcm.clear();
                self.fb_l
                    .synthesize_into(&cce.spec, &cce.ics, &mut self.cce_pcm)?;
                self.fb_pool.swap_in(ElemKind::Cce, cce.tag, &mut self.fb_l);
                apply_independent_pcm(&mut self.frame_ch, &ids[..n_ids], &self.cce_pcm, cce)?;
            }
            self.cces = cces;
            if !skip_downmix && !multichannel && self.mix_down_mono && self.frame_ch.len() >= 2 {
                let n = self.frame_ch[0].len().min(self.frame_ch[1].len());
                for i in 0..n {
                    self.frame_ch[0][i] = 0.5 * (self.frame_ch[0][i] + self.frame_ch[1][i]);
                }
                self.frame_ch.truncate(1);
                self.pcm_l.clone_from(&self.frame_ch[0]);
                self.n_ch = 1;
            }
        }
        for p in &mut pending {
            let spec = std::mem::take(&mut p.spec);
            self.recycle_spec(spec);
        }
        pending.clear();
        self.pending = pending;
        self.refill_spec_bufs();
        Ok(())
    }

    pub(crate) fn refill_spec_bufs(&mut self) {
        if self.spec_l.capacity() == 0 {
            self.spec_l = self.spec_pool.pop().unwrap_or_default();
        }
        if self.spec_r.capacity() == 0 {
            self.spec_r = self.spec_pool.pop().unwrap_or_default();
        }
    }

    /// LC / HE element loop: 3-bit element IDs up to `ID_END` (the ER AAC LD
    /// id-less walk lives in `decode_ld`).
    pub(crate) fn decode_lc_elements(
        &mut self,
        br: &mut BitReader<'_>,
        fs_index: u8,
        core_aot: u8,
        sample_rate: u32,
        channel_configuration: u8,
        multichannel: bool,
    ) -> Result<()> {
        let mut last_elem = None;
        loop {
            if br.bits_remaining() < 3 {
                break;
            }
            let id = super::raw_data_block::IdSynEle::from_bits(br.read(3)? as u8);
            match id {
                super::raw_data_block::IdSynEle::End => break,
                super::raw_data_block::IdSynEle::Sce | super::raw_data_block::IdSynEle::Lfe => {
                    let tag = br.read(4)? as u8;
                    let kind = if id == super::raw_data_block::IdSynEle::Sce {
                        ElemKind::Sce
                    } else {
                        ElemKind::Lfe
                    };
                    last_elem = Some((kind, tag));
                    if multichannel {
                        super::fb_pool::reject_dup(&self.elems, kind, tag)?;
                    }
                    let (ics, tns) = parse_ics_into(
                        br,
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
                super::raw_data_block::IdSynEle::Cpe => {
                    let tag = br.read(4)? as u8;
                    last_elem = Some((ElemKind::Cpe, tag));
                    if multichannel {
                        super::fb_pool::reject_dup(&self.elems, ElemKind::Cpe, tag)?;
                    }
                    let (ics_l, tns_l, ics_r, tns_r) =
                        self.decode_cpe(br, fs_index, core_aot, tag)?;
                    self.stash_chan(ElemKind::Cpe, tag, 0, ics_l, tns_l, true, true);
                    self.stash_chan(ElemKind::Cpe, tag, 1, ics_r, tns_r, false, true);
                }
                super::raw_data_block::IdSynEle::Cce => {
                    self.cces.push(parse_cce(br, fs_index, core_aot)?);
                }
                super::raw_data_block::IdSynEle::Dse => super::skip::skip_dse(br)?,
                super::raw_data_block::IdSynEle::Pce => {
                    self.pce = Some(super::channel_map::parse_pce(br)?);
                }
                super::raw_data_block::IdSynEle::Fil => {
                    let cnt = super::skip::fill_count(br)?;
                    let fs_sbr = self.sbr_out_rate.unwrap_or(sample_rate.saturating_mul(2));
                    super::sbr_attach::ingest_fil(
                        br,
                        cnt,
                        last_elem,
                        fs_sbr,
                        &mut self.sbr_pool,
                        &mut self.pending_sbrs,
                    )?;
                }
            }
        }
        let _ = channel_configuration;
        Ok(())
    }
}
