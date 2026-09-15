//! [`EncodeInfo`] — the finish tallies of the push [`super::Encoder`].

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
    /// Encoder delay in decoded samples: 1024 (LC) or 3018 (HE v1: LC
    /// priming, halfband and SBR chain at the output rate). Skip this many
    /// decoded samples for valid audio.
    pub priming: u64,
    /// Unplayed decoded tail after the source: LC
    /// `ceil(samples/1024)*1024 - samples`; HE `coded − priming − samples`.
    pub remainder: u64,
    /// Decoded length before container trim: `aac_frames * 1024` (LC) or
    /// `aac_frames * 2048` (HE, output rate).
    pub coded_samples: u64,
    /// MPEG layout of the coded planes (`1` mono / `2` stereo).
    pub layout: crate::Layout,
}
