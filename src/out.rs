//! PCM accumulation for ADTS / M4A / split-channel decode.

use crate::engine::adts::AdtsHeader;
use crate::engine::decode::{DecodedFrame, INV_S16, StreamDecoder};
use crate::engine::error::Error as EngineError;
use crate::error::{AacError, Result};
use crate::options::DecodeOptions;

pub(crate) struct Out {
    sample_rate: u32,
    channels: usize,
    tracks: Vec<Vec<f32>>,
    samples_decoded: usize,
    max_samples: usize,
}

impl Out {
    pub(crate) fn new(first: &DecodedFrame, opts: &DecodeOptions) -> Result<Self> {
        let sample_rate = first.sample_rate;
        if sample_rate == 0 || sample_rate > opts.max_sample_rate {
            return Err(AacError::sample_rate(sample_rate, opts.max_sample_rate));
        }
        let max_samples = opts.max_frames(sample_rate);
        let tracks = (0..first.channels).map(|_| Vec::new()).collect();
        Ok(Out {
            sample_rate,
            channels: first.channels,
            tracks,
            samples_decoded: 0,
            max_samples,
        })
    }

    pub(crate) fn reserve_per_track(&mut self, frames_hint: usize) {
        let n = self.tracks.len().max(1);
        let per = frames_hint / n;
        for t in &mut self.tracks {
            t.reserve(per);
        }
    }

    pub(crate) fn push(&mut self, frame: &DecodedFrame, opts: &DecodeOptions) -> Result<()> {
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
        for (ch, dst) in self.tracks.iter_mut().enumerate() {
            dst.reserve(per_ch);
            for &v in &frame.planar[ch] {
                dst.push(v * INV_S16);
            }
        }
        self.samples_decoded += per_ch;
        if self.samples_decoded > self.max_samples {
            let observed_s = self.samples_decoded as f64 / self.sample_rate.max(1) as f64;
            return Err(AacError::too_long(observed_s, opts.max_duration_secs));
        }
        Ok(())
    }

    pub(crate) fn finish(self) -> (u32, Vec<Vec<f32>>) {
        (self.sample_rate, self.tracks)
    }
}

pub(crate) fn push_frame(
    out: &mut Option<Out>,
    frame: &DecodedFrame,
    opts: &DecodeOptions,
) -> Result<()> {
    match out {
        None => {
            if frame.channels == 0 {
                return Ok(());
            }
            let mut o = Out::new(frame, opts)?;
            o.push(frame, opts)?;
            *out = Some(o);
        }
        Some(o) => o.push(frame, opts)?,
    }
    Ok(())
}

fn frame_err(frames_decoded: u64, e: EngineError) -> AacError {
    AacError::decode(format!(
        "aac: decode failed at frame {frames_decoded}: {e:?}"
    ))
}

pub(crate) fn push_adts_mono(
    out: &mut Option<Out>,
    dec: &mut StreamDecoder,
    hdr: &AdtsHeader,
    payload: &[u8],
    opts: &DecodeOptions,
    frames_decoded: u64,
    hint: usize,
) -> Result<()> {
    let aot = hdr.audio_object_type();
    let fs = hdr.sampling_frequency_index;
    let sr = hdr.sample_rate();
    let ch = hdr.channel_configuration;
    let nraw = hdr.number_of_raw_data_blocks_in_frame;
    match out {
        None => {
            let mut pcm = Vec::with_capacity(hint * 8);
            let rate = dec
                .decode_raw_mono_f32(aot, fs, sr, ch, nraw, payload, &mut pcm)
                .map_err(|e| frame_err(frames_decoded, e))?;
            if rate == 0 || rate > opts.max_sample_rate {
                return Err(AacError::sample_rate(rate, opts.max_sample_rate));
            }
            let n = pcm.len();
            let max_samples = opts.max_frames(rate);
            if n > max_samples {
                let observed_s = n as f64 / f64::from(rate.max(1));
                return Err(AacError::too_long(observed_s, opts.max_duration_secs));
            }
            *out = Some(Out {
                sample_rate: rate,
                channels: 1,
                tracks: vec![pcm],
                samples_decoded: n,
                max_samples,
            });
        }
        Some(o) => {
            let rate = dec
                .decode_raw_mono_f32(aot, fs, sr, ch, nraw, payload, &mut o.tracks[0])
                .map_err(|e| frame_err(frames_decoded, e))?;
            if rate != o.sample_rate {
                return Err(AacError::decode(format!(
                    "aac: sample rate changed mid-stream ({}Hz → {rate}Hz)",
                    o.sample_rate
                )));
            }
            o.samples_decoded = o.tracks[0].len();
            if o.samples_decoded > o.max_samples {
                let observed_s = o.samples_decoded as f64 / o.sample_rate.max(1) as f64;
                return Err(AacError::too_long(observed_s, opts.max_duration_secs));
            }
        }
    }
    Ok(())
}
