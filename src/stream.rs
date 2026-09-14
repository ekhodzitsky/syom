//! Streaming decode: a resumable push [`Decoder`] for ADTS and LATM/LOAS
//! byte streams, [`Decoder::from_asc`] / [`Decoder::decode_au`] for raw
//! access units, plus [`decode_streaming`] for complete in-memory buffers
//! of any supported container (ADTS, LATM/LOAS, M4A/ISOBMFF).
//!
//! Peak PCM memory is one AAC frame: decoded planes are borrowed by the
//! frame callback and never accumulated by the decoder itself. The push
//! decoder's input buffer is capped at
//! [`crate::DEFAULT_MAX_BUFFERED_INPUT_BYTES`] (resident, not a lifetime
//! total — F07 / TASK-25).
//!
//! ```
//! use syom::{DecodeOptions, Decoder};
//! let adts = include_bytes!("goldens/sine48.adts");
//! let mut dec = Decoder::new(DecodeOptions::speech());
//! let mut samples = 0usize;
//! for chunk in adts.chunks(7) {
//!     dec.feed(chunk, |f| {
//!         samples += f.samples;
//!         Ok(())
//!     })?;
//! }
//! let info = dec.finish(|f| {
//!     samples += f.samples;
//!     Ok(())
//! })?;
//! assert_eq!(info.sample_rate, 48_000);
//! assert_eq!(samples as u64, info.samples);
//! # Ok::<(), syom::AacError>(())
//! ```

use crate::budgets::{BudgetExceeded, BudgetKind};
use crate::engine::asc::AudioSpecificConfig;
use crate::engine::decode::StreamDecoder;
use crate::engine::error::Error as EngineError;
use crate::engine::latm::MuxCfg;
use crate::error::{AacError, Result};
use crate::isomp4::sniff_is_isobmff;
use crate::options::{ChannelMode, DecodeOptions};

mod au;
mod frame;
mod m4a;
mod m4a_pos;
mod m4a_seek;
mod pump;
mod read;

pub use frame::{Frame, StreamInfo};
pub use m4a_pos::{M4aSeek, preroll_aus};
pub use m4a_seek::decode_seek_streaming;
pub use read::decode_read_streaming;

/// Container the push decoder has locked onto.
enum Container {
    Unknown,
    Adts,
    Latm,
    /// ASC + complete `raw_data_block()` via [`Decoder::decode_au`].
    Au,
}

/// Resumable push decoder for ADTS and LATM/LOAS byte streams, or raw
/// access units after [`Decoder::from_asc`]. Peak PCM RAM is O(frame).
/// M4A/ISOBMFF input is rejected on `feed` (`moov` needs random access;
/// use [`decode_streaming`] on the full slice).
///
/// Bytes are buffered until the container is sniffable (~8 bytes) and until
/// a whole compressed frame has arrived; partial trailing frames are
/// dropped by [`Decoder::finish`], matching one-shot decode. Whether input
/// is undecodable is likewise decided at `finish` ([`AacError::NotAac`];
/// LATM/LOAS keeps its decode-class error), not mid-`feed`.
///
/// Lifecycle: **open** → `feed` / `finish`; a parser, limit, or callback
/// error moves to **failed**; a successful `finish` moves to **finished**.
/// `feed` / `finish` on failed or finished return an error until
/// [`Decoder::reset`]. `reset` clears LC/HE/PS overlap and counters and
/// keeps prepared workspace capacity.
pub struct Decoder {
    opts: DecodeOptions,
    buf: Vec<u8>,
    pos: usize,
    container: Container,
    dec: StreamDecoder,
    /// Sticky LATM `StreamMuxConfig()`; persists across feeds.
    mux: Option<MuxCfg>,
    /// Parsed ASC when this instance is in raw-AU mode.
    au: Option<AudioSpecificConfig>,
    /// Mono fast-path target, cleared per frame.
    mono_scratch: Vec<f32>,
    /// `(sample_rate, channels)` locked by the first emitted frame.
    locked: Option<(u32, usize)>,
    max_samples: usize,
    aac_frames: u64,
    /// Per-channel samples decoded (cap counter, pre-skip).
    samples_decoded: u64,
    /// Per-channel samples handed to the callback.
    samples_out: u64,
    emitted: u64,
    /// Last sink-frame sample count (0 until the first mono-fast-path frame).
    sink_frame_samples: usize,
    life: Life,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Life {
    Open,
    Finished,
    Failed,
}

impl Decoder {
    pub fn new(opts: DecodeOptions) -> Self {
        let mut dec = StreamDecoder::new();
        dec.mix_down_mono = matches!(opts.channel_mode, ChannelMode::Mono);
        Self {
            opts,
            buf: Vec::new(),
            pos: 0,
            container: Container::Unknown,
            dec,
            mux: None,
            au: None,
            mono_scratch: Vec::new(),
            locked: None,
            max_samples: 0,
            aac_frames: 0,
            samples_decoded: 0,
            samples_out: 0,
            emitted: 0,
            sink_frame_samples: 0,
            life: Life::Open,
        }
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
        let e = e.with_truncated_at(self.pos as u64);
        self.life = Life::Failed;
        Err(e)
    }

