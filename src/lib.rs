#![doc = include_str!("../README.md")]
#![cfg_attr(docsrs, feature(doc_cfg))]

mod budgets;
mod decode;
#[doc(hidden)]
pub mod decode_cmp;
mod enc_stream;
mod encode;
#[doc(hidden)]
pub mod encode_cmp;
mod engine;
mod error;
mod isomp4;
mod layout;
mod m4a_write;
#[doc(hidden)]
pub mod mem_iso;
mod options;
mod sniff;
mod stream;

pub use budgets::{
    BudgetExceeded, BudgetKind, DEFAULT_MAX_BOX_DEPTH, DEFAULT_MAX_BUFFERED_INPUT_BYTES,
    DEFAULT_MAX_CHANNELS, DEFAULT_MAX_DECLARED_AU_BYTES, DEFAULT_MAX_INDEX_ENTRIES,
    DEFAULT_MAX_METADATA_BYTES, DEFAULT_MAX_OUTPUT_BYTES, DEFAULT_MAX_WORKSPACE_BYTES, InputScope,
    MemoryBudgets, allocable_bytes, check_planned, input_cap_applies, pcm_bytes,
    workspace_lower_bound_bytes,
};
#[cfg(test)]
pub(crate) use decode::read_file_capped;
pub use decode::{
    DecodedAac, decode, decode_bytes, decode_read, decode_read_with, decode_seek, decode_seek_with,
    decode_with, read, read_with,
};
pub use enc_stream::{EncodeInfo, EncodedFrame, Encoder};
pub use encode::{encode, encode_with, write, write_with};
pub use error::{AacError, LifecycleState, MalformedKind, PcmReject, Result, UnsupportedFeature};
pub use isomp4::sniff_is_isobmff;
pub use layout::{Channel, FrameMeta, Layout, MAX_PLANES, mpeg_channels};
pub use options::{
    ChannelMode, DEFAULT_MAX_DECODE_SAMPLE_RATE, DEFAULT_MAX_DURATION_SECS,
    DEFAULT_MAX_INPUT_BYTES, DEFAULT_MAX_SAMPLE_RATE, DecodeOptions, EncodeContainer,
    EncodeOptions,
};
pub use sniff::{sniff_aac, sniff_is_adts, sniff_is_latm};
pub use stream::{
    Decoder, Frame, M4aSeek, StreamInfo, decode_read_streaming, decode_seek_streaming,
    decode_streaming, preroll_aus,
};

#[cfg(test)]
#[path = "decode_tests.rs"]
mod decode_tests;

#[cfg(test)]
#[path = "decode_budget_tests.rs"]
mod decode_budget_tests;

#[cfg(test)]
#[path = "docs_tests.rs"]
mod docs_tests;

#[cfg(test)]
#[path = "api_contract_tests.rs"]
mod api_contract_tests;

#[cfg(test)]
#[path = "decode_read_tests.rs"]
mod decode_read_tests;

#[cfg(test)]
#[path = "decode_cmp_tests.rs"]
mod decode_cmp_tests;

#[cfg(test)]
#[path = "baseline_tests.rs"]
mod baseline_tests;

#[cfg(test)]
#[path = "corpus_tests.rs"]
mod corpus_tests;

#[cfg(test)]
#[path = "listen_protocol_tests.rs"]
mod listen_protocol_tests;

#[cfg(test)]
#[path = "oracle_tests.rs"]
mod oracle_tests;

#[cfg(test)]
#[path = "conformance_tests.rs"]
mod conformance_tests;

#[cfg(test)]
#[path = "lab_isolation_tests.rs"]
mod lab_isolation_tests;

#[cfg(test)]
#[path = "footprint_tests.rs"]
mod footprint_tests;

#[cfg(test)]
#[path = "mem_iso_tests.rs"]
mod mem_iso_tests;

#[cfg(test)]
#[path = "encode_tests.rs"]
mod encode_tests;

#[cfg(test)]
#[path = "encode_prime_tests.rs"]
mod encode_prime_tests;

#[cfg(test)]
#[path = "encode_pcm_tests.rs"]
mod encode_pcm_tests;

#[cfg(test)]
#[path = "encode_cmp_tests.rs"]
mod encode_cmp_tests;

#[cfg(test)]
#[path = "encode_rate_tests.rs"]
mod encode_rate_tests;

#[cfg(test)]
#[path = "encode_transient_tests.rs"]
mod encode_transient_tests;

#[cfg(test)]
#[path = "encode_ath_tests.rs"]
mod encode_ath_tests;

#[cfg(test)]
#[path = "encode_tonality_tests.rs"]
mod encode_tonality_tests;

#[cfg(test)]
#[path = "encode_preecho_tests.rs"]
mod encode_preecho_tests;

#[cfg(test)]
#[path = "encode_short_tns_tests.rs"]
mod encode_short_tns_tests;

#[cfg(test)]
#[path = "encode_group_tests.rs"]
mod encode_group_tests;

#[cfg(test)]
#[path = "encode_section_tests.rs"]
mod encode_section_tests;

#[cfg(test)]
#[path = "encode_refine_tests.rs"]
mod encode_refine_tests;

#[cfg(test)]
#[path = "encode_pns_tests.rs"]
mod encode_pns_tests;

#[cfg(test)]
#[path = "encode_is_tests.rs"]
mod encode_is_tests;

#[cfg(test)]
#[path = "decode_mc_tests.rs"]
mod decode_mc_tests;

#[cfg(test)]
#[path = "stream_tests.rs"]
mod stream_tests;

#[cfg(test)]
#[path = "stream_reset_tests.rs"]
mod stream_reset_tests;

#[cfg(test)]
#[path = "stream_budget_tests.rs"]
mod stream_budget_tests;

#[cfg(test)]
#[path = "stream_latm_tests.rs"]
mod stream_latm_tests;

#[cfg(test)]
#[path = "stream_m4a_tests.rs"]
mod stream_m4a_tests;

#[cfg(test)]
#[path = "stream_m4a_seek_tests.rs"]
mod stream_m4a_seek_tests;

#[cfg(test)]
#[path = "stream_au_tests.rs"]
mod stream_au_tests;

#[cfg(test)]
#[path = "stream_read_tests.rs"]
mod stream_read_tests;

#[cfg(test)]
#[path = "error_tests.rs"]
mod error_tests;

#[cfg(test)]
#[path = "fuzz_smoke_tests.rs"]
mod fuzz_smoke_tests;

#[cfg(test)]
#[path = "fuzz_parse_tests.rs"]
mod fuzz_parse_tests;

#[cfg(test)]
#[path = "fuzz_stream_tests.rs"]
mod fuzz_stream_tests;
