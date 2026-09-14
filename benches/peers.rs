//! Rust peer collectors for the decode-comparison preflight.
//!
//! Compiled as a submodule of `benches/aac.rs` and of `src/decode_cmp_tests.rs`.
//! Parent must bring `Candidate`, `Collect`, `Lane`, and `Pcm` into scope.

use std::io::Cursor;

use super::{Candidate, Collect, Lane, Pcm};
use oxideav_aac::decode::StreamDecoder;
use rusty_aac::AacDecoder;
use symphonia::core::codecs::CodecParameters;
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

pub const RUSTY_AAC: &str = "0.5.0";
pub const OXIDEAV_AAC: &str = "0.1.7";
pub const SYMPHONIA: &str = "0.6.1";

pub fn rusty_candidate() -> Candidate {
    Candidate {
        id: "rusty_aac",
        version: RUSTY_AAC,
        collect: rusty_collect,
    }
}

pub fn oxideav_candidate() -> Candidate {
    Candidate {
        id: "oxideav-aac",
        version: OXIDEAV_AAC,
        collect: oxideav_collect,
    }
}

pub fn symphonia_adts_candidate() -> Candidate {
    Candidate {
        id: "symphonia",
        version: SYMPHONIA,
        collect: symphonia_adts,
    }
}

pub fn symphonia_m4a_candidate() -> Candidate {
    Candidate {
        id: "symphonia",
        version: SYMPHONIA,
        collect: symphonia_m4a,
    }
}

fn is_isobmff(bytes: &[u8]) -> bool {
    bytes.len() >= 8 && &bytes[4..8] == b"ftyp"
}

fn rusty_collect(bytes: &[u8], _lane: Lane) -> Collect {
    if is_isobmff(bytes) {
        return Collect::Unavailable("no ISOBMFF demux".into());
    }
    let mut dec = AacDecoder::new();
    let mut pos = 0usize;
    let mut planes: Vec<Vec<f32>> = Vec::new();
    let mut rate = 0u32;
    let mut ch = 0usize;
    while pos + 7 <= bytes.len() {
        let Ok(hdr) = rusty_aac::parse_adts(&bytes[pos..]) else {
            break;
        };
        if hdr.frame_length == 0 {
            break;
        }
        let end = (pos + hdr.frame_length).min(bytes.len());
        match dec.decode(&bytes[pos..end], None) {
            Ok(pcm) => {
                let nch = pcm.channels.max(1) as usize;
                if pcm.samples.len() % nch != 0 {
                    return Collect::Failed("interleave length".into());
                }
                if planes.is_empty() {
                    ch = nch;
                    rate = pcm.sample_rate;
                    planes = vec![Vec::new(); ch];
                } else if nch != ch || pcm.sample_rate != rate {
                    return Collect::Failed("inconsistent frame geometry".into());
                }
                for (i, &s) in pcm.samples.iter().enumerate() {
                    planes[i % ch].push(s);
                }
            }
            Err(rusty_aac::Error::Again) => {}
            Err(e) => return Collect::Failed(e.to_string()),
        }
        pos = end;
    }
    if planes.is_empty() {
        return Collect::Unavailable("no ADTS frames decoded".into());
    }
    Collect::Pcm(Pcm {
        sample_rate: rate,
        planes,
    })
}

fn oxideav_collect(bytes: &[u8], _lane: Lane) -> Collect {
    if is_isobmff(bytes) {
        return Collect::Unavailable("no ISOBMFF demux".into());
    }
    let mut dec = StreamDecoder::new();
    match dec.decode_all(bytes) {
        Err(e) => Collect::Failed(e.to_string()),
        Ok(frames) if frames.is_empty() => Collect::Unavailable("no frames".into()),
        Ok(frames) => {
            let ch = frames[0].channels;
            let rate = frames[0].sample_rate;
            if ch == 0 {
                return Collect::Failed("zero channels".into());
            }
            let mut planes = vec![Vec::new(); ch];
            for f in &frames {
                if f.channels != ch || f.sample_rate != rate {
                    return Collect::Failed("inconsistent frame geometry".into());
                }
                if f.pcm.len() % ch != 0 {
                    return Collect::Failed("interleave length".into());
                }
                for (i, &s) in f.pcm.iter().enumerate() {
                    planes[i % ch].push(f32::from(s) / 32768.0);
                }
            }
            Collect::Pcm(Pcm {
                sample_rate: rate,
                planes,
            })
        }
    }
}

