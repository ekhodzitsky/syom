#![doc = include_str!("../README.md")]
#![cfg_attr(docsrs, feature(doc_cfg))]

mod decode;
mod engine;
mod error;
mod isomp4;
mod options;
mod out;
mod sniff;

pub use decode::{DecodedAac, decode, decode_bytes, decode_with, read, read_with};
pub use error::{AacError, Result};
pub use isomp4::sniff_is_isobmff;
pub use options::{
    ChannelMode, DEFAULT_MAX_DECODE_SAMPLE_RATE, DEFAULT_MAX_DURATION_SECS,
    DEFAULT_MAX_INPUT_BYTES, DEFAULT_MAX_SAMPLE_RATE, DecodeOptions,
};
pub use sniff::{sniff_aac, sniff_is_adts, sniff_is_latm};

#[cfg(test)]
#[path = "decode_tests.rs"]
mod decode_tests;

#[cfg(test)]
#[path = "error_tests.rs"]
mod error_tests;
