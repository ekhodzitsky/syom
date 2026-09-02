//! ADTS / M4A / LATM drivers over the LC engine.

use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::engine::adts::AdtsHeader;
use crate::engine::asc::AudioSpecificConfig;
use crate::engine::decode::{DecodedFrame, StreamDecoder};
use crate::error::{AacError, Result};
use crate::isomp4::{self, sniff_is_isobmff};
use crate::options::{ChannelMode, DecodeOptions};
use crate::sniff::{sniff_is_adts, sniff_is_latm};

/// Hard cap on buffered input size (same rationale as the historical MP3 path).
const MAX_INPUT_BYTES: usize = 1 << 30; // 1 GiB

/// Decoded AAC at native sample rate (planar f32, mono-mixed or split).
#[derive(Debug, Clone)]
pub struct DecodedAac {
    pub sample_rate: u32,
    pub channels: Vec<Vec<f32>>,
}

/// Decode ADTS, M4A, or LATM/LOAS bytes with speech-ingest defaults.
#[inline]
pub fn decode(data: &[u8]) -> Result<DecodedAac> {
    decode_with(data, &DecodeOptions::speech())
}

/// Alias of [`decode`].
#[inline]
pub fn decode_bytes(data: &[u8]) -> Result<DecodedAac> {
    decode(data)
}

/// Read a file and [`decode`] it.
pub fn read(path: impl AsRef<std::path::Path>) -> Result<DecodedAac> {
    decode(&std::fs::read(path)?)
}

/// Read a file and [`decode_with`] it.
pub fn read_with(path: impl AsRef<std::path::Path>, opts: &DecodeOptions) -> Result<DecodedAac> {
    decode_with(&std::fs::read(path)?, opts)
}

/// Decode ADTS, M4A, or LATM/LOAS bytes under `opts`.
pub fn decode_with(data: &[u8], opts: &DecodeOptions) -> Result<DecodedAac> {
    if data.len() > MAX_INPUT_BYTES {
        return Err(AacError::too_long(
            data.len() as f64 / 40_000.0, // rough lower bound only for message
            opts.max_duration_secs,
        ));
    }
    if sniff_is_isobmff(data) {
        decode_m4a(data, opts)
    } else if sniff_is_adts(data) {
        decode_adts(data, opts)
    } else if sniff_is_latm(data) {
        decode_latm(data, opts)
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
        let (hdr, payload_off) = match AdtsHeader::parse(&data[pos..]) {
            Ok(v) => v,
            Err(_) => {
                if frames_decoded == 0 {
                    return Err(AacError::NotAac);
                }
                pos += 1;
                continue;
            }
        };
        let frame_len = usize::from(hdr.aac_frame_length);
        if frame_len < payload_off || pos + frame_len > data.len() {
            if frames_decoded == 0 {
                return Err(AacError::NotAac);
            }
            if pos + frame_len > data.len() {
                break;
            }
            pos += 1;
            continue;
        }
        let payload = &data[pos + payload_off..pos + frame_len];
        let frame = dec.decode_frame(&hdr, payload).map_err(|e| {
            AacError::decode(format!(
                "aac: decode failed at frame {frames_decoded}: {e:?}"
            ))
        })?;
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

    let out_rate = asc.output_sample_rate;
    if out_rate == 0 || out_rate > opts.max_sample_rate {
        return Err(AacError::sample_rate(out_rate, opts.max_sample_rate));
    }
    let max_samples = opts.max_frames(out_rate);
    if track.total_samples > max_samples as u64 {
        let observed_s = track.total_samples as f64 / out_rate.max(1) as f64;
        return Err(AacError::too_long(observed_s, opts.max_duration_secs));
    }

    let mut dec = StreamDecoder::new();
    dec.mix_down_mono = matches!(mode, ChannelMode::Mono);
    let reserve = (track.total_samples as usize).min(max_samples);

    if matches!(mode, ChannelMode::Mono) {
        let mut pcm = Vec::with_capacity(reserve);
        for (idx, &(off, len)) in track.frames.iter().enumerate() {
            let payload = sample_payload(data, off, len)?;
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
                Err(e) => {
                    return Err(AacError::decode(format!(
                        "aac: decode failed at frame {idx}: {e:?}"
                    )));
                }
            }
            if pcm.len() > max_samples {
                let observed_s = pcm.len() as f64 / out_rate.max(1) as f64;
                return Err(AacError::too_long(observed_s, opts.max_duration_secs));
            }
        }
        return after_edit(
            DecodedAac {
                sample_rate: out_rate,
                channels: vec![pcm],
            },
            track.skip_samples(out_rate),
        );
    }

    let mut out: Option<Out> = None;
    for (idx, &(off, len)) in track.frames.iter().enumerate() {
        let payload = sample_payload(data, off, len)?;
        let frame = match dec.decode_raw_data_block(
            asc.aot,
            asc.sampling_frequency_index,
            asc.sample_rate,
            asc.channel_configuration,
            1,
            payload,
        ) {
            Ok(frame) => frame,
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
    after_edit(
        out.map(Out::finish).ok_or(AacError::NotAac)?,
        track.skip_samples(out_rate),
    )
}

fn sample_payload(data: &[u8], off: u64, len: u32) -> Result<&[u8]> {
    let end = off
        .checked_add(u64::from(len))
        .ok_or_else(|| AacError::format("aac: sample range overflow"))? as usize;
    data.get(off as usize..end)
        .ok_or_else(|| AacError::format("aac: sample range past end of file"))
}

fn after_edit(mut decoded: DecodedAac, skip: usize) -> Result<DecodedAac> {
    for ch in &mut decoded.channels {
        let n = skip.min(ch.len());
        ch.drain(..n);
    }
    if decoded.channels.iter().all(Vec::is_empty) {
        return Err(AacError::NotAac);
    }
    Ok(decoded)
}

#[rustfmt::skip]
fn decode_latm(data: &[u8], opts: &DecodeOptions) -> Result<DecodedAac> {
    let mix = matches!(opts.channel_mode, ChannelMode::Mono);
    let (rate, planar) = crate::engine::latm::decode_loas_planar(data, mix)
        .map_err(|e| AacError::decode(format!("latm: {e:?}")))?;
    if rate == 0 || rate > opts.max_sample_rate {
        return Err(AacError::sample_rate(rate, opts.max_sample_rate));
    }
    let channels: Vec<Vec<f32>> = planar.into_iter().map(|ch| ch.into_iter().map(conv_f64).collect()).collect();
    let n = channels.first().map(Vec::len).unwrap_or(0);
    if n > opts.max_frames(rate) {
        return Err(AacError::too_long(n as f64 / f64::from(rate.max(1)), opts.max_duration_secs));
    }
    Ok(DecodedAac { sample_rate: rate, channels })
}
