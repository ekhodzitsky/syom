//! Original AAC-LC engine (ISO/IEC 14496-3 / 13818-7).
//!
//! No SBR / PS / LATM / SSR / LTP. Product entry is [`decode::StreamDecoder`].

pub mod adts;
pub mod asc;
pub mod bits;
pub mod decode;
pub(crate) mod error;
pub mod filterbank;
pub mod huff;
pub mod huff_esc;
pub mod huff_pair;
pub mod huff_quad;
pub mod ics;
pub mod ics_body;
pub mod imdct;
pub mod pns;
pub mod section;
pub mod sf;
pub mod sf_tab;
pub mod skip;
pub mod spectrum;
pub mod stereo;
pub mod swb;
pub mod tns;

#[allow(unused_imports)]
pub use error::Error;
pub use error::Result;

#[cfg(test)]
#[path = "huff_tests.rs"]
mod huff_tests;

#[cfg(test)]
#[path = "ics_tests.rs"]
mod ics_tests;

#[cfg(test)]
#[path = "tools_tests.rs"]
mod tools_tests;

#[cfg(test)]
#[path = "golden_tests.rs"]
mod golden_tests;