fn symphonia_adts(bytes: &[u8], _lane: Lane) -> Collect {
    symphonia_collect(bytes, "aac")
}

fn symphonia_m4a(bytes: &[u8], _lane: Lane) -> Collect {
    symphonia_collect(bytes, "m4a")
}

fn symphonia_collect(bytes: &[u8], ext: &str) -> Collect {
    // Cursor<&[u8]>: no per-iter input copy charged only to this peer.
    let src = Cursor::new(bytes);
    let mss = MediaSourceStream::new(Box::new(src), Default::default());
    let mut hint = Hint::new();
    hint.with_extension(ext);
    let Ok(mut format) = symphonia::default::get_probe().probe(
        &hint,
        mss,
        FormatOptions::default(),
        MetadataOptions::default(),
    ) else {
        return Collect::Unavailable("probe failed".into());
    };
    let Some(track) = format.default_track(TrackType::Audio) else {
        return Collect::Unavailable("no audio track".into());
    };
    let Some(CodecParameters::Audio(params)) = track.codec_params.clone() else {
        return Collect::Unavailable("no audio codec params".into());
    };
    let Ok(mut decoder) = symphonia::default::get_codecs()
        .make_audio_decoder(&params, &AudioDecoderOptions::default())
    else {
        return Collect::Unavailable("decoder not constructed".into());
    };
    let mut planes: Vec<Vec<f32>> = Vec::new();
    let mut rate = 0u32;
    let mut decoded = false;
    while let Ok(Some(pkt)) = format.next_packet() {
        let Ok(buf) = decoder.decode(&pkt) else {
            continue;
        };
        if buf.frames() == 0 {
            continue;
        }
        decoded = true;
        rate = buf.spec().rate();
        let mut tmp: Vec<Vec<f32>> = Vec::new();
        buf.copy_to_vecs_planar(&mut tmp);
        if planes.is_empty() {
            planes = tmp;
        } else if planes.len() != tmp.len() {
            return Collect::Failed("inconsistent frame geometry".into());
        } else {
            for (dst, src) in planes.iter_mut().zip(tmp) {
                dst.extend_from_slice(&src);
            }
        }
    }
    if !decoded || planes.is_empty() {
        return Collect::Unavailable("no packets decoded".into());
    }
    Collect::Pcm(Pcm {
        sample_rate: rate,
        planes,
    })
}

/// Drop-output decode for the named discard lane (no accumulated PCM).
pub fn rusty_discard(bytes: &[u8]) -> Result<usize, String> {
    if is_isobmff(bytes) {
        return Err("no ISOBMFF demux".into());
    }
    let mut dec = AacDecoder::new();
    let mut pos = 0usize;
    let mut n = 0usize;
    while pos + 7 <= bytes.len() {
        let Ok(hdr) = rusty_aac::parse_adts(&bytes[pos..]) else {
            break;
        };
        if hdr.frame_length == 0 {
            break;
        }
        let end = (pos + hdr.frame_length).min(bytes.len());
        match dec.decode(&bytes[pos..end], None) {
            Ok(pcm) => n += pcm.frames(),
            Err(rusty_aac::Error::Again) => {}
            Err(e) => return Err(e.to_string()),
        }
        pos = end;
    }
    Ok(n)
}

pub fn oxideav_discard(bytes: &[u8]) -> Result<usize, String> {
    if is_isobmff(bytes) {
        return Err("no ISOBMFF demux".into());
    }
    let mut dec = StreamDecoder::new();
    let frames = dec.decode_all(bytes).map_err(|e| e.to_string())?;
    Ok(frames.iter().map(|f| f.pcm.len() / f.channels.max(1)).sum())
}

pub fn symphonia_adts_discard(bytes: &[u8]) -> Result<usize, String> {
    match symphonia_adts(bytes, Lane::DiscardOutput) {
        Collect::Pcm(p) => Ok(p.planes.first().map(Vec::len).unwrap_or(0)),
        Collect::Failed(e) | Collect::Unavailable(e) => Err(e),
    }
}
