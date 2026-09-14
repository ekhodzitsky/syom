//! Decode options: channel mode and duration / rate caps.
//!
//! Memory budgets (compressed bytes, collected PCM, channels, M4A index,
//! resident workspace) live in [`crate::MemoryBudgets`] and are independent of
//! `max_duration_secs`. `speech()` duration and `ChannelMode` are unchanged.

use crate::budgets::MemoryBudgets;

/// Hard upper bound on a finite compressed buffer or file (1 GiB).
///
/// Applies to one-shot `decode` / `decode_with` / `decode_streaming` /
/// `read`. Independent of `max_duration_secs`. Streaming `Decoder::feed`
/// must not inherit this as a lifetime cap ([`crate::InputScope`]; TASK-25).
pub const DEFAULT_MAX_INPUT_BYTES: u64 = 1 << 30;

/// Duration / rate caps for a lecture-length MP4.
pub const DEFAULT_MAX_DURATION_SECS: f64 = 7200.0;
pub const DEFAULT_MAX_SAMPLE_RATE: u32 = 192_000;
/// Ceiling when converting duration → frame budget.
pub const DEFAULT_MAX_DECODE_SAMPLE_RATE: u32 = 48_000;

/// Whether decoded channels are mixed to mono or kept separate.
///
/// [`Default`] is [`Self::Mono`]: it follows [`DecodeOptions::speech`],
/// the one-call [`crate::decode`] / [`crate::read`] contract. General
/// audio uses [`DecodeOptions::audio`] or [`DecodeOptions::unbounded`]
/// ([`Self::Split`]), not a silent default flip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChannelMode {
    #[default]
    Mono,
    Split,
}

/// Options for `decode_with`.
///
/// `#[non_exhaustive]`: new fields (memory, labels, timing) may appear
/// before 1.0. Construct via [`Self::speech`], [`Self::audio`],
/// [`Self::unbounded`], or `..` update syntax.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct DecodeOptions {
    pub channel_mode: ChannelMode,
    /// Hard upper bound on decoded audio length (seconds).
    pub max_duration_secs: f64,
    /// Reject headers whose sample rate is above this value (or zero).
    pub max_sample_rate: u32,
    /// Cap used when deriving the frame budget from `max_duration_secs`.
    pub max_decode_sample_rate: u32,
    /// Independent input/output/workspace budgets (TASK-23/24).
    pub memory: MemoryBudgets,
}

impl Default for DecodeOptions {
    fn default() -> Self {
        Self {
            channel_mode: ChannelMode::Mono,
            max_duration_secs: DEFAULT_MAX_DURATION_SECS,
            max_sample_rate: DEFAULT_MAX_SAMPLE_RATE,
            max_decode_sample_rate: DEFAULT_MAX_DECODE_SAMPLE_RATE,
            memory: MemoryBudgets::default(),
        }
    }
}

impl DecodeOptions {
    /// Mono + lecture caps (same as [`Default`]). One-call [`crate::decode`]
    /// / [`crate::read`] use this. Stereo and 5.1 are mixed to one plane.
    #[inline]
    pub fn speech() -> Self {
        Self::default()
    }

    /// Split channels + the same 2 h / 192 kHz lecture caps as [`speech`].
    ///
    /// Collection still uses the default 4 GiB / 8-channel fences, not
    /// [`Self::unbounded`]. `decode` / `read` stay speech-mono; pass this
    /// to [`crate::decode_with`] / [`crate::read_with`] when coded layout
    /// must be kept.
    ///
    /// ```
    /// use syom::{decode, decode_with, DecodeOptions};
    /// let bytes = include_bytes!("goldens/lecture.m4a");
    /// assert_eq!(decode(bytes)?.channels.len(), 1);
    /// assert_eq!(decode_with(bytes, &DecodeOptions::audio())?.channels.len(), 2);
    /// # Ok::<(), syom::AacError>(())
    /// ```
    #[inline]
    pub fn audio() -> Self {
        Self {
            channel_mode: ChannelMode::Split,
            ..Self::speech()
        }
    }

