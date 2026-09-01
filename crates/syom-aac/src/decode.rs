//! Product ADTS / M4A drivers over the LC engine.

use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::engine::adts::AdtsHeader;
use crate::engine::asc::AudioSpecificConfig;
use crate::engine::decode::{DecodedFrame, StreamDecoder};
use crate::error::{AacError, Result};
use crate::isomp4;
use crate::options::{ChannelMode, DecodeOptions};

/// Hard cap on buffered input size (same rationale as the historical MP3 path).
const MAX_INPUT_BYTES: usize = 1 << 30; // 1 GiB

/// Decoded AAC at native sample rate (planar f32, mono-mixed or split).
#[derive(Debug, Clone)]
pub struct DecodedAac {
    pub sample_rate: u32,
    pub channels: Vec<Vec<f32>>,
}

/// Sniff ADTS: 12-bit sync, layer 0, plausible frame length.
pub fn sniff_is_adts(data: &[u8]) -> bool {
    if data.len() < 6 {
        return false;
    }
    let h = [data[0], data[1], data[2], data[3], data[4], data[5]];
    adts_prefix_plausible(&h)
}

fn adts_prefix_plausible(h: &[u8; 6]) -> bool {
    if h[0] != 0xFF || (h[1] & 0xF0) != 0xF0 {
        return false;
    }
    if (h[1] >> 1) & 3 != 0 {
        return false;
    }
    let sf_index = (h[2] >> 2) & 0x0F;
    if sf_index == 0x0F {
        return false;
    }
    let frame_length =
        (usize::from(h[3] & 3) << 11) | (usize::from(h[4]) << 3) | (usize::from(h[5]) >> 5);
    frame_length >= 7
}

/// Decode ADTS or M4A bytes with product STT defaults.
#[inline]
pub fn decode(data: &[u8]) -> Result<DecodedAac> {
    decode_with(data, &DecodeOptions::product())
}

/// Decode ADTS or M4A bytes under `opts`.
pub fn decode_with(data: &[u8], opts: &DecodeOptions) -> Result<DecodedAac> {
    if data.len() > MAX_INPUT_BYTES {
        return Err(AacError::too_long(
            data.len() as f64 / 40_000.0, // rough lower bound only for message
            opts.max_duration_secs,
        ));
    }
    if data.len() >= 8 && &data[4..8] == b"ftyp" {
        decode_m4a(data, opts)
    } else {
        decode_adts(data, opts)
    }
}

/// Filterbank scale is ±32768; this matches the historical i16→f32 map.
const INV_S16: f32 = 1.0 / 32768.0;

#[inline]
fn conv_f64(v: f64) -> f32 {
    (v as f32) * INV_S16
}

struct Out {
    sample_rate: u32,
    channels: usize,
    tracks: Vec<Vec<f32>>,
    samples_decoded: usize,
    max_samples: usize,
}

impl Out {
    fn new(first: &DecodedFrame, mode: ChannelMode, opts: &DecodeOptions) -> Result<Self> {
        let sample_rate = first.sample_rate;
        if sample_rate == 0 || sample_rate > opts.max_sample_rate {
            return Err(AacError::sample_rate(sample_rate, opts.max_sample_rate));
        }
        let max_samples = opts.max_frames(sample_rate);
        let tracks = match mode {
            ChannelMode::Mono => vec![Vec::new()],
            ChannelMode::Split => (0..first.channels).map(|_| Vec::new()).collect(),
        };
        Ok(Out {
            sample_rate,
            channels: first.channels,
            tracks,
            samples_decoded: 0,
            max_samples,
        })
    }

    fn reserve_per_track(&mut self, frames_hint: usize) {
        let n = self.tracks.len().max(1);
        let per = frames_hint / n;
        for t in &mut self.tracks {
            t.reserve(per);
        }
    }

