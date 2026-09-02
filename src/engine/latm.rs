//! LATM / LOAS transport — ISO/IEC 14496-3 §1.7 (AAC payloads).

use super::asc::AudioSpecificConfig;
use super::bits::BitReader;
use super::decode::StreamDecoder;
use super::error::{Error, Result};

/// LOAS `AudioSyncStream` syncword.
pub const LOAS_SYNC: u32 = 0x2B7;

/// Decode a LOAS/LATM byte stream to planar f32 (filterbank scale) + rate.
pub fn decode_loas_planar(
    data: &[u8],
    mix_down_mono: bool,
    max_samples: usize,
) -> Result<(u32, Vec<Vec<f32>>)> {
    let mut pos = 0usize;
    let mut dec = StreamDecoder::new();
    dec.mix_down_mono = mix_down_mono;
    let mut tracks: Vec<Vec<f32>> = Vec::new();
    let mut rate = 0u32;
    let mut mux: Option<MuxCfg> = None;
    while pos + 3 <= data.len() {
        let v = (u32::from(data[pos]) << 16)
            | (u32::from(data[pos + 1]) << 8)
            | u32::from(data[pos + 2]);
        let sync = v >> 13;
        if sync != LOAS_SYNC {
            pos += 1;
            continue;
        }
        let mux_len = (v & 0x1FFF) as usize;
        let start = pos + 3;
        let end = start.saturating_add(mux_len);
        if end > data.len() {
            break;
        }
        let mut br = BitReader::new(&data[start..end]);
        let use_same = br.read_bit()?;
        if !use_same {
            mux = Some(MuxCfg::parse(&mut br)?);
        }
        let cfg = mux.as_ref().ok_or(Error::LatmNoPreviousMuxConfig)?;
        let payload = read_payload(&mut br, cfg)?;
        let frame = dec.decode_raw_data_block(
            cfg.asc.aot,
            cfg.asc.sampling_frequency_index,
            cfg.asc.sample_rate,
            cfg.asc.channel_configuration,
            1,
            &payload,
        )?;
        if rate == 0 {
            rate = frame.sample_rate;
        }
        if tracks.is_empty() {
            tracks = vec![Vec::new(); frame.planar.len().max(1)];
        }
        while tracks.len() < frame.planar.len() {
            tracks.push(Vec::new());
        }
        for (i, ch) in frame.planar.iter().enumerate() {
            if let Some(dst) = tracks.get_mut(i) {
                dst.extend_from_slice(ch);
            }
        }
        if tracks.first().map(Vec::len).unwrap_or(0) > max_samples {
            return Err(Error::Format("latm: too long"));
        }
        pos = end;
    }
    if tracks.iter().all(Vec::is_empty) {
        return Err(Error::LoasSyncInvalid);
    }
    Ok((rate, tracks))
}

struct MuxCfg {
    asc: AudioSpecificConfig,
    frame_length_type: u8,
    frame_length: u32,
}

impl MuxCfg {
    fn parse(br: &mut BitReader<'_>) -> Result<Self> {
        let audio_mux_version = br.read_bit()?;
        if audio_mux_version {
            let audio_mux_version_a = br.read_bit()?;
            if audio_mux_version_a {
                return Err(Error::LatmAudioMuxVersionAReserved);
            }
            let _tara = latm_value(br)?;
        }
        let _same_time = br.read_bit()?;
        let _num_sub_frames = br.read(6)?;
        let num_program = br.read(4)?;
        let num_layer = br.read(3)?;
        if num_program != 0 || num_layer != 0 {
            return Err(Error::LatmConfigOutOfRange);
        }
        if audio_mux_version {
            let asc_len = latm_value(br)?;
            let start = br.bit_position();
            let (asc, _) = AudioSpecificConfig::parse_from_reader(br)?;
            let used = br.bit_position().saturating_sub(start);
            if used < u64::from(asc_len) {
                br.skip((u64::from(asc_len) - used) as u32)?;
            }
            let frame_length_type = br.read(3)? as u8;
            let frame_length = read_frame_len(br, frame_length_type)?;
            skip_mux_tail(br)?;
            return Ok(Self {
                asc,
                frame_length_type,
                frame_length,
            });
        }
        let (asc, _) = AudioSpecificConfig::parse_from_reader(br)?;
        let frame_length_type = br.read(3)? as u8;
        let frame_length = read_frame_len(br, frame_length_type)?;
        skip_mux_tail(br)?;
        Ok(Self {
            asc,
            frame_length_type,
            frame_length,
        })
    }
}

fn read_frame_len(br: &mut BitReader<'_>, ty: u8) -> Result<u32> {
    match ty {
        0 => {
            let _ = br.read(8)?; // latmBufferFullness
            Ok(0)
        }
        1 => Ok(br.read(9)?),
        _ => Err(Error::LatmUnsupportedFrameLengthType),
    }
}

fn skip_mux_tail(br: &mut BitReader<'_>) -> Result<()> {
    let other = br.read_bit()?;
    if other {
        let escaped = br.read_bit()?;
        if escaped {
            loop {
                let more = br.read_bit()?;
                let _ = br.read(8)?;
                if !more {
                    break;
                }
            }
        } else {
            let n = br.read(8)? + 1;
            br.skip(n)?;
        }
    }
    let crc = br.read_bit()?;
    if crc {
        let _ = br.read(8)?;
    }
    Ok(())
}

fn latm_value(br: &mut BitReader<'_>) -> Result<u32> {
    let mut v = br.read(8)?;
    while br.read_bit()? {
        v = (v << 8) | br.read(8)?;
    }
    Ok(v)
}

fn read_payload(br: &mut BitReader<'_>, cfg: &MuxCfg) -> Result<Vec<u8>> {
    let nbytes = if cfg.frame_length_type == 1 {
        cfg.frame_length.div_ceil(8) as usize
    } else {
        let mut n = 0u32;
        loop {
            let b = br.read(8)?;
            n += b;
            if b != 255 {
                break;
            }
        }
        n as usize
    };
    let mut out = Vec::with_capacity(nbytes);
    for _ in 0..nbytes {
        out.push(br.read(8)? as u8);
    }
    Ok(out)
}

#[cfg(test)]
#[path = "latm_tests.rs"]
mod latm_tests;
