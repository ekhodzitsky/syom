//! Implementation-bound, caller-authenticated persistent AAC-LC state.
use super::*;
use crate::engine::{filterbank::Filterbank, pns::Lcg};

const MAGIC: &[u8; 8] = b"SYOMLC01";
const IMPLEMENTATION: &str = concat!(env!("CARGO_PKG_VERSION"), "/lc-checkpoint-1");
const IMPLEMENTATION_BYTES: usize = 32;
const HEADER: usize = MAGIC.len() + IMPLEMENTATION_BYTES + 20 * 8;
const STATE: usize = HEADER + 3 * 8;
const OVERLAP: usize = STATE + 4;
const SIZE: usize = OVERLAP + 2 * Filterbank::CHECKPOINT_BYTES;

// Structural mismatch detection only, not authentication of payload/source.
pub(super) fn source_tag(track: &AacTrack) -> u64 {
    let mut tag = 0xcbf2_9ce4_8422_2325u64;
    let mut bytes = |data: &[u8]| {
        for &byte in data {
            tag = (tag ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3);
        }
    };
    bytes(&(track.asc.len() as u64).to_le_bytes());
    bytes(&track.asc);
    for (off, len) in &track.frames {
        bytes(&off.to_le_bytes());
        bytes(&len.to_le_bytes());
    }
    for number in [
        track.edit_start,
        track.edit_duration,
        u64::from(track.movie_timescale),
        u64::from(track.media_timescale),
        track.media_duration,
    ] {
        bytes(&number.to_le_bytes());
    }
    tag
}

impl<R: Read + Seek> M4aSeek<R> {
    /// Persistent state format version. A format match alone does not establish
    /// compatibility: package implementation, source and options must also match.
    pub const CHECKPOINT_FORMAT_VERSION: u16 = 1;
    /// Exact encoded length for version 1. No allocation is based on blob counts.
    pub const CHECKPOINT_BYTES: usize = SIZE;

    /// Whether this ASC/options/presentation can use persistent LC state.
    /// Qualified scope: 1024-sample AAC-LC, mono output from mono/stereo, at
    /// 8/16/22.05/24/32/44.1/48 kHz. HE/PS, PCE, profile prediction, exact mode,
    /// failed exact state and ambiguous tag-only gapless trim are excluded.
    /// In-band unsupported history is additionally checked after each AU.
    #[must_use]
    pub fn supports_checkpoint(&self) -> bool {
        !self.exact_mode
            && !self.exact_failed
            && self.checkpoint_clean
            && self.checkpoint_gapless_safe
            && self.asc.frame_samples == 1024
            && self.asc.aot == 2
            && self.asc.pce.is_none()
            && !self.asc.sbr_present
            && !self.asc.ps_present
            && self.asc.sample_rate == self.asc.output_sample_rate
            && matches!(
                self.asc.output_sample_rate,
                8000 | 16000 | 22050 | 24000 | 32000 | 44100 | 48000
            )
            && matches!(self.asc.channel_configuration, 1 | 2)
            && self.mono
    }

    /// Check static scope plus the observed DSP history. Once a read/codec fault,
    /// unsafe frame shape, PCE/coupling or HE history is seen, this source handle
    /// stays ineligible even after an ordinary seek. A callback interruption
    /// after a valid frame is allowed. A fresh source is needed after a fault.
    #[must_use]
    pub fn checkpoint_compatible(&self) -> bool {
        self.supports_checkpoint() && self.checkpoint_history_safe()
    }

    fn checkpoint_history_safe(&self) -> bool {
        !self.dec.sbr_active
            && self.dec.pce.is_none()
            && self.dec.fb_pool.is_empty()
            && self.dec.cces.is_empty()
            && self.dec.tools.aot == 2
            && !self.dec.tools.multichannel
    }

    pub(super) fn remember_checkpoint_frame(&mut self, rate: u32, samples: usize) {
        self.checkpoint_clean &= self.checkpoint_history_safe()
            && rate == self.asc.output_sample_rate
            && samples == 1024;
    }

    fn checkpoint_header(&self) -> Option<Vec<u8>> {
        let mut out = Vec::with_capacity(HEADER);
        out.extend_from_slice(MAGIC);
        let mut implementation = [0u8; IMPLEMENTATION_BYTES];
        implementation
            .get_mut(..IMPLEMENTATION.len())?
            .copy_from_slice(IMPLEMENTATION.as_bytes());
        out.extend_from_slice(&implementation);
        let memory = &self.opts.memory;
        for number in [
            u64::from(self.asc.output_sample_rate),
            u64::from(self.asc.channel_configuration),
            self.skip,
            self.play,
            self.track.frames.len() as u64,
            self.checkpoint_source_tag,
            self.track.total_samples,
            u64::from(self.track.has_elst) | (u64::from(self.track.edit_open_end) << 1),
            self.opts.max_duration_secs.to_bits(),
            u64::from(self.opts.max_sample_rate),
            u64::from(self.opts.max_decode_sample_rate),
            memory.max_input_bytes,
            memory.max_buffered_input_bytes,
            memory.max_output_bytes,
            u64::from(memory.max_channels),
            memory.max_metadata_bytes,
            u64::from(memory.max_index_entries),
            u64::from(memory.max_box_depth),
            memory.max_workspace_bytes,
            u64::from(memory.max_declared_au_bytes),
        ] {
            out.extend_from_slice(&number.to_le_bytes());
        }
        Some(out)
    }

