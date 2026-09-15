//! Frame emit for [`super::Encoder`] (line cap / TASK-78 stack planes):
//! ADTS, LATM/LOAS or raw framing of one access unit, then the callback.

use super::{EncodedFrame, Encoder, FRAME};
use crate::error::Result;
use crate::options::EncodeContainer;

impl Encoder {
    pub(super) fn emit<F>(
        &mut self,
        bufs: &[[f32; FRAME]; 2],
        samples: usize,
        scratch: &mut Vec<u8>,
        cb: &mut F,
    ) -> Result<()>
    where
        F: FnMut(EncodedFrame<'_>) -> Result<()>,
    {
        let mut slots: [&[f32]; 2] = [&[]; 2];
        for (i, b) in bufs.iter().enumerate().take(self.channels) {
            slots[i] = &b[..];
        }
        let planes = &slots[..self.channels];
        if self.lc()?.lookahead_enabled() {
            if let Some(au) = self.lc()?.push_frame(planes)? {
                self.deliver(&au, samples, scratch, cb)?;
            }
            return Ok(());
        }
        self.lc()?.encode_into(planes)?;
        let mut au = std::mem::take(&mut self.au_scratch);
        au.clear();
        au.extend_from_slice(self.lc()?.payload());
        let r = self.deliver(&au, samples, scratch, cb);
        self.au_scratch = au;
        r
    }

    /// Frame `au` under the encoder's framing into `scratch`, count it,
    /// and hand it to the callback with the raw unit as `payload`.
    pub(super) fn deliver<F>(
        &mut self,
        au: &[u8],
        samples: usize,
        scratch: &mut Vec<u8>,
        cb: &mut F,
    ) -> Result<()>
    where
        F: FnMut(EncodedFrame<'_>) -> Result<()>,
    {
        scratch.clear();
        let hdr = crate::engine::adts::ADTS_HEADER_BYTES_NO_CRC;
        let payload_off = match self.framing {
            EncodeContainer::Adts => {
                crate::encode::adts_frame_into(au, self.enc.fs_index(), self.channels, scratch);
                Some(hdr)
            }
            EncodeContainer::Latm => {
                crate::engine::latm_write::loas_frame_into(&self.asc, self.asc_bits, au, scratch)?;
                None
            }
            _ => {
                scratch.extend_from_slice(au);
                Some(0)
            }
        };
        self.aac_frames += 1;
        self.bytes += scratch.len() as u64;
        let framed: &[u8] = scratch;
        let payload = match payload_off {
            Some(off) => &framed[off..],
            None => au,
        };
        cb(EncodedFrame {
            samples,
            au: framed,
            payload,
        })
    }
}