    fn push(
        &mut self,
        frame: &DecodedFrame,
        mode: ChannelMode,
        opts: &DecodeOptions,
    ) -> Result<()> {
        if frame.channels == 0 {
            return Ok(());
        }
        if frame.sample_rate != self.sample_rate {
            return Err(AacError::decode(format!(
                "aac: sample rate changed mid-stream ({}Hz → {}Hz)",
                self.sample_rate, frame.sample_rate
            )));
        }
        if frame.channels != self.channels {
            return Err(AacError::decode(format!(
                "aac: channel count changed mid-stream ({} → {})",
                self.channels, frame.channels
            )));
        }
        if frame.planar.len() != self.channels {
            return Err(AacError::decode("aac: planar channel count mismatch"));
        }
        let per_ch = frame.planar.first().map(|c| c.len()).unwrap_or(0);
        match mode {
            ChannelMode::Mono if self.channels == 1 => {
                let dst = &mut self.tracks[0];
                dst.reserve(per_ch);
                for &v in &frame.planar[0] {
                    dst.push(conv_f64(v));
                }
            }
            ChannelMode::Mono => {
                let n = self.channels as f32;
                let dst = &mut self.tracks[0];
                dst.reserve(per_ch);
                for i in 0..per_ch {
                    let mut sum = 0.0_f32;
                    for ch in &frame.planar {
                        sum += conv_f64(ch[i]);
                    }
                    dst.push(sum / n);
                }
            }
            ChannelMode::Split => {
                for (ch, dst) in self.tracks.iter_mut().enumerate() {
                    dst.reserve(per_ch);
                    for &v in &frame.planar[ch] {
                        dst.push(conv_f64(v));
                    }
                }
            }
        }
        self.samples_decoded += per_ch;
        if self.samples_decoded > self.max_samples {
            let observed_s = self.samples_decoded as f64 / self.sample_rate.max(1) as f64;
            return Err(AacError::too_long(observed_s, opts.max_duration_secs));
        }
        Ok(())
    }

    fn finish(self) -> DecodedAac {
        DecodedAac {
            sample_rate: self.sample_rate,
            channels: self.tracks,
        }
    }
}

fn decode_frame_guarded(
    dec: &mut StreamDecoder,
    f: impl FnOnce(&mut StreamDecoder) -> crate::engine::Result<DecodedFrame>,
    frame_idx: u64,
) -> Result<DecodedFrame> {
    match catch_unwind(AssertUnwindSafe(|| f(dec))) {
        Ok(Ok(frame)) => Ok(frame),
        Ok(Err(e)) => Err(AacError::decode(format!(
            "aac: decode failed at frame {frame_idx}: {e:?}"
        ))),
        Err(_) => Err(AacError::decode(format!(
            "aac: decoder panicked at frame {frame_idx}"
        ))),
    }
}

fn push_frame(
    out: &mut Option<Out>,
    frame: &DecodedFrame,
    mode: ChannelMode,
    opts: &DecodeOptions,
) -> Result<()> {
    match out {
        None => {
            if frame.channels == 0 {
                return Ok(());
            }
            let mut o = Out::new(frame, mode, opts)?;
            o.push(frame, mode, opts)?;
            *out = Some(o);
        }
        Some(o) => o.push(frame, mode, opts)?,
    }
    Ok(())
}

fn decode_adts(data: &[u8], opts: &DecodeOptions) -> Result<DecodedAac> {
    let mode = opts.channel_mode;
    let mut dec = StreamDecoder::new();
    dec.mix_down_mono = matches!(mode, ChannelMode::Mono);
    let mut out: Option<Out> = None;
    let mut frames_decoded = 0u64;
    let mut pos = 0usize;

    while pos < data.len() {
        let parsed = catch_unwind(AssertUnwindSafe(|| AdtsHeader::parse(&data[pos..])));
        let (hdr, payload_off) = match parsed {
            Ok(Ok(v)) => v,
            Ok(Err(_)) | Err(_) => {
                if frames_decoded == 0 {
                    return Err(AacError::NotAac);
                }
                pos += 1;
                continue;
            }
        };
        let frame_len = usize::from(hdr.aac_frame_length);
        if frame_len < payload_off {
            if frames_decoded == 0 {
                return Err(AacError::NotAac);
            }
            pos += 1;
            continue;
        }
        if pos + frame_len > data.len() {
            if frames_decoded == 0 {
                return Err(AacError::NotAac);
            }
            break;
        }
        let payload = &data[pos + payload_off..pos + frame_len];
        let frame =
            decode_frame_guarded(&mut dec, |d| d.decode_frame(&hdr, payload), frames_decoded)?;
        push_frame(&mut out, &frame, mode, opts)?;
        frames_decoded += 1;
        pos += frame_len;
    }

    out.map(Out::finish).ok_or(AacError::NotAac)
}

