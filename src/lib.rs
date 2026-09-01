//! On-device audio extract. Container in, 16 kHz s16le mono out.

pub mod cli;
pub mod error;

pub(crate) mod extract;
pub(crate) mod resample;
pub(crate) mod wav;

#[cfg(test)]
mod aac_fixture;
