#![doc = include_str!("../README.md")]
#![cfg_attr(docsrs, feature(doc_cfg))]

mod decode;
mod encode;
mod engine;
mod error;
mod isomp4;
mod options;
mod sniff;
mod stream;

pub use decode::{DecodedAac, decode, decode_bytes, decode_with, read, read_with};
pub use encode::{encode, encode_with, write, write_with};
pub use error::{AacError, Result};
pub use isomp4::sniff_is_isobmff;
pub use options::{
    ChannelMode, DEFAULT_MAX_DECODE_SAMPLE_RATE, DEFAULT_MAX_DURATION_SECS,
    DEFAULT_MAX_INPUT_BYTES, DEFAULT_MAX_SAMPLE_RATE, DecodeOptions, EncodeContainer,
    EncodeOptions,
};
pub use sniff::{sniff_aac, sniff_is_adts, sniff_is_latm};
pub use stream::{Decoder, Frame, StreamInfo, decode_streaming};

#[cfg(test)]
#[path = "decode_tests.rs"]
mod decode_tests;

#[cfg(test)]
#[path = "encode_tests.rs"]
mod encode_tests;

#[cfg(test)]
#[path = "decode_mc_tests.rs"]
mod decode_mc_tests;

#[cfg(test)]
#[path = "stream_tests.rs"]
mod stream_tests;

#[cfg(test)]
#[path = "stream_latm_tests.rs"]
mod stream_latm_tests;

#[cfg(test)]
#[path = "stream_m4a_tests.rs"]
mod stream_m4a_tests;

#[cfg(test)]
#[path = "error_tests.rs"]
mod error_tests;