fn decode_m4a(data: &[u8], opts: &DecodeOptions) -> Result<DecodedAac> {
    let mode = opts.channel_mode;
    let track = isomp4::parse_aac_track(data).map_err(|e| {
        if e.is_format_class() {
            AacError::NotAac
        } else {
            e
        }
    })?;

    let asc = catch_unwind(AssertUnwindSafe(|| AudioSpecificConfig::parse(&track.asc)))
        .map_err(|_| AacError::format("aac: ASC parser panicked"))
        .and_then(|r| {
            r.map_err(|e| AacError::format(format!("aac: bad AudioSpecificConfig: {e:?}")))
        })?
        .0;

    let core_rate = asc.sample_rate;
    if core_rate == 0 || core_rate > opts.max_sample_rate {
        return Err(AacError::sample_rate(core_rate, opts.max_sample_rate));
    }
    let max_samples = opts.max_frames(core_rate);
    if track.total_samples > max_samples as u64 {
        let observed_s = track.total_samples as f64 / core_rate as f64;
        return Err(AacError::too_long(observed_s, opts.max_duration_secs));
    }

    let mut dec = StreamDecoder::new();
    dec.mix_down_mono = matches!(mode, ChannelMode::Mono);
    let reserve = (track.total_samples as usize).min(max_samples);

    if matches!(mode, ChannelMode::Mono) {
        let mut pcm = Vec::with_capacity(reserve);
        for (idx, &(off, len)) in track.frames.iter().enumerate() {
            let end = off
                .checked_add(u64::from(len))
                .ok_or_else(|| AacError::format("aac: sample range overflow"))?
                as usize;
            let start = off as usize;
            if end > data.len() {
                return Err(AacError::format("aac: sample range past end of file"));
            }
            let payload = data
                .get(start..end)
                .ok_or_else(|| AacError::format("aac: sample range past end of file"))?;
            match dec.decode_raw_mono_f32(
                asc.aot,
                asc.sampling_frequency_index,
                asc.sample_rate,
                asc.channel_configuration,
                1,
                payload,
                &mut pcm,
            ) {
                Ok(_) => {}
                Err(_) if !pcm.is_empty() => continue,
                Err(e) => {
                    return Err(AacError::decode(format!(
                        "aac: decode failed at frame {idx}: {e:?}"
                    )));
                }
            }
            if pcm.len() > max_samples {
                let observed_s = pcm.len() as f64 / core_rate.max(1) as f64;
                return Err(AacError::too_long(observed_s, opts.max_duration_secs));
            }
        }
        if pcm.is_empty() {
            return Err(AacError::NotAac);
        }
        return Ok(DecodedAac {
            sample_rate: core_rate,
            channels: vec![pcm],
        });
    }

    let mut out: Option<Out> = None;
    for (idx, &(off, len)) in track.frames.iter().enumerate() {
        let end = off
            .checked_add(u64::from(len))
            .ok_or_else(|| AacError::format("aac: sample range overflow"))?
            as usize;
        let start = off as usize;
        if end > data.len() {
            return Err(AacError::format("aac: sample range past end of file"));
        }
        let payload = data
            .get(start..end)
            .ok_or_else(|| AacError::format("aac: sample range past end of file"))?;
        let frame = match dec.decode_raw_data_block(
            asc.aot,
            asc.sampling_frequency_index,
            asc.sample_rate,
            asc.channel_configuration,
            1,
            payload,
        ) {
            Ok(frame) => frame,
            Err(_) if out.is_some() => continue,
            Err(e) => {
                return Err(AacError::decode(format!(
                    "aac: decode failed at frame {idx}: {e:?}"
                )));
            }
        };
        match &mut out {
            None => {
                if frame.channels == 0 {
                    continue;
                }
                let mut o = Out::new(&frame, mode, opts)?;
                o.reserve_per_track(reserve);
                o.push(&frame, mode, opts)?;
                out = Some(o);
            }
            Some(o) => o.push(&frame, mode, opts)?,
        }
    }
    out.map(Out::finish).ok_or(AacError::NotAac)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniff_rejects_short_and_mp3ish() {
        assert!(!sniff_is_adts(&[]));
        assert!(!sniff_is_adts(&[0xFF, 0xFB, 0, 0, 0, 0])); // mp3-ish layer bits
    }

    #[test]
    fn empty_decode_is_not_aac() {
        assert!(matches!(decode(&[]), Err(AacError::NotAac)));
        assert!(matches!(decode(b"hello"), Err(AacError::NotAac)));
    }
}
