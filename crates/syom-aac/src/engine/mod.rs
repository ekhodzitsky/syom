//! Vendored AAC-LC / HE-AAC engine (oxideav-aac MIT) + zero-deps bit I/O.
//! Product uses [`decode::StreamDecoder`].

#![allow(missing_docs)]
#![allow(dead_code)]
#![allow(clippy::all)]

pub mod adts;
pub mod asc;
pub mod bits;
pub mod cce;
pub mod channel_layout;
pub mod channel_map;
pub mod crc;
pub mod decode;
pub mod decoded_spectrum;
pub mod dequant;
pub mod element_decode;
pub(crate) mod error;
pub mod extension_payload;
pub mod filterbank;
pub mod gain_control;
pub mod gain_control_data;
pub mod hcr;
pub mod huff_lut;
pub mod ics_body;
pub mod ics_info;
pub mod imdct;
pub mod intensity_stereo;
pub mod ipqf;
pub mod latm;
pub mod ltp;
pub mod ms_stereo;
pub mod pce;
pub mod pcm;
pub mod pns;
pub mod predictor;
pub mod ps_data;
pub mod ps_decoder;
pub mod ps_decorr;
pub mod ps_huffman;
pub mod ps_hybrid;
pub mod ps_map;
pub mod ps_stereo;
pub mod pulse_data;
pub mod raw_data_block;
pub mod rvlc;
pub mod sbr_decoder;
pub mod sbr_dequant;
pub mod sbr_element;
pub mod sbr_env_adjust;
pub mod sbr_envelope;
pub mod sbr_extension;
pub mod sbr_freq_bands;
pub mod sbr_grid;
pub mod sbr_header;
pub mod sbr_hf_gen;
pub mod sbr_huffman;
pub mod sbr_limiter;
pub mod sbr_noise_table;
pub mod sbr_qmf;
pub mod sbr_reconstruct;
pub mod sbr_time_grid;
pub mod scale_factor_data;
pub mod section_data;
pub mod spectral_codebook;
pub mod spectral_data;
pub mod spectrum_huffman;
pub mod ssr;
pub mod swb_offset;
pub mod tns_coef;
pub mod tns_data;
pub mod tns_frame;
pub mod tns_max;

pub use error::Error;
pub type Result<T> = core::result::Result<T, Error>;
