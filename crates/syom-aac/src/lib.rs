//! # syom-aac
//!
//! **AAC-LC** decoder for speech ingest (ADTS + M4A). HE-AAC is Media.
//!
//! ## Engine
//!
//! Spectral decode is original AAC-LC (ISO/IEC 14496-3 / 13818-7).
//! HE-AAC SBR/PS (AOT 5 / 29) is rejected as Media. Comments cite
//! spec section numbers (`§4.6.x`).

#![cfg_attr(docsrs, feature(doc_cfg))]

mod decode;
mod engine;
mod error;
mod isomp4;
mod options;

pub use decode::{DecodedAac, decode, decode_with, sniff_is_adts};
pub use error::{AacError, Result};
pub use isomp4::{AacTrack, parse_aac_track, sniff_is_isobmff, sniff_is_m4a};
pub use options::{
    ChannelMode, DEFAULT_MAX_DECODE_SAMPLE_RATE, DEFAULT_MAX_DURATION_SECS,
    DEFAULT_MAX_INPUT_BYTES, DEFAULT_MAX_SAMPLE_RATE, DecodeOptions,
};
