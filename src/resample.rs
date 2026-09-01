//! Downmix and resample to product PCM: 16 kHz s16le mono.

use crate::error::{SyomError, media};

pub const SAMPLE_RATE: u32 = 16_000;

/// Incremental converter. Keeps a few source-rate mono samples, not the
/// whole file.
pub(crate) struct Pcm16Mono {
    channels: usize,
    in_rate: u32,
    pending: Vec<f32>,
    origin: u64,
    out_i: u64,
    out: Vec<u8>,
}

impl Pcm16Mono {
    pub(crate) fn new(channels: usize, rate: u32) -> Result<Self, SyomError> {
        if channels == 0 || rate == 0 {
            return Err(media("zero channels or rate"));
        }
        Ok(Self {
            channels,
            in_rate: rate,
            pending: Vec::new(),
            origin: 0,
            out_i: 0,
            out: Vec::new(),
        })
    }

    pub(crate) fn push(&mut self, interleaved: &[f32]) -> Result<(), SyomError> {
        let frames = interleaved.len() / self.channels;
        let nch = self.channels as f32;
        for i in 0..frames {
            let base = i.saturating_mul(self.channels);
            let mut s = 0.0f32;
            for c in 0..self.channels {
                let idx = base.saturating_add(c);
                s += interleaved
                    .get(idx)
                    .copied()
                    .ok_or_else(|| media("short frame"))?;
            }
            self.pending.push(s / nch);
        }
        self.emit(false)
    }

    pub(crate) fn finish(mut self) -> Result<Vec<u8>, SyomError> {
        self.emit(true)?;
        if self.out.is_empty() {
            return Err(media("empty decode"));
        }
        Ok(self.out)
    }

    fn emit(&mut self, last: bool) -> Result<(), SyomError> {
        if self.in_rate == SAMPLE_RATE {
            return self.flush_same_rate();
        }
        let in_rate = f64::from(self.in_rate);
        let out_rate = f64::from(SAMPLE_RATE);
        loop {
            let src = self.out_i as f64 * in_rate / out_rate;
            let i0 = src.floor() as u64;
            if i0 < self.origin {
                return Err(media("resample origin"));
            }
            let local0 =
                usize::try_from(i0.saturating_sub(self.origin)).map_err(|_| media("index"))?;
            if local0 >= self.pending.len() {
                break;
            }
            if !last && local0.saturating_add(1) >= self.pending.len() {
                break;
            }
            let a = self
                .pending
                .get(local0)
                .copied()
                .ok_or_else(|| media("resample"))?;
            let b = self
                .pending
                .get(local0.saturating_add(1))
                .copied()
                .unwrap_or(a);
            let frac = (src - src.floor()) as f32;
            pack_sample(&mut self.out, a + (b - a) * frac);
            self.out_i = self.out_i.saturating_add(1);
        }
        self.drop_consumed()
    }

    fn flush_same_rate(&mut self) -> Result<(), SyomError> {
        let n = self.pending.len() as u64;
        for s in self.pending.drain(..) {
            pack_sample(&mut self.out, s);
        }
        self.origin = self.origin.saturating_add(n);
        self.out_i = self.out_i.saturating_add(n);
        Ok(())
    }

    fn drop_consumed(&mut self) -> Result<(), SyomError> {
        let src = self.out_i as f64 * f64::from(self.in_rate) / f64::from(SAMPLE_RATE);
        let i0 = src.floor() as u64;
        if i0 <= self.origin {
            return Ok(());
        }
        let drop64 = i0.saturating_sub(self.origin);
        let drop = usize::try_from(drop64).map_err(|_| media("drop"))?;
        let drop = drop.min(self.pending.len());
        self.pending.drain(..drop);
        self.origin = self.origin.saturating_add(drop as u64);
        Ok(())
    }
}

fn pack_sample(out: &mut Vec<u8>, s: f32) {
    let v = (s.clamp(-1.0, 1.0) * 32767.0).round() as i16;
    out.extend_from_slice(&v.to_le_bytes());
}

pub(crate) fn to_pcm16_mono_16k(
    interleaved: &[f32],
    channels: usize,
    rate: u32,
) -> Result<Vec<u8>, SyomError> {
    let mut conv = Pcm16Mono::new(channels, rate)?;
    conv.push(interleaved)?;
    conv.finish()
}

#[cfg(test)]
#[path = "resample_tests.rs"]
mod resample_tests;
