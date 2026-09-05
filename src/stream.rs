//! Streaming decode: a resumable push [`Decoder`] for ADTS and LATM/LOAS
//! byte streams, plus [`decode_streaming`] for complete in-memory buffers
//! of any supported container (ADTS, LATM/LOAS, M4A/ISOBMFF).
//!
//! Peak PCM memory is one AAC frame: decoded planes are borrowed by the
//! frame callback and never accumulated by the decoder itself. The push
//! decoder's input buffer holds at most one partial frame plus the bytes
//! fed but not yet pumped (compacted past 64 KiB of consumed prefix).
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

use crate::engine::adts::AdtsHeader;
use crate::engine::bits::BitReader;
use crate::engine::decode::StreamDecoder;
use crate::engine::error::Error as EngineError;
use crate::engine::latm::{LOAS_SYNC, MuxCfg, read_payload};
use crate::error::{AacError, Result};
use crate::isomp4::sniff_is_isobmff;
use crate::options::{ChannelMode, DEFAULT_MAX_INPUT_BYTES, DecodeOptions};
use crate::sniff::{sniff_is_adts, sniff_is_latm};

mod m4a;

/// One decoded AAC frame: planar f32 in [-1, 1], one plane per channel.
///
/// The planes borrow decoder scratch and are valid **only for the duration
/// of the frame callback** — copy them out to keep them.
pub struct Frame<'a> {
    /// Native sample rate after SBR (2× the core rate for HE-AAC).
    pub sample_rate: u32,
    /// Per-channel sample count in this frame.
    pub samples: usize,
    /// Planes in the same channel order as [`crate::DecodedAac`].
    pub planar: &'a [&'a [f32]],
}

/// Tallies from a finished stream decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamInfo {
    /// Native sample rate of the stream (0 if nothing decodable was seen).
    pub sample_rate: u32,
    /// Channels per emitted frame (0 if nothing decodable was seen).
    pub channels: usize,
    /// Payload frames decoded, including edit-list-skipped ones.
    pub aac_frames: u64,
    /// Output samples per channel after any `elst` skip.
    pub samples: u64,
}

/// Container the push decoder has locked onto.
enum Container {
    Unknown,
    Adts,
    Latm,
}

/// Resumable push decoder for ADTS and LATM/LOAS byte streams. Peak PCM RAM
/// is O(frame). M4A/ISOBMFF input is rejected (`moov` needs random access;
/// use [`decode_streaming`] on the full slice).
///
/// Bytes are buffered until the container is sniffable (~8 bytes) and until
/// a whole compressed frame has arrived; partial trailing frames are
/// dropped by [`Decoder::finish`], matching one-shot decode. Whether input
/// is undecodable is likewise decided at `finish` ([`AacError::NotAac`];
/// LATM/LOAS keeps its decode-class error), not mid-`feed`.
pub struct Decoder {
    opts: DecodeOptions,
    buf: Vec<u8>,
    pos: usize,
    bytes_fed: u64,
    container: Container,
    dec: StreamDecoder,
    /// Sticky LATM `StreamMuxConfig()`; persists across feeds.
    mux: Option<MuxCfg>,
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
}

impl Decoder {
    pub fn new(opts: DecodeOptions) -> Self {
        let mut dec = StreamDecoder::new();
        dec.mix_down_mono = matches!(opts.channel_mode, ChannelMode::Mono);
        Self {
            opts,
            buf: Vec::new(),
            pos: 0,
            bytes_fed: 0,
            container: Container::Unknown,
            dec,
            mux: None,
            mono_scratch: Vec::new(),
            locked: None,
            max_samples: 0,
            aac_frames: 0,
            samples_decoded: 0,
            samples_out: 0,
            emitted: 0,
        }
    }

