//! Presentation-sample seeking on M4A (edit/tag window + codec preroll).

use std::io::{self, Read, Seek, SeekFrom};

use crate::engine::asc::AudioSpecificConfig;
use crate::engine::decode::StreamDecoder;
use crate::error::{AacError, Result};
use crate::isomp4::AacTrack;
use crate::options::{ChannelMode, DecodeOptions};
use crate::stream::{Frame, StreamInfo};

#[path = "m4a_pos_au.rs"]
mod au;
#[path = "m4a_pos_checkpoint.rs"]
mod checkpoint;
#[path = "m4a_pos_completion.rs"]
pub(super) mod completion;
#[path = "m4a_pos_exact.rs"]
mod exact;
#[path = "m4a_pos_open.rs"]
mod open;
pub use completion::PresentationCompletion;
pub use exact::{ExactSeekHistory, ExactSeekPlan, ExactSeekPosition};

fn read_exact<R: Read>(r: &mut R, buf: &mut [u8]) -> Result<()> {
    loop {
        match r.read_exact(buf) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                return Err(AacError::truncated_at(None));
            }
            Err(e) => return Err(AacError::from(e)),
        }
    }
}

/// LC mono/stereo overlap AUs after a prefix is verified noise-free.
/// Cold seeks, PNS, HE/PS and surround can require replay from AU 0
/// (see [`M4aSeek::seek`]).
#[must_use]
pub fn preroll_aus(sbr: bool) -> u64 {
    if sbr { 0 } else { 2 }
}

/// Seekable M4A source: presentation-sample `seek` with codec preroll.
///
/// ```
/// use std::io::Cursor;
/// use syom::{DecodeOptions, M4aSeek};
/// let m4a = include_bytes!("../goldens/sine441.m4a");
/// let mut src = M4aSeek::open(Cursor::new(m4a), DecodeOptions::speech())?;
/// let at = src.seek(1024)?;
/// assert_eq!(at, 1024);
/// let mut samples = 0usize;
/// let info = src.decode(|f| {
///     samples += f.samples;
///     Ok(())
/// })?;
/// assert_eq!(samples as u64, info.samples);
/// # Ok::<(), syom::AacError>(())
/// ```
pub struct M4aSeek<R> {
    reader: R,
    track: AacTrack,
    asc: AudioSpecificConfig,
    opts: DecodeOptions,
    dec: Box<StreamDecoder>,
    scratch: Vec<f32>,
    next_frame: usize,
    discard_left: usize,
    play_left: u64,
    pos: u64,
    skip: u64,
    play: u64,
    samples_per_au: u64,
    preroll: u64,
    locked: Option<(u32, usize)>,
    mono: bool,
    // Contiguous decoded prefix that leaves the initial PNS state unchanged.
    noise_free_prefix: usize,
    exact_mode: bool,
    exact_failed: bool,
    source_identity: std::sync::Arc<()>,
    checkpoint_gapless_safe: bool,
    gapless: Option<crate::gapless::Delay>,
    checkpoint_clean: bool,
    checkpoint_restored: bool,
    checkpoint_source_tag: u64,
    #[cfg(test)]
    last_seek_aus: u64,
}

impl<R: Read + Seek> M4aSeek<R> {
    fn reset_codec(&mut self) {
        self.checkpoint_restored = false;
        self.exact_mode = false;
        self.exact_failed = false;
        self.dec.reset();
        self.dec.mix_down_mono = self.mono;
        if let Some(pce) = self.asc.pce.clone() {
            self.dec.set_config_pce(pce);
        }
        self.dec.set_he_config(
            self.asc.sbr_present,
            self.asc.ps_present,
            self.asc.output_sample_rate,
        );
        self.dec.set_frame_samples(self.asc.frame_samples);
        self.locked = None;
        self.scratch.clear();
    }

    /// Presentation length in output samples (`elst` when present).
    #[must_use]
    pub fn presentation_len(&self) -> u64 {
        self.play
    }

    /// Current presentation position.
    #[must_use]
    pub fn position(&self) -> u64 {
        self.pos
    }

    /// Profile overlap in AUs (see [`preroll_aus`]). Actual history replay
    /// can be longer for cold seeks, PNS, legacy profiles, HE/PS or surround.
    #[must_use]
    pub fn preroll_aus(&self) -> u64 {
        self.preroll
    }

    #[cfg(test)]
    pub(crate) fn last_seek_aus(&self) -> u64 {
        self.last_seek_aus
    }

