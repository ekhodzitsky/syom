//! Streaming encode: a resumable push [`Encoder`] for planar f32 PCM →
//! AAC-LC in ADTS or raw access units ([`EncodeContainer::Raw`]).
//!
//! Feed PCM in chunks of any size; the callback fires once per completed
//! AAC frame (1024 samples per channel). ADTS wraps the unit; Raw emits
//! the `raw_data_block` plus a stable [`Encoder::asc`].
//! Leftover samples are buffered across feeds; [`Encoder::finish`] encodes
//! the zero-padded tail frame, exactly like one-shot [`crate::encode_with`]
//! — same frame sequence, same bytes. M4A is rejected at construction
//! (`stco` / sample sizes need the finished totals; use
//! [`crate::encode_with`] with [`EncodeContainer::M4a`]), mirroring how the
//! push [`crate::Decoder`] rejects ISOBMFF input.
//!
//! With [`EncodeOptions::lookahead`] on, the callback trails the input by
//! one frame (the attack detector sees the next frame before a frame is
//! encoded) and `finish` flushes the held frame; byte-parity with one-shot
//! holds for the same options.
//!
//! Lifecycle: **open** → `feed` / `finish`; a PCM, rate, callback, or
//! encode error moves to **failed**; a successful `finish` moves to
//! **finished**. Further `feed` / `finish` error until [`Encoder::reset`].
//!
//! ```
//! use syom::{DecodeOptions, EncodeOptions, Encoder};
//! let mut enc = Encoder::new(48_000, 1, &EncodeOptions::adts())?;
//! let pcm = vec![0.0f32; 3000];
//! let mut adts = Vec::new();
//! for chunk in pcm.chunks(777) {
//!     enc.feed(&[chunk], |f| {
//!         adts.extend_from_slice(f.au);
//!         Ok(())
//!     })?;
//! }
//! let info = enc.finish(|f| {
//!     adts.extend_from_slice(f.au);
//!     Ok(())
//! })?;
//! assert_eq!(info.samples, 3000);
//! assert_eq!(info.aac_frames, 4); // padded tail + overlap drain
//! assert_eq!(info.priming, 1024);
//! assert_eq!(info.remainder, 72); // 3*1024 - 3000
//! assert_eq!(info.bytes as usize, adts.len());
//! let dec = syom::decode_with(&adts, &DecodeOptions::unbounded())?;
//! assert_eq!(dec.channels[0].len(), 4 * 1024);
//! # Ok::<(), syom::AacError>(())
//! ```
//!
//! Raw AUs for an external muxer: [`EncodeOptions::raw`], [`Encoder::asc`],
//! [`crate::wrap_adts_au`].

use crate::engine::adts::ADTS_SAMPLE_RATES_HZ;
use crate::engine::enc_frame::LcEncoder;
use crate::engine::swb::LONG_WINDOW_LEN as FRAME;
use crate::error::{AacError, Result};
use crate::options::{EncodeContainer, EncodeOptions};

/// One encoded AAC frame.
///
/// The bytes borrow encoder scratch and are valid **only for the duration
/// of the callback** — copy them out to keep them.
#[non_exhaustive]
pub struct EncodedFrame<'a> {
    /// Input samples per channel this frame carries: 1024, or the tail
    /// remainder at [`Encoder::finish`] (that frame is zero-padded to 1024
    /// in the bitstream, like one-shot encode).
    pub samples: usize,
    /// Container bytes: ADTS frame, or the raw `raw_data_block` when the
    /// encoder was built with [`EncodeContainer::Raw`].
    pub au: &'a [u8],
    /// Elementary `raw_data_block` (no ADTS header). Same lifetime as `au`.
    pub payload: &'a [u8],
}

/// Tallies from a finished streaming encode.
///
/// Input / bitstream tallies plus the AAC timeline (Apple QA1636-style).
/// ADTS still cannot carry trim. M4A writes [`Self::priming`] as
/// `elst.media_time`, [`Self::samples`] as `elst`/`mvhd` presentation
/// duration, and [`Self::remainder`] as the unplayed `mdhd` tail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct EncodeInfo {
    /// Input sample rate.
    pub sample_rate: u32,
    /// Channels per frame (1 or 2).
    pub channels: usize,
    /// AAC frames emitted, including the overlap-drain frame of zeros.
    pub aac_frames: u64,
    /// Input samples per channel consumed (source length).
    pub samples: u64,
    /// Bytes handed to the callback (ADTS headers included).
    pub bytes: u64,
    /// Encoder delay (1024). Skip this many decoded samples for valid audio.
    pub priming: u64,
    /// Zero-pad in the last *content* block: `ceil(samples/1024)*1024 - samples`.
    pub remainder: u64,
    /// `aac_frames * 1024` (decoded ADTS length before container trim).
    pub coded_samples: u64,
    /// MPEG layout of the coded planes (`1` mono / `2` stereo).
    pub layout: crate::Layout,
}