    /// Split channels, no practical duration / rate ceiling.
    #[inline]
    pub fn unbounded() -> Self {
        Self {
            channel_mode: ChannelMode::Split,
            max_duration_secs: f64::INFINITY,
            max_sample_rate: u32::MAX,
            max_decode_sample_rate: u32::MAX,
            memory: MemoryBudgets::unbounded_collection(),
        }
    }

    #[inline]
    pub fn with_channel_mode(mut self, mode: ChannelMode) -> Self {
        self.channel_mode = mode;
        self
    }

    #[inline]
    pub fn with_max_duration_secs(mut self, secs: f64) -> Self {
        self.max_duration_secs = secs;
        self
    }

    #[inline]
    pub fn with_max_sample_rate(mut self, rate: u32) -> Self {
        self.max_sample_rate = rate;
        self
    }

    #[inline]
    pub fn with_memory(mut self, memory: MemoryBudgets) -> Self {
        self.memory = memory;
        self
    }

    /// Reject limits that would disable protection or are not a duration.
    ///
    /// [`f64::INFINITY`] is the explicit unlimited contract used by
    /// [`Self::unbounded`]. NaN, negative infinity and negative durations
    /// error. `max_sample_rate == 0` and a zero decode-rate cap with a
    /// finite duration also error.
    pub fn validate(&self) -> crate::Result<()> {
        let d = self.max_duration_secs;
        if d.is_nan() {
            return Err(crate::AacError::invalid_limits("max_duration_secs is NaN"));
        }
        if d.is_infinite() && d < 0.0 {
            return Err(crate::AacError::invalid_limits("max_duration_secs is -inf"));
        }
        if d.is_finite() && d < 0.0 {
            return Err(crate::AacError::invalid_limits(
                "max_duration_secs is negative",
            ));
        }
        if self.max_sample_rate == 0 {
            return Err(crate::AacError::invalid_limits("max_sample_rate is 0"));
        }
        if self.max_decode_sample_rate == 0 && d.is_finite() {
            return Err(crate::AacError::invalid_limits(
                "max_decode_sample_rate is 0",
            ));
        }
        self.memory
            .validate()
            .map_err(|e| crate::AacError::invalid_limits(format!("{} budget is 0", e.kind)))?;
        Ok(())
    }

    /// Maximum decoded frames allowed at `sample_rate` under these options.
    ///
    /// Call [`Self::validate`] before decoding. Invalid durations yield 0
    /// here so a missed check cannot open an unlimited cap.
    #[inline]
    pub fn max_frames(&self, sample_rate: u32) -> usize {
        let d = self.max_duration_secs;
        if d.is_infinite() && d > 0.0 {
            return usize::MAX;
        }
        if !d.is_finite() || d <= 0.0 {
            return 0;
        }
        let cap = self.max_decode_sample_rate.max(1);
        let rate = sample_rate.min(cap) as f64;
        let frames = d * rate;
        if !frames.is_finite() || frames >= usize::MAX as f64 {
            return usize::MAX;
        }
        frames as usize
    }
}

/// Output container for the encoder.
///
/// `#[non_exhaustive]`: LATM/LOAS encode is a later transport (TASK-94).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum EncodeContainer {
    /// ADTS elementary stream.
    #[default]
    Adts,
    /// M4A / ISOBMFF (`ftyp` + `mdat` + `moov`).
    M4a,
}

