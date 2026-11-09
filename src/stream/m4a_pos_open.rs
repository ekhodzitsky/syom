//! Open a presentation source and validate its edit/tag window.
use super::super::m4a::map_m4a_parse_err;
use super::*;
use crate::isomp4::{self, TrackBoxes, load_track_boxes};

impl<R: Read + Seek> M4aSeek<R> {
    /// Parse `moov` and sit at presentation 0, after edit-list or tag trimming.
    pub fn open(mut reader: R, opts: DecodeOptions) -> Result<Self> {
        opts.validate()?;
        let boxes = load_track_boxes(&mut reader, &opts.memory)?;
        let gap = match &boxes {
            TrackBoxes::Flat { moov, .. } | TrackBoxes::Frag { moov, .. } => {
                crate::gapless::itunsmpb_in(moov)
            }
        };
        let tagged = match &boxes {
            TrackBoxes::Flat { moov, .. } | TrackBoxes::Frag { moov, .. } => {
                moov.windows(b"iTunSMPB".len()).any(|w| w == b"iTunSMPB")
            }
        };
        let track = match boxes {
            TrackBoxes::Flat { moov, file_len } => {
                isomp4::parse_aac_track_with_len(&moov, file_len, &opts.memory)
                    .map_err(map_m4a_parse_err)?
            }
            TrackBoxes::Frag {
                moov, moofs, mdats, ..
            } => isomp4::frag::track_from_parts(&moov, &moofs, &mdats, &opts.memory)?,
        };
        let (asc, _) = AudioSpecificConfig::parse(&track.asc).map_err(AacError::from)?;
        let out_rate = asc.output_sample_rate;
        if out_rate == 0 || out_rate > opts.max_sample_rate {
            return Err(AacError::sample_rate(out_rate, opts.max_sample_rate));
        }
        let spf = crate::gapless::output_frame_samples(&asc).max(1);
        let (skip, play_opt) = track.edit_window(out_rate)?;
        let mut skip = skip as u64;
        let coded = spf.saturating_mul(track.frames.len() as u64);
        let mut play = match play_opt {
            Some(p) => p as u64,
            None => coded.saturating_sub(skip),
        };
        let mut applied = None;
        if !track.has_elst
            && let Some(d) = gap.filter(|d| d.matches_frames(track.frames.len() as u64, spf))
            && usize::try_from(d.priming).is_ok()
            && usize::try_from(d.source).is_ok()
        {
            skip = d.priming;
            play = d.source;
            applied = Some(d);
        }
        let mono = matches!(opts.channel_mode, ChannelMode::Mono);
        let n_ch = if mono {
            1
        } else {
            u32::from(asc.channel_configuration).max(1)
        };
        opts.memory.check_output(n_ch, track.total_samples)?;
        // LD overlap is one 512-sample frame. LC mono/stereo needs two
        // (measured); HE/PS replays from the start because of in-band SBR.
        let preroll = if asc.aot == crate::engine::asc::AOT_LD {
            1
        } else {
            preroll_aus(asc.sbr_present)
        };
        let checkpoint_gapless_safe = track.has_elst || !tagged;
        let checkpoint_source_tag = checkpoint::source_tag(&track);
        let mut s = Self {
            reader,
            track,
            asc,
            opts,
            dec: StreamDecoder::new(),
            scratch: Vec::new(),
            next_frame: 0,
            discard_left: skip as usize,
            play_left: play,
            pos: 0,
            skip,
            play,
            samples_per_au: spf,
            preroll,
            locked: None,
            mono,
            noise_free_prefix: 0,
            exact_mode: false,
            exact_failed: false,
            source_identity: std::sync::Arc::new(()),
            checkpoint_gapless_safe,
            gapless: applied,
            checkpoint_clean: true,
            checkpoint_restored: false,
            checkpoint_source_tag,
            #[cfg(test)]
            last_seek_aus: 0,
        };
        s.reset_codec();
        Ok(s)
    }
}