/// Resumable push encoder: planar f32 chunks in, ADTS frames out.
///
/// Byte-exact with one-shot [`crate::encode_with`] on the same PCM, for any
/// feed chunking. Peak RAM is one frame of PCM plus the current access
/// unit. Validation matches one-shot encode: the ADTS sample-rate table,
/// 1–2 channels, finite samples in `[-1, 1]`, equal plane lengths.
/// `|x| > 1` and non-finite samples are [`crate::AacError::InvalidPcm`] (no clip).
pub struct Encoder {
    enc: LcEncoder,
    sample_rate: u32,
    channels: usize,
    wrap_adts: bool,
    /// LC `AudioSpecificConfig` (2 bytes). Not sized by input duration.
    asc: Vec<u8>,
    /// Samples buffered across feeds, one plane per channel.
    pending: [Vec<f32>; 2],
    pending_len: usize,
    /// Reused access-unit scratch the callback borrows.
    scratch: Vec<u8>,
    samples: u64,
    aac_frames: u64,
    bytes: u64,
    life: Life,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Life {
    Open,
    Finished,
    Failed,
}

impl Encoder {
    /// New encoder for `channels` planes at `sample_rate` under `opts`.
    ///
    /// Errors [`AacError::Encode`] on an unsupported rate, channel count,
    /// or zero bitrate. [`EncodeContainer::M4a`] is [`AacError::Unsupported`]
    /// (use [`crate::encode_with`]; M4A needs finish-time sizes).
    pub fn new(sample_rate: u32, channels: usize, opts: &EncodeOptions) -> Result<Self> {
        if !ADTS_SAMPLE_RATES_HZ.contains(&sample_rate) {
            return Err(AacError::encode(format!(
                "encode: unsupported sample rate {sample_rate}Hz (not in the AAC table)"
            )));
        }
        if !(1..=2).contains(&channels) {
            return Err(AacError::encode(format!(
                "encode: channels must be 1 or 2, got {channels}"
            )));
        }
        if opts.bitrate_bps == 0 {
            return Err(AacError::encode("encode: bitrate must be > 0"));
        }
        let max_bps = crate::engine::enc_frame::max_bitrate_bps(sample_rate, channels);
        if opts.bitrate_bps > max_bps {
            return Err(AacError::encode(format!(
                "encode: bitrate {} bps exceeds AAC-LC 6144 bits/channel (max {max_bps} bps)",
                opts.bitrate_bps
            )));
        }
        if opts.container == EncodeContainer::M4a {
            return Err(AacError::Unsupported(
                crate::UnsupportedFeature::EncodeM4aStreaming,
            ));
        }
        let wrap_adts = opts.container != EncodeContainer::Raw;
        let enc = crate::encode::new_lc(sample_rate, channels, opts)?;
        let asc = crate::engine::asc::write_lc(enc.fs_index(), channels as u8);
        Ok(Self {
            enc,
            sample_rate,
            channels,
            wrap_adts,
            asc,
            pending: [Vec::new(), Vec::new()],
            pending_len: 0,
            scratch: Vec::new(),
            samples: 0,
            aac_frames: 0,
            bytes: 0,
            life: Life::Open,
        })
    }

    /// LC `AudioSpecificConfig` for this encoder (stable; not input-sized).
    #[must_use]
    pub fn asc(&self) -> &[u8] {
        &self.asc
    }

    fn ensure_open(&self) -> Result<()> {
        match self.life {
            Life::Open => Ok(()),
            Life::Finished => Err(AacError::Lifecycle {
                state: crate::LifecycleState::Finished,
            }),
            Life::Failed => Err(AacError::Lifecycle {
                state: crate::LifecycleState::Failed,
            }),
        }
    }

    fn fail<T>(&mut self, e: AacError) -> Result<T> {
        self.life = Life::Failed;
        Err(e)
    }

    #[must_use]
    pub fn frames(&self) -> u64 {
        self.aac_frames
    }

    #[must_use]
    pub fn samples(&self) -> u64 {
        self.samples
    }

    #[must_use]
    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    #[must_use]
    pub fn is_failed(&self) -> bool {
        self.life == Life::Failed
    }

    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.life == Life::Finished
    }

    /// Drop overlap, rate credit, pending PCM, and counters. Prepared KBD
    /// windows, psy spreading, and vector capacity are kept. Rate, channels,
    /// and [`EncodeOptions`] are unchanged. No global cache.
    pub fn reset(&mut self) -> Result<()> {
        self.enc.reset();
        for p in &mut self.pending {
            p.clear();
        }
        self.pending_len = 0;
        self.scratch.clear();
        self.samples = 0;
        self.aac_frames = 0;
        self.bytes = 0;
        self.life = Life::Open;
        Ok(())
    }

