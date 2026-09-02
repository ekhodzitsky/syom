//! Decode options: channel mode and duration / rate caps.

/// Hard upper bound on buffered compressed input (1 GiB).
///
/// Duration caps remain the real guard; this only fences pathological headers
/// before the first frame is found.
pub const DEFAULT_MAX_INPUT_BYTES: u64 = 1 << 30;

/// Duration / rate caps for a lecture-length MP4.
pub const DEFAULT_MAX_DURATION_SECS: f64 = 7200.0;
pub const DEFAULT_MAX_SAMPLE_RATE: u32 = 192_000;
/// Ceiling when converting duration → frame budget.
pub const DEFAULT_MAX_DECODE_SAMPLE_RATE: u32 = 48_000;

/// Whether decoded channels are mixed to mono or kept separate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChannelMode {
    #[default]
    Mono,
    Split,
}

/// Options for `decode_with`.
#[derive(Debug, Clone, PartialEq)]
pub struct DecodeOptions {
    pub channel_mode: ChannelMode,
    /// Hard upper bound on decoded audio length (seconds).
    pub max_duration_secs: f64,
    /// Reject headers whose sample rate is above this value (or zero).
    pub max_sample_rate: u32,
    /// Cap used when deriving the frame budget from `max_duration_secs`.
    pub max_decode_sample_rate: u32,
}

impl Default for DecodeOptions {
    fn default() -> Self {
        Self {
            channel_mode: ChannelMode::Mono,
            max_duration_secs: DEFAULT_MAX_DURATION_SECS,
            max_sample_rate: DEFAULT_MAX_SAMPLE_RATE,
            max_decode_sample_rate: DEFAULT_MAX_DECODE_SAMPLE_RATE,
        }
    }
}

impl DecodeOptions {
    /// Mono + lecture caps (same as [`Default`]).
    #[inline]
    pub fn speech() -> Self {
        Self::default()
    }

    /// No practical duration / rate ceiling.
    #[inline]
    pub fn unbounded() -> Self {
        Self {
            channel_mode: ChannelMode::Split,
            max_duration_secs: f64::INFINITY,
            max_sample_rate: u32::MAX,
            max_decode_sample_rate: u32::MAX,
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

    /// Maximum decoded frames allowed at `sample_rate` under these options.
    #[inline]
    pub fn max_frames(&self, sample_rate: u32) -> usize {
        let rate = sample_rate.min(self.max_decode_sample_rate).max(1) as f64;
        if !self.max_duration_secs.is_finite() {
            return usize::MAX;
        }
        let frames = self.max_duration_secs * rate;
        if !frames.is_finite() || frames <= 0.0 {
            return 0;
        }
        frames.min(usize::MAX as f64) as usize
    }
}

#[cfg(test)]
#[path = "options_tests.rs"]
mod options_tests;
