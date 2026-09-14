//! SCE finish and CPE pair for [`super::decode::StreamDecoder`].

use super::bits::BitReader;
use super::cce::{PendingChan, apply_cce_to_pending};
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
        let spec = if take_l {
            std::mem::take(&mut self.spec_l)
        } else {
            std::mem::take(&mut self.spec_r)
        };
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
        Ok((ics_l, tns_l, ics_r, tns_r))
    }

    pub(crate) fn finish_pending(&mut self, fs_index: u8, multichannel: bool) -> Result<()> {
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
            if self.cces[i].domain == 0 {
                apply_cce_to_pending(&mut self.pending, &self.cces[i], fs_index)?;
            }
        }
        for p in &mut self.pending {
            if let Some(t) = p.tns.as_ref() {
                tns::apply(&mut p.spec, t, &p.ics, fs_index)?;
            }
        }
        for cce in &mut self.cces {
            if let Some(t) = cce.tns.clone() {
                tns::apply(&mut cce.spec, &t, &cce.ics, fs_index)?;
            }
        }
        for i in 0..self.cces.len() {
            if self.cces[i].domain == 1 {
                apply_cce_to_pending(&mut self.pending, &self.cces[i], fs_index)?;
            }
        }
        let pending = std::mem::take(&mut self.pending);
        self.n_ch = 0;
        let mut iter = pending.into_iter();
        while let Some(p) = iter.next() {
            if p.kind == ElemKind::Cpe && p.part == 0 {
                let r = iter.next().ok_or(Error::Format("CPE missing right"))?;
                if multichannel {
                    self.fb_pool
                        .swap_pair(p.tag, &mut self.fb_l, &mut self.fb_r);
                }
                self.fb_l
                    .synthesize_into(&p.spec, &p.ics, &mut self.pcm_l)?;
                self.fb_r
                    .synthesize_into(&r.spec, &r.ics, &mut self.pcm_r)?;
                if multichannel {
                    self.fb_pool
                        .swap_pair(p.tag, &mut self.fb_l, &mut self.fb_r);
                }
                if !multichannel && self.mix_down_mono {
                    let n = self.pcm_l.len().min(self.pcm_r.len());
                    for (l, rv) in self.pcm_l.iter_mut().zip(self.pcm_r.iter()).take(n) {
                        *l = 0.5 * (*l + *rv);
                    }
                    self.pcm_l.truncate(n);
                    self.push_pcm(false);
                } else {
                    self.push_pcm(false);
                    self.push_pcm(true);
                }
            } else {
                if multichannel {
                    self.fb_pool.swap_in(p.kind, p.tag, &mut self.fb_l);
                }
                self.fb_l
                    .synthesize_into(&p.spec, &p.ics, &mut self.pcm_l)?;
                if multichannel {
                    self.fb_pool.swap_in(p.kind, p.tag, &mut self.fb_l);
                }
                self.push_pcm(false);
            }
        }
        Ok(())
    }
}
