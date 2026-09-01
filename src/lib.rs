//! On-device audio extract. Container in, 16 kHz s16le mono out.

pub mod cli;
pub mod error;

pub(crate) mod boxes;
pub(crate) mod extract;
pub(crate) mod pcm_mp4;
pub(crate) mod resample;
pub(crate) mod table;
pub(crate) mod wav;

#[cfg(test)]
mod aac_fixture;
#[cfg(test)]
mod mp4_fixture;