    /// AAC frames handed to the callback (including a frame whose callback
    /// then returned an error).
    #[must_use]
    pub fn frames(&self) -> u64 {
        self.aac_frames
    }

    /// Per-channel samples handed to the callback.
    #[must_use]
    pub fn samples(&self) -> u64 {
        self.samples_out
    }

    #[must_use]
    pub fn is_failed(&self) -> bool {
        self.life == Life::Failed
    }

    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.life == Life::Finished
    }

    /// Drop LC/HE/PS overlap, mux config, and counters. Prepared workspace
    /// capacity (filterbank slots, spectral/PCM planes, input buffer) is
    /// kept. [`DecodeOptions`] are unchanged. No global cache.
    pub fn reset(&mut self) {
        self.dec.reset();
        self.buf.clear();
        self.pos = 0;
        self.container = Container::Unknown;
        self.mux = None;
        self.au = None;
        self.mono_scratch.clear();
        self.locked = None;
        self.max_samples = 0;
        self.aac_frames = 0;
        self.samples_decoded = 0;
        self.samples_out = 0;
        self.emitted = 0;
        self.sink_frame_samples = 0;
        self.life = Life::Open;
    }

    #[cfg(test)]
    pub(crate) fn test_pcm_cap(&self) -> usize {
        self.dec.pcm_l.capacity()
    }

    #[cfg(test)]
    pub(crate) fn test_buf_cap(&self) -> usize {
        self.buf.capacity()
    }

