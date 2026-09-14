//! Frame emit for [`super::Encoder`] (line cap / TASK-78 stack planes).

use super::{EncodedFrame, Encoder, FRAME};
use crate::error::Result;

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
        if self.enc.lookahead_enabled() {
            if let Some(au) = self.enc.push_frame(planes)? {
                self.deliver(&au, samples, scratch, cb)?;
            }
            return Ok(());
        }
        self.enc.encode_into(planes)?;
        let fs = self.enc.fs_index();
        scratch.clear();
        crate::encode::adts_frame_into(self.enc.payload(), fs, self.channels, scratch);
        self.aac_frames += 1;
        self.bytes += scratch.len() as u64;
        cb(EncodedFrame {
            samples,
            au: scratch,
        })
    }

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
        crate::encode::adts_frame_into(au, self.enc.fs_index(), self.channels, scratch);
        self.aac_frames += 1;
        self.bytes += scratch.len() as u64;
        cb(EncodedFrame {
            samples,
            au: scratch,
        })
    }
}