    /// Seek to a presentation sample. `0` is the first sample after `elst`
    /// skip. Negative is before-start; `> presentation_len` is beyond-end.
    /// Cold or stateful seeks may decode the prefix from AU 0 to preserve
    /// PNS and prediction history. Previously verified noise-free LC/LD
    /// prefixes use only the profile's overlap preroll.
    pub fn seek(&mut self, presentation_sample: i64) -> Result<u64> {
        if presentation_sample < 0 {
            return Err(AacError::invalid_limits(
                "m4a seek before presentation start",
            ));
        }
        let pres = presentation_sample as u64;
        if pres > self.play {
            let rate = self.asc.output_sample_rate.max(1);
            return Err(AacError::too_long(
                pres as f64 / f64::from(rate),
                self.play as f64 / f64::from(rate),
            ));
        }
        self.reset_codec();
        self.pos = pres;
        self.play_left = self.play - pres;
        #[cfg(test)]
        {
            self.last_seek_aus = 0;
        }
        if pres == self.play {
            self.next_frame = self.track.frames.len();
            self.discard_left = 0;
            return Ok(pres);
        }
        let coded = self.skip.saturating_add(pres);
        let spf = self.samples_per_au.max(1);
        let target = coded / spf;
        let intra = coded % spf;
        // Skipping unexamined AUs loses PNS RNG history. Legacy profiles also
        // carry prediction/filterbank state. Only a verified noise-free LC/LD
        // prefix permits the short overlap preroll; otherwise replay from AU 0.
        let overlap_start = target.saturating_sub(self.preroll);
        let start = if self.asc.sbr_present
            || self.asc.channel_configuration > 2
            || !matches!(
                self.asc.aot,
                crate::engine::asc::AOT_LC | crate::engine::asc::AOT_LD
            )
            || overlap_start > self.noise_free_prefix as u64
        {
            0
        } else {
            overlap_start
        };
        for i in start..target {
            let idx = usize::try_from(i).unwrap_or(usize::MAX);
            if idx >= self.track.frames.len() {
                break;
            }
            self.decode_au(idx)?;
            #[cfg(test)]
            {
                self.last_seek_aus += 1;
            }
        }
        self.next_frame = usize::try_from(target).unwrap_or(usize::MAX);
        self.discard_left = usize::try_from(intra).unwrap_or(usize::MAX);
        Ok(pres)
    }

    /// Decode from the current position to the presentation end.
    pub fn decode<F>(&mut self, mut on_frame: F) -> Result<StreamInfo>
    where
        F: FnMut(Frame<'_>) -> Result<()>,
    {
        if self.exact_failed {
            return Err(AacError::Lifecycle {
                state: crate::LifecycleState::Failed,
            });
        }
        let mut aac_frames = 0u64;
        let mut emitted = 0u64;
        let mut samples_out = 0u64;
        while self.next_frame < self.track.frames.len() && self.play_left > 0 {
            let (rate, n) = self.decode_au(self.next_frame)?;
            self.next_frame += 1;
            aac_frames += 1;
            if n == 0 {
                continue;
            }
            let n_ch = if self.mono {
                1
            } else {
                self.dec.frame_planes().len()
            };
            match self.locked {
                None => {
                    if rate == 0 || rate > self.opts.max_sample_rate {
                        return Err(AacError::sample_rate(rate, self.opts.max_sample_rate));
                    }
                    self.locked = Some((rate, n_ch));
                }
                Some((r, c)) => {
                    if rate != r || c != n_ch {
                        return Err(AacError::decode(
                            "aac: sample rate or channel count changed mid-stream",
                        ));
                    }
                }
            }
            if self.discard_left >= n {
                self.discard_left -= n;
                continue;
            }
            let start = self.discard_left;
            self.discard_left = 0;
            let mut take = n - start;
            take = take.min(self.play_left as usize);
            self.play_left -= take as u64;
            if take == 0 {
                continue;
            }
            emitted += 1;
            samples_out += take as u64;
            self.pos += take as u64;
            let end = start + take;
            if self.mono {
                let plane: [&[f32]; 1] = [&self.scratch[start..end]];
                on_frame(Frame {
                    sample_rate: rate,
                    samples: take,
                    planar: &plane,
                    meta: self.dec.last_meta(),
                })?;
            } else {
                let mut slots: [&[f32]; crate::layout::MAX_PLANES] =
                    [&[]; crate::layout::MAX_PLANES];
                let src = self.dec.frame_planes();
                let n = src.len().min(crate::layout::MAX_PLANES);
                for (i, ch) in src.iter().enumerate().take(n) {
                    slots[i] = &ch[start..end];
                }
                on_frame(Frame {
                    sample_rate: rate,
                    samples: take,
                    planar: &slots[..n],
                    meta: self.dec.last_meta(),
                })?;
            }
        }
        self.validate_presentation_end()?;
        if emitted == 0 && samples_out == 0 && self.play > 0 && self.pos == 0 {
            return Err(AacError::NotAac);
        }
        let (sample_rate, channels) = self
            .locked
            .unwrap_or((self.asc.output_sample_rate, if self.mono { 1 } else { 0 }));
        Ok(StreamInfo {
            sample_rate,
            core_rate: self.dec.last_core_rate(),
            channels,
            layout: self.dec.last_meta().layout,
            aac_frames,
            samples: samples_out,
            priming: self
                .gapless
                .map(|d| d.priming)
                .or_else(|| self.track.has_elst.then_some(self.skip)),
            remainder: self
                .gapless
                .map(|d| d.remainder)
                .or_else(|| self.track.unplayed_tail(self.skip, self.pos, sample_rate)),
            presentation_completion: self.presentation_completion(),
        })
    }
}