    /// Feed bytes; `on_frame` fires once per completed AAC frame. Returns
    /// the number of bytes consumed (all of them; partial frames are
    /// buffered internally).
    pub fn feed<F>(&mut self, bytes: &[u8], on_frame: F) -> Result<usize>
    where
        F: FnMut(Frame<'_>) -> Result<()>,
    {
        self.bytes_fed = self.bytes_fed.saturating_add(bytes.len() as u64);
        if self.bytes_fed > DEFAULT_MAX_INPUT_BYTES {
            return Err(AacError::too_long(
                self.bytes_fed as f64 / 40_000.0, // rough lower bound, like decode_with
                self.opts.max_duration_secs,
            ));
        }
        self.buf.extend_from_slice(bytes);
        let mut cb = on_frame;
        self.pump(&mut cb, false)?;
        // Compact the consumed prefix so `buf` stays O(frame + fresh input).
        if self.pos >= 64 * 1024 && self.pos >= self.buf.len() / 2 {
            self.buf.drain(..self.pos);
            self.pos = 0;
        }
        Ok(bytes.len())
    }

    /// End of input: flush, drop a partial trailing frame, and report
    /// tallies. Errors [`AacError::NotAac`] when nothing decodable was seen.
    pub fn finish<F>(mut self, on_frame: F) -> Result<StreamInfo>
    where
        F: FnMut(Frame<'_>) -> Result<()>,
    {
        let mut cb = on_frame;
        self.pump(&mut cb, true)?;
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
            channels,
            aac_frames: self.aac_frames,
            samples: self.samples_out,
        })
    }