    /// Feed bytes; `on_frame` fires once per completed AAC frame. Returns
    /// the number of bytes consumed (all of them; partial frames are
    /// buffered internally). After an error the decoder is **failed**:
    /// further `feed` / `finish` error until [`Self::reset`].
    pub fn feed<F>(&mut self, bytes: &[u8], on_frame: F) -> Result<usize>
    where
        F: FnMut(Frame<'_>) -> Result<()>,
    {
        self.ensure_open()?;
        self.opts.validate()?;
        let cap = usize::try_from(self.opts.memory.max_buffered_input_bytes).unwrap_or(usize::MAX);
        let cap = cap.max(1);
        let mut cb = on_frame;
        let mut off = 0usize;
        while off < bytes.len() {
            let mut buf = std::mem::take(&mut self.buf);
            self.compact(&mut buf);
            let used = buf.len();
            if used >= cap {
                let res = self.pump(&buf, None, None, &mut cb, false);
                if res.is_ok() {
                    self.compact(&mut buf);
                }
                self.buf = buf;
                if let Err(e) = res {
                    return self.fail(e);
                }
                if self.buf.len() >= cap {
                    return self.fail(AacError::from(BudgetExceeded {
                        kind: BudgetKind::Input,
                        observed: self.buf.len() as u64,
                        max: self.opts.memory.max_buffered_input_bytes,
                    }));
                }
                continue;
            }
            let n = (bytes.len() - off).min(cap - used);
            buf.extend_from_slice(&bytes[off..off + n]);
            off += n;
            let res = self.pump(&buf, None, None, &mut cb, false);
            if res.is_ok() {
                self.compact(&mut buf);
            }
            self.buf = buf;
            if let Err(e) = res {
                return self.fail(e);
            }
        }
        Ok(bytes.len())
    }

    fn compact(&mut self, buf: &mut Vec<u8>) {
        if self.pos == 0 {
            return;
        }
        if self.pos >= buf.len() {
            buf.clear();
            self.pos = 0;
            return;
        }
        buf.drain(..self.pos);
        self.pos = 0;
    }

    /// End of input: flush, drop a partial trailing frame, and report
    /// tallies. Errors [`AacError::NotAac`] when nothing decodable was seen.
    /// On success the decoder is **finished**; on error it is **failed**.
    pub fn finish<F>(&mut self, on_frame: F) -> Result<StreamInfo>
    where
        F: FnMut(Frame<'_>) -> Result<()>,
    {
        self.ensure_open()?;
        self.opts.validate()?;
        let buf = std::mem::take(&mut self.buf);
        let mut cb = on_frame;
        if let Err(e) = self.pump(&buf, None, None, &mut cb, true) {
            return self.fail(e);
        }
        match self.tallies() {
            Ok(info) => {
                self.life = Life::Finished;
                Ok(info)
            }
            Err(e) => self.fail(e),
        }
    }

    /// One-shot over a complete slice (ADTS/LATM): sniffs and pumps without
    /// copying the input into the push buffer. Equivalent to one `feed` of
    /// the whole slice plus `finish`.
    fn decode_slice<F>(mut self, data: &[u8], on_frame: F) -> Result<StreamInfo>
    where
        F: FnMut(Frame<'_>) -> Result<()>,
    {
        self.opts.validate()?;
        let mut cb = on_frame;
        self.pump(data, None, None, &mut cb, true)?;
        self.tallies()
    }

    /// One-shot mono over a complete slice: decode straight into `dst`,
    /// skipping the per-frame scratch + callback copy. Caller guarantees
    /// [`ChannelMode::Mono`]; `est_frames` pre-sizes `dst` after frame 1.
    fn decode_slice_mono(
        mut self,
        data: &[u8],
        est_frames: Option<usize>,
        dst: &mut Vec<f32>,
    ) -> Result<StreamInfo> {
        fn ignore(_: Frame<'_>) -> Result<()> {
            Ok(())
        }
        self.opts.validate()?;
        self.pump(data, Some(dst), est_frames, &mut ignore, true)?;
        self.tallies()
    }

    /// Terminal tallies; [`AacError::NotAac`] when nothing decodable was seen.
    fn tallies(&self) -> Result<StreamInfo> {
        if self.emitted == 0 {
            return Err(match self.container {
                // One-shot LATM reports an empty LOAS stream as decode-class.
                Container::Latm => AacError::from(EngineError::LoasSyncInvalid),
                _ => AacError::NotAac,
            });
        }
        let (sample_rate, channels) = self.locked.unwrap_or((0, 0));
        Ok(StreamInfo {
            sample_rate,
            core_rate: self.dec.last_core_rate(),
            channels,
            layout: self.dec.last_meta().layout,
            aac_frames: self.aac_frames,
            samples: self.samples_out,
            priming: None,
            remainder: None,
        })
    }
}

/// Stream-decode a complete in-memory buffer (any container, incl. M4A).
///
/// `on_frame` fires once per decoded AAC frame; return an error from it to
/// abort. Peak PCM RAM is O(frame); M4A still holds the input slice (the
/// frame index needs `moov`).
pub fn decode_streaming<F>(data: &[u8], opts: &DecodeOptions, on_frame: F) -> Result<StreamInfo>
where
    F: FnMut(Frame<'_>) -> Result<()>,
{
    opts.validate()?;
    opts.memory.check_input_finite(data.len() as u64)?;
    if sniff_is_isobmff(data) {
        return m4a::stream_m4a(data, opts, on_frame);
    }
    Decoder::new(opts.clone()).decode_slice(data, on_frame)
}

/// One-shot mono fast path for [`crate::decode_with`]: append each frame's
/// mixed-mono samples straight into `dst` — no per-frame scratch, no
/// callback copy. ADTS/LATM only; M4A keeps the [`decode_streaming`] path.
pub(crate) fn decode_streaming_mono_into(
    data: &[u8],
    opts: &DecodeOptions,
    est_frames: Option<usize>,
    dst: &mut Vec<f32>,
) -> Result<StreamInfo> {
    opts.validate()?;
    opts.memory.check_input_finite(data.len() as u64)?;
    Decoder::new(opts.clone()).decode_slice_mono(data, est_frames, dst)
}
