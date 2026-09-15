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
    /// Raw `raw_data_block` access units (push [`crate::Encoder`] only).
    /// One-shot [`crate::encode_with`] rejects this: muxers need per-AU
    /// sizes. Pair with [`crate::Encoder::asc`] and
    /// [`crate::wrap_adts_au`] / [`crate::mux_raw_lc_m4a`].
    Raw,
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
    /// Bandwise leftover-bit sf refine (TASK-74). Default off.
    pub band_refine: bool,
    /// Perceptual noise substitution (TASK-75). Default off.
    pub pns: bool,
    /// Intensity stereo (TASK-76). Default off.
    pub intensity: bool,
    /// HE-AAC v1 (SBR) instead of AAC-LC (TASK-90). Default off — `encode`
    /// stays LC. The LC core runs at half the input rate (16 / 22.05 /
    /// 24 / 32 / 44.1 / 48 kHz input only, else
    /// [`crate::UnsupportedFeature::EncodeHeRate`]), mono / stereo,
    /// `bitrate_bps` is the whole-stream budget (core + SBR; ≤ 6144 bits
    /// per channel per core frame). ADTS signals SBR implicitly (header
    /// carries the core rate); M4A and [`crate::Encoder::asc`] carry the
    /// explicit two-rate AOT 5 config. Priming is 3018 output samples.
    pub he: bool,
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
            band_refine: false,
            pns: false,
            intensity: false,
            he: false,
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

    /// Raw access units at 128 kbps (push [`crate::Encoder`] only).
    #[inline]
    pub fn raw() -> Self {
        Self {
            container: EncodeContainer::Raw,
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

    /// Spend leftover frame bits on `sf[b] -= 1` for underfunded long-window
    /// bands (at most 16 keeps / 48 tries). Off by default.
    #[inline]
    pub fn with_band_refine(mut self, on: bool) -> Self {
        self.band_refine = on;
        self
    }

    /// Perceptual noise substitution on long-window noise-like HF bands.
    /// Off by default; one-shot and push encode honor it identically.
    #[inline]
    pub fn with_pns(mut self, on: bool) -> Self {
        self.pns = on;
        self
    }

    /// Intensity stereo on long-window HF bands. Off by default; one-shot
    /// and push encode honor it identically.
    #[inline]
    pub fn with_intensity(mut self, on: bool) -> Self {
        self.intensity = on;
        self
    }

    /// HE-AAC v1 (SBR on an LC core at half the input rate). Off by
    /// default. One-shot and push encode honor it identically.
    ///
    /// ```
    /// use syom::{DecodeOptions, EncodeOptions, decode_with, encode_with, probe};
    /// let pcm = vec![vec![0.0f32; 48_000]; 2];
    /// let opts = EncodeOptions::adts().with_bitrate_bps(48_000).with_he(true);
    /// let adts = encode_with(&pcm, 48_000, &opts)?;
    /// assert_eq!(probe(&adts)?.meta.core_rate, 24_000); // ADTS header: LC at the core rate
    /// let dec = decode_with(&adts, &DecodeOptions::audio())?;
    /// assert_eq!((dec.sample_rate, dec.core_rate), (48_000, 24_000));
    /// # Ok::<(), syom::AacError>(())
    /// ```
    #[inline]
    pub fn with_he(mut self, on: bool) -> Self {
        self.he = on;
        self
    }
}

#[path = "options_validate.rs"]
mod validate;

#[cfg(test)]
#[path = "options_tests.rs"]
mod options_tests;