    fn pump<F>(&mut self, on_frame: &mut F, final_flush: bool) -> Result<()>
    where
        F: FnMut(Frame<'_>) -> Result<()>,
    {
        if matches!(self.container, Container::Unknown) {
            let avail = &self.buf[self.pos..];
            if avail.len() < 8 && !final_flush {
                return Ok(()); // wait for enough bytes to sniff
            }
            if sniff_is_isobmff(avail) {
                return Err(AacError::format(
                    "aac: M4A/ISOBMFF needs random access; use decode_streaming",
                ));
            }
            // Sniff order and ADTS fallback mirror `decode_with`.
            self.container = if sniff_is_adts(avail) || !sniff_is_latm(avail) {
                Container::Adts
            } else {
                Container::Latm
            };
        }
        match self.container {
            Container::Adts => self.pump_adts(on_frame),
            Container::Latm => self.pump_latm(on_frame),
            Container::Unknown => Ok(()),
        }
    }

    fn pump_adts<F>(&mut self, on_frame: &mut F) -> Result<()>
    where
        F: FnMut(Frame<'_>) -> Result<()>,
    {
        loop {
            let avail = &self.buf[self.pos..];
            let (hdr, payload_off) = match AdtsHeader::parse(avail) {
                Ok(v) => v,
                // Partial header: wait for more bytes (finish drops it).
                Err(EngineError::UnexpectedEnd) => return Ok(()),
                // Definite garbage: resync one byte, like the one-shot loop.
                Err(_) => {
                    self.pos += 1;
                    continue;
                }
            };
            let frame_len = usize::from(hdr.aac_frame_length);
            if avail.len() < frame_len {
                return Ok(()); // partial frame: wait (finish drops it)
            }
            let payload = &avail[payload_off..frame_len];
            let mono = matches!(self.opts.channel_mode, ChannelMode::Mono);
            let idx = self.aac_frames;
            let rate = if mono {
                self.mono_scratch.clear();
                self.dec
                    .decode_raw_mono_f32(
                        hdr.audio_object_type(),
                        hdr.sampling_frequency_index,
                        hdr.sample_rate(),
                        hdr.channel_configuration,
                        hdr.number_of_raw_data_blocks_in_frame,
                        payload,
                        &mut self.mono_scratch,
                    )
                    .map_err(|e| {
                        AacError::decode(format!("aac: decode failed at frame {idx}: {e:?}"))
                    })?
            } else {
                self.dec
                    .decode_frame_scaled(
                        hdr.audio_object_type(),
                        hdr.sampling_frequency_index,
                        hdr.sample_rate(),
                        hdr.channel_configuration,
                        payload,
                    )
                    .map_err(|e| {
                        AacError::decode(format!("aac: decode failed at frame {idx}: {e}"))
                    })?
            };
            self.pos += frame_len;
            self.aac_frames += 1;
            self.emit(rate, on_frame)?;
        }
    }

    fn pump_latm<F>(&mut self, on_frame: &mut F) -> Result<()>
    where
        F: FnMut(Frame<'_>) -> Result<()>,
    {
        loop {
            let avail = &self.buf[self.pos..];
            if avail.len() < 3 {
                return Ok(()); // retain the trailing bytes across feeds
            }
            let v = (u32::from(avail[0]) << 16) | (u32::from(avail[1]) << 8) | u32::from(avail[2]);
            if v >> 13 != LOAS_SYNC {
                self.pos += 1; // byte-scan resync, like the one-shot loop
                continue;
            }
            let mux_len = (v & 0x1FFF) as usize;
            if avail.len() < 3 + mux_len {
                return Ok(()); // partial LOAS frame: wait (finish drops it)
            }
            let body = &avail[3..3 + mux_len];
            let mut br = BitReader::new(body);
            let use_same = br.read_bit().map_err(AacError::from)?;
            if !use_same {
                self.mux = Some(MuxCfg::parse(&mut br).map_err(AacError::from)?);
            }
            let cfg = self
                .mux
                .as_ref()
                .ok_or(EngineError::LatmNoPreviousMuxConfig)
                .map_err(AacError::from)?;
            let (aot, fs, sr, ch) = (
                cfg.asc.aot,
                cfg.asc.sampling_frequency_index,
                cfg.asc.sample_rate,
                cfg.asc.channel_configuration,
            );
            let payload = read_payload(&mut br, cfg).map_err(AacError::from)?;
            let mono = matches!(self.opts.channel_mode, ChannelMode::Mono);
            let rate = if mono {
                self.mono_scratch.clear();
                self.dec
                    .decode_raw_mono_f32(aot, fs, sr, ch, 1, &payload, &mut self.mono_scratch)
                    .map_err(AacError::from)?
            } else {
                self.dec
                    .decode_frame_scaled(aot, fs, sr, ch, &payload)
                    .map_err(AacError::from)?
            };
            self.pos += 3 + mux_len;
            self.aac_frames += 1;
            self.emit(rate, on_frame)?;
        }
    }

    /// Caps + consistency for one decoded frame, then the callback.
    fn emit<F>(&mut self, rate: u32, on_frame: &mut F) -> Result<()>
    where
        F: FnMut(Frame<'_>) -> Result<()>,
    {
        let mono = matches!(self.opts.channel_mode, ChannelMode::Mono);
        let (n_ch, n_samples) = if mono {
            (1, self.mono_scratch.len())
        } else {
            let planes = self.dec.frame_planes();
            (planes.len(), planes.first().map_or(0, Vec::len))
        };
        if n_ch == 0 {
            return Ok(()); // channel-less frame: consumed, not emitted
        }
        match self.locked {
            None => {
                if rate == 0 || rate > self.opts.max_sample_rate {
                    return Err(AacError::sample_rate(rate, self.opts.max_sample_rate));
                }
                self.locked = Some((rate, n_ch));
                self.max_samples = self.opts.max_frames(rate);
            }
            Some((r, c)) => {
                if rate != r {
                    return Err(AacError::decode(format!(
                        "aac: sample rate changed mid-stream ({r}Hz → {rate}Hz)"
                    )));
                }
                if c != n_ch {
                    return Err(AacError::decode(format!(
                        "aac: channel count changed mid-stream ({c} → {n_ch})"
                    )));
                }
            }
        }
        self.samples_decoded += n_samples as u64;
        if self.samples_decoded > self.max_samples as u64 {
            let observed_s = self.samples_decoded as f64 / f64::from(rate.max(1));
            return Err(AacError::too_long(observed_s, self.opts.max_duration_secs));
        }
        self.samples_out += n_samples as u64;
        self.emitted += 1;
        // Borrow the planes for the duration of the callback only.
        let mono_plane: [&[f32]; 1];
        let split_planes: Vec<&[f32]>;
        let planar: &[&[f32]] = if mono {
            mono_plane = [self.mono_scratch.as_slice()];
            &mono_plane
        } else {
            split_planes = self.dec.frame_planes().iter().map(Vec::as_slice).collect();
            &split_planes
        };
        on_frame(Frame {
            sample_rate: rate,
            samples: n_samples,
            planar,
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
    if data.len() as u64 > DEFAULT_MAX_INPUT_BYTES {
        return Err(AacError::too_long(
            data.len() as f64 / 40_000.0, // rough lower bound only for message
            opts.max_duration_secs,
        ));
    }
    if sniff_is_isobmff(data) {
        return m4a::stream_m4a(data, opts, on_frame);
    }
    let mut dec = Decoder::new(opts.clone());
    let mut cb = on_frame;
    dec.feed(data, &mut cb)?;
    dec.finish(&mut cb)
}