    /// Feed PCM planes (any length, equal lengths per channel). `on_frame`
    /// fires per completed AAC frame. Returns samples consumed per channel.
    pub fn feed<F>(&mut self, planes: &[&[f32]], on_frame: F) -> Result<usize>
    where
        F: FnMut(EncodedFrame<'_>) -> Result<()>,
    {
        self.ensure_open()?;
        if planes.len() != self.channels {
            return Err(AacError::InvalidPcm(crate::PcmReject::ChannelCount));
        }
        let n = planes.first().map_or(0, |p| p.len());
        if planes.iter().any(|p| p.len() != n) {
            return Err(AacError::InvalidPcm(crate::PcmReject::PlaneLength));
        }
        if let Err(e) = crate::encode::check_pcm_samples(planes.iter().copied()) {
            return self.fail(e);
        }
        let mut cb = on_frame;
        let mut scratch = std::mem::take(&mut self.scratch);
        let mut off = 0usize;
        while self.pending_len + (n - off) >= FRAME {
            let take = FRAME - self.pending_len;
            let mut bufs = [[0.0f32; FRAME]; 2];
            for (ch, buf) in bufs.iter_mut().enumerate().take(self.channels) {
                buf[..self.pending_len].copy_from_slice(&self.pending[ch][..self.pending_len]);
                buf[self.pending_len..].copy_from_slice(&planes[ch][off..off + take]);
            }
            if let Err(e) = self.emit(&bufs, FRAME, &mut scratch, &mut cb) {
                self.pending_len = 0;
                self.samples += (off + take) as u64;
                self.scratch = scratch;
                return self.fail(e);
            }
            off += take;
            self.pending_len = 0;
        }
        self.scratch = scratch;
        if off < n {
            if self.pending_len == 0 {
                for (ch, plane) in planes.iter().enumerate() {
                    self.pending[ch].clear();
                    self.pending[ch].extend_from_slice(&plane[off..]);
                }
                self.pending_len = n - off;
            } else {
                for (ch, plane) in planes.iter().enumerate() {
                    self.pending[ch].extend_from_slice(&plane[off..]);
                }
                self.pending_len += n - off;
            }
        }
        self.samples += n as u64;
        Ok(n)
    }

    /// End of input: encode the zero-padded tail (if any) and report tallies.
    /// Lookahead flushes the held frame first. Empty input is [`AacError::InvalidPcm`].
    pub fn finish<F>(&mut self, on_frame: F) -> Result<EncodeInfo>
    where
        F: FnMut(EncodedFrame<'_>) -> Result<()>,
    {
        self.ensure_open()?;
        if self.samples == 0 {
            return self.fail(AacError::InvalidPcm(crate::PcmReject::Empty));
        }
        let mut cb = on_frame;
        let mut scratch = std::mem::take(&mut self.scratch);
        let r = self.flush_end(&mut scratch, &mut cb);
        self.scratch = scratch;
        if let Err(e) = r {
            return self.fail(e);
        }
        let n = self.samples;
        let block = FRAME as u64;
        let remainder = (block - (n % block)) % block;
        self.life = Life::Finished;
        Ok(EncodeInfo {
            sample_rate: self.sample_rate,
            channels: self.channels,
            aac_frames: self.aac_frames,
            samples: n,
            bytes: self.bytes,
            priming: block,
            remainder,
            coded_samples: self.aac_frames * block,
            layout: crate::Layout::Mpeg(self.channels as u8),
        })
    }

    fn flush_end<F>(&mut self, scratch: &mut Vec<u8>, cb: &mut F) -> Result<()>
    where
        F: FnMut(EncodedFrame<'_>) -> Result<()>,
    {
        if self.pending_len > 0 {
            let tail = self.pending_len;
            let mut bufs = [[0.0f32; FRAME]; 2];
            for (ch, buf) in bufs.iter_mut().enumerate().take(self.channels) {
                buf[..tail].copy_from_slice(&self.pending[ch][..tail]);
            }
            if self.enc.lookahead_enabled() {
                let planes: Vec<&[f32]> = bufs[..self.channels].iter().map(|b| &b[..]).collect();
                if let Some(au) = self.enc.push_frame(&planes)? {
                    self.deliver(&au, FRAME, scratch, cb)?;
                }
                if let Some(au) = self.enc.flush()? {
                    self.deliver(&au, tail, scratch, cb)?;
                }
            } else {
                self.emit(&bufs, tail, scratch, cb)?;
            }
        } else if self.enc.lookahead_enabled()
            && let Some(au) = self.enc.flush()?
        {
            self.deliver(&au, FRAME, scratch, cb)?;
        }
        for au in self.enc.drain_overlap()? {
            self.deliver(&au, 0, scratch, cb)?;
        }
        Ok(())
    }
}

#[path = "enc_stream_emit.rs"]
mod emit;

#[cfg(test)]
#[path = "enc_stream_tests.rs"]
mod enc_stream_tests;
