//! Engine errors. ISO/IEC 14496-3 / 13818-7 parse and tool failures.

use std::fmt;

/// Local engine result.
pub type Result<T> = core::result::Result<T, Error>;

/// Failures while parsing or reconstructing AAC-LC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Bitstream ended mid-field.
    UnexpectedEnd,
    /// ADTS syncword was not `0xFFF` (§1.A.2.2.1).
    AdtsSyncNotFound,
    /// ADTS `layer` was not zero.
    AdtsLayerNonZero,
    /// Reserved ADTS `sampling_frequency_index` (13/14) or illegal 15.
    AdtsReservedSampleRateIndex,
    /// `aac_frame_length` smaller than the header.
    AdtsFrameLengthTooSmall,
    /// `audioObjectType` is not AAC-LC (2). HE-AAC (5/29) is Media.
    UnsupportedAot(u8),
    /// `frameLengthFlag == 1` (960-line) is out of v1.
    UnsupportedFrameLength,
    /// `samplingFrequencyIndex` has no SWB table.
    UnsupportedSampleRateIndex(u8),
    /// Malformed `ics_info` / grouping / `max_sfb`.
    IcsInfoInvalid,
    /// `section_data` ran past `max_sfb`.
    SectionDataOverrun,
    /// Huffman codebook index with no table (12, or >15).
    InvalidCodebook(u8),
    /// Huffman prefix did not match a complete codeword.
    HuffmanInvalid,
    /// Spectral / pulse / TNS geometry walked off the window.
    SpectrumInvalid,
    /// Filterbank `spec` length disagrees with `window_sequence`.
    FilterbankInvalid,
    /// Channel element the LC decoder does not reconstruct (kept for skip).
    UnsupportedElement(u8),
    /// Generic structural reject (PCE/LTP/predictor on the LC path).
    Format(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEnd => write!(f, "aac: unexpected end of bitstream"),
            Self::AdtsSyncNotFound => write!(f, "aac: ADTS sync not found"),
            Self::AdtsLayerNonZero => write!(f, "aac: ADTS layer must be 0"),
            Self::AdtsReservedSampleRateIndex => write!(f, "aac: reserved ADTS sample rate"),
            Self::AdtsFrameLengthTooSmall => write!(f, "aac: ADTS frame length too small"),
            Self::UnsupportedAot(a) => write!(f, "aac: unsupported audioObjectType {a}"),
            Self::UnsupportedFrameLength => write!(f, "aac: 960-line frames are Media"),
            Self::UnsupportedSampleRateIndex(i) => {
                write!(f, "aac: no SWB table for samplingFrequencyIndex {i}")
            }
            Self::IcsInfoInvalid => write!(f, "aac: invalid ics_info"),
            Self::SectionDataOverrun => write!(f, "aac: section_data overrun"),
            Self::InvalidCodebook(c) => write!(f, "aac: invalid codebook {c}"),
            Self::HuffmanInvalid => write!(f, "aac: Huffman codeword invalid"),
            Self::SpectrumInvalid => write!(f, "aac: spectral geometry invalid"),
            Self::FilterbankInvalid => write!(f, "aac: filterbank length mismatch"),
            Self::UnsupportedElement(e) => write!(f, "aac: unsupported id_syn_ele {e}"),
            Self::Format(msg) => write!(f, "aac: {msg}"),
        }
    }
}

impl std::error::Error for Error {}