    /// Save bounded native DSP state at an interior, fully consumed AU boundary.
    /// Returns `None` at initial/EOF positions, during priming, after partial-AU
    /// trimming, or outside the qualified scope. A successful capture is exactly
    /// [`Self::CHECKPOINT_BYTES`] bytes: implementation/options/index geometry,
    /// next AU, native presentation position/remainder, PNS RNG and two LC
    /// filterbank overlaps/window shapes. Transform and PCM scratch is omitted.
    ///
    /// This is opaque implementation state, **not an authenticated source ID**.
    /// Callers must authenticate the blob and bind it to immutable source bytes,
    /// the exact syom implementation/build (including patches/CPU policy), and
    /// options before restoring. The index tag only detects structural mismatch;
    /// identical indexes with different AAC payloads are not distinguished.
    /// No I/O or payload hashing is performed here. This is independent of the
    /// replay-based [`Self::save_exact_position`] contract.
    pub fn checkpoint(&self) -> Option<Vec<u8>> {
        if !self.checkpoint_compatible()
            || self.pos == 0
            || self.pos >= self.play
            || self.next_frame >= self.track.frames.len()
            || self.discard_left != 0
            || (self.next_frame as u64).checked_mul(1024) != self.skip.checked_add(self.pos)
        {
            return None;
        }
        let mut out = self.checkpoint_header()?;
        out.reserve(SIZE - out.len());
        for value in [self.next_frame as u64, self.pos, self.play_left] {
            out.extend_from_slice(&value.to_le_bytes());
        }
        out.extend_from_slice(&self.dec.rng.checkpoint().to_le_bytes());
        self.dec.fb_l.write_checkpoint(&mut out)?;
        self.dec.fb_r.write_checkpoint(&mut out)?;
        Some(out)
    }

    /// Restore a caller-authenticated checkpoint without replay or media reads.
    /// A compatible source must already be open with identical decode options.
    /// Format/version, structural geometry, counts, window shapes and finite
    /// overlap values are validated before replacing DSP/position state. Every
    /// rejected import leaves the previous reader/decoder state unchanged.
    ///
    /// Valid restore uses one temporary fixed-size `StreamDecoder` allocation
    /// plus a 200-byte header; blob-controlled lengths never allocate. The first
    /// resumed AU rejects nonfinite output before delivering a callback. Finite
    /// but forged DSP values cannot be proven faithful to earlier media, so the
    /// authentication/binding requirement in [`Self::checkpoint`] is mandatory.
    /// Package-version or format changes are incompatible; no cross-build,
    /// cross-architecture or consumer-specific format compatibility is promised.
    pub fn restore_checkpoint(&mut self, bytes: &[u8]) -> Result<()> {
        let invalid = || AacError::format("invalid or incompatible AAC-LC checkpoint");
        if !self.checkpoint_compatible() || bytes.len() != SIZE {
            return Err(invalid());
        }
        let header = self.checkpoint_header().ok_or_else(invalid)?;
        if bytes[..HEADER] != header {
            return Err(invalid());
        }
        let number = |at: usize| -> Result<u64> {
            Ok(u64::from_le_bytes(
                bytes[at..at + 8].try_into().map_err(|_| invalid())?,
            ))
        };
        let next = number(HEADER)?;
        let pos = number(HEADER + 8)?;
        let left = number(HEADER + 16)?;
        if next == 0
            || next >= self.track.frames.len() as u64
            || pos == 0
            || pos >= self.play
            || left != self.play - pos
            || next.checked_mul(1024) != self.skip.checked_add(pos)
        {
            return Err(invalid());
        }
        let next = usize::try_from(next).map_err(|_| invalid())?;
        let mut candidate = StreamDecoder::new();
        candidate.mix_down_mono = true;
        candidate.set_frame_samples(1024);
        candidate.tools.reset();
        candidate.rng = Lcg::from_checkpoint(u32::from_le_bytes(
            bytes[STATE..OVERLAP].try_into().map_err(|_| invalid())?,
        ));
        let right = OVERLAP + Filterbank::CHECKPOINT_BYTES;
        candidate
            .fb_l
            .restore_checkpoint(&bytes[OVERLAP..right])
            .ok_or_else(invalid)?;
        candidate
            .fb_r
            .restore_checkpoint(&bytes[right..])
            .ok_or_else(invalid)?;
        self.dec = candidate;
        self.next_frame = next;
        self.pos = pos;
        self.discard_left = 0;
        self.play_left = left;
        self.scratch.clear();
        self.locked = Some((self.asc.output_sample_rate, 1));
        self.noise_free_prefix = 0;
        self.checkpoint_restored = true;
        Ok(())
    }
}

#[cfg(test)]
#[path = "m4a_pos_checkpoint_tests.rs"]
mod tests;
