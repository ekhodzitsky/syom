//! Typed errors for the AAC reader (stable `Display`, no `anyhow`).

use crate::budgets::{BudgetExceeded, BudgetKind};
use std::fmt;
use std::io;

/// Result alias for this crate.
pub type Result<T> = std::result::Result<T, AacError>;

/// Errors produced while sniffing or decoding AAC / M4A streams.
///
/// `#[non_exhaustive]`: TASK-51 will add distinguishable unsupported /
/// truncated / lifecycle variants. Match with `_` or named variants;
/// do not string-match [`Self::Format`] / [`Self::Decode`] / [`Self::Encode`].
#[derive(Debug)]
#[non_exhaustive]
pub enum AacError {
    /// Underlying `Read` / `Seek` failure.
    Io(io::Error),
    /// Stream is not ADTS or M4A/AAC.
    NotAac,
    /// Sample rate is zero or above the configured ceiling.
    UnsupportedSampleRate { rate: u32, max: u32 },
    /// Decoded (or declared) duration exceeds the configured budget.
    TooLong { observed_secs: f64, max_secs: f64 },
    /// `DecodeOptions` are not a usable limit set (NaN, negative, etc.).
    InvalidLimits(String),
    /// An independent memory budget was exceeded (`planned == max` is ok).
    Limit {
        kind: BudgetKind,
        observed: u64,
        max: u64,
    },
    /// Structural / demux failure.
    Format(String),
    /// Decoder engine rejected a frame.
    Decode(String),
    /// Encoder rejected input or could not meet constraints.
    Encode(String),
}

impl AacError {
    #[inline]
    pub fn format(msg: impl Into<String>) -> Self {
        Self::Format(msg.into())
    }

    #[inline]
    pub fn decode(msg: impl Into<String>) -> Self {
        Self::Decode(msg.into())
    }

    #[inline]
    pub fn encode(msg: impl Into<String>) -> Self {
        Self::Encode(msg.into())
    }

    #[inline]
    pub fn too_long(observed_secs: f64, max_secs: f64) -> Self {
        Self::TooLong {
            observed_secs,
            max_secs,
        }
    }

    #[inline]
    pub fn sample_rate(rate: u32, max: u32) -> Self {
        Self::UnsupportedSampleRate { rate, max }
    }

    #[inline]
    pub fn invalid_limits(msg: impl Into<String>) -> Self {
        Self::InvalidLimits(msg.into())
    }

    #[inline]
    pub fn limit(kind: BudgetKind, observed: u64, max: u64) -> Self {
        Self::Limit {
            kind,
            observed,
            max,
        }
    }

    /// Whether this should surface as generic unsupported-format upstream.
    pub fn is_format_class(&self) -> bool {
        matches!(self, Self::NotAac | Self::Format(_))
    }
}

impl fmt::Display for AacError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::NotAac => write!(f, "Unsupported audio format"),
            Self::UnsupportedSampleRate { rate, max } => {
                write!(f, "Unsupported sample rate: {rate}Hz (max {max}Hz)")
            }
            Self::TooLong {
                observed_secs,
                max_secs,
            } => write!(
                f,
                "Audio file too long ({observed_secs:.0}s). Maximum supported: {max_secs:.0}s."
            ),
            Self::InvalidLimits(msg) => write!(f, "invalid decode limits: {msg}"),
            Self::Limit {
                kind,
                observed,
                max,
            } => write!(f, "decode budget exceeded ({kind}: {observed} > {max})"),
            Self::Format(msg) | Self::Decode(msg) | Self::Encode(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for AacError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for AacError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<BudgetExceeded> for AacError {
    fn from(e: BudgetExceeded) -> Self {
        Self::Limit {
            kind: e.kind,
            observed: e.observed,
            max: e.max,
        }
    }
}

impl From<crate::engine::error::Error> for AacError {
    fn from(value: crate::engine::error::Error) -> Self {
        Self::decode(value.to_string())
    }
}