/// Options for `encode_with` / `write_with`.
///
/// `#[non_exhaustive]`: rate-control / preset fields may appear (TASK-65).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct EncodeOptions {
    pub container: EncodeContainer,
    /// ABR target in bits per second of the whole stream (TASK-65/66).
    /// Per-frame ceiling plus unused bytes after `ID_END` so payload/valid
    /// stays within ±3% on ≥10 s non-silent tracks. Silence and `N < 2048`
    /// may undershoot. Not CBR: ADTS `adts_buffer_fullness = 0x7FF` (no
    /// bit reservoir). Must not exceed AAC-LC 6144 bits/channel per
    /// 1024-sample frame (`6144 · channels · sample_rate / 1024`).
    pub bitrate_bps: u32,
    /// One-frame attack lookahead (default off). When on, the attack
    /// detector runs one frame ahead of the encode: an attack anywhere in
    /// frame N+1 makes frame N a LongStart (its start-window slope covers
    /// the pre-attack tail) and frame N+1 an EightShort, so even a click in
    /// the first samples of a frame is coded on short windows — the causal
    /// default codes such early attacks with the LongStart's flat-region
    /// long transform, which leaves reduced-but-audible pre-echo. Costs one
    /// extra frame of latency (1024 samples) in both one-shot and push
    /// encode; the push `Encoder` flushes the held frame at `finish`.
    pub lookahead: bool,
    /// Terhardt absolute-threshold floor (TASK-68). Default off: production
    /// goldens stay byte-identical. When on, bands below ATH at 0 dBFS =
    /// 96 dB SPL are dropped in addition to the −60 dB relative floor.
    pub ath: bool,
    /// Johnston SFM tonality (TASK-69). Default off. When on, noise-like
    /// bands get a lower `target_q` (0.25×); the coded mask is unchanged.
    pub tonality: bool,
    /// Short-window TNS (TASK-72). Default off — long TNS unchanged.
    pub short_tns: bool,
    /// Short-window grouping (TASK-71). Default off — 8 groups of 1.
    pub short_group: bool,
}

impl Default for EncodeOptions {
    fn default() -> Self {
        Self {
            container: EncodeContainer::Adts,
            bitrate_bps: 128_000,
            lookahead: false,
            ath: false,
            tonality: false,
            short_tns: false,
            short_group: false,
        }
    }
}

impl EncodeOptions {
    /// ADTS at 128 kbps (same as [`Default`]).
    #[inline]
    pub fn adts() -> Self {
        Self::default()
    }

    /// M4A at 128 kbps.
    #[inline]
    pub fn m4a() -> Self {
        Self {
            container: EncodeContainer::M4a,
            ..Self::default()
        }
    }

    #[inline]
    pub fn with_container(mut self, container: EncodeContainer) -> Self {
        self.container = container;
        self
    }

    #[inline]
    pub fn with_bitrate_bps(mut self, bps: u32) -> Self {
        self.bitrate_bps = bps;
        self
    }

    /// One-frame attack lookahead: better pre-echo suppression on
    /// early-in-frame onsets, at one extra frame (1024 samples) of latency.
    /// Off by default; honored identically by one-shot encode and the push
    /// `Encoder` (which stays byte-exact with one-shot for the same
    /// options).
    ///
    /// ```
    /// use syom::{DecodeOptions, EncodeOptions, encode_with, decode_with};
    /// let pcm = vec![vec![0.0f32; 4096]];
    /// let opts = EncodeOptions::adts().with_lookahead(true);
    /// let adts = encode_with(&pcm, 48_000, &opts)?;
    /// let dec = decode_with(&adts, &DecodeOptions::unbounded())?;
    /// assert_eq!(dec.channels[0].len(), 5 * 1024); // +1 overlap-drain frame
    /// # Ok::<(), syom::AacError>(())
    /// ```
    #[inline]
    pub fn with_lookahead(mut self, on: bool) -> Self {
        self.lookahead = on;
        self
    }

    /// Terhardt ATH floor. Off by default; one-shot and push encode honor
    /// it identically (byte-exact for the same options).
    #[inline]
    pub fn with_ath(mut self, on: bool) -> Self {
        self.ath = on;
        self
    }

    /// Johnston SFM tonality. Off by default; one-shot and push encode
    /// honor it identically (byte-exact for the same options).
    #[inline]
    pub fn with_tonality(mut self, on: bool) -> Self {
        self.tonality = on;
        self
    }

    /// Short-window TNS. Off by default; long-window TNS is unchanged.
    #[inline]
    pub fn with_short_tns(mut self, on: bool) -> Self {
        self.short_tns = on;
        self
    }

    /// Short-window grouping. Off by default (8×1). One-shot and push
    /// encode honor it identically (byte-exact for the same options).
    #[inline]
    pub fn with_short_group(mut self, on: bool) -> Self {
        self.short_group = on;
        self
    }
}

#[cfg(test)]
#[path = "options_tests.rs"]
mod options_tests;
