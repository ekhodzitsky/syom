//! M4A timing metadata, including count-checked iTunSMPB fallback.

use super::{Probe, ProbeContainer, ProbeDuration, ProbeTrim, from_asc};
use crate::budgets::MemoryBudgets;
use crate::engine::asc::AudioSpecificConfig;
use crate::error::{AacError, Result};
use crate::isomp4::{convert_units, duration_samples, find_moov_slice, parse_aac_meta};

pub(super) fn probe_m4a(data: &[u8], mem: &MemoryBudgets) -> Result<Probe> {
    if crate::isomp4::frag::is_fragmented(data) {
        return probe_fmp4(data, mem);
    }
    let moov = find_moov_slice(data, mem)?;
    let track = parse_aac_meta(moov, mem)?;
    let (asc, _) = AudioSpecificConfig::parse(&track.asc).map_err(AacError::from)?;
    let tagged = if track.has_elst {
        None
    } else {
        crate::gapless::itunsmpb_in(moov).filter(|d| {
            track
                .coded_frames
                .is_some_and(|n| d.matches_frames(n, crate::gapless::output_frame_samples(&asc)))
        })
    };
    let (duration, trim) = if let Some(d) = tagged {
        (
            ProbeDuration::Exact { samples: d.source },
            ProbeTrim::Exact {
                priming: d.priming,
                remainder: d.remainder,
            },
        )
    } else {
        m4a_timing(&track, asc.output_sample_rate)?
    };
    from_asc(ProbeContainer::M4a, &asc, mem, duration, trim)
}

/// Bounded fMP4 (TASK-124): the init `moov` carries ASC, rates and the
/// optional priming edit; the coded length lives in the fragments, so
/// duration stays `Unknown` unless the edit caps it.
fn probe_fmp4(data: &[u8], mem: &MemoryBudgets) -> Result<Probe> {
    let moov = find_moov_slice(data, mem)?;
    let init = crate::isomp4::frag::parse_init(moov, mem)?;
    let (asc, _) = AudioSpecificConfig::parse(&init.asc).map_err(AacError::from)?;
    let out_rate = asc.output_sample_rate;
    let (duration, trim) = if init.has_elst {
        let priming = convert_units(
            init.edit_start,
            out_rate,
            init.media_timescale,
            "elst media_time",
        )?;
        let duration = if init.edit_duration == 0 {
            ProbeDuration::Unknown // DASH init: written before the duration
        } else {
            ProbeDuration::Exact {
                samples: duration_samples(
                    init.edit_duration,
                    out_rate,
                    init.movie_timescale,
                    None,
                )?,
            }
        };
        (
            duration,
            ProbeTrim::Exact {
                priming,
                remainder: 0,
            },
        )
    } else {
        (ProbeDuration::Unknown, ProbeTrim::Unknown)
    };
    from_asc(ProbeContainer::M4a, &asc, mem, duration, trim)
}

fn m4a_timing(
    track: &crate::isomp4::AacMeta,
    output_rate: u32,
) -> Result<(ProbeDuration, ProbeTrim)> {
    if track.has_elst {
        let priming = convert_units(
            track.edit_start,
            output_rate,
            track.media_timescale,
            "elst media_time",
        )?;
        let available = crate::isomp4::available_samples(
            track.media_duration,
            track.edit_start,
            output_rate,
            track.media_timescale,
        )?;
        let samples = duration_samples(
            track.edit_duration,
            output_rate,
            track.movie_timescale,
            available,
        )?;
        let remainder = available.map_or(0, |left| left.saturating_sub(samples));
        Ok((
            ProbeDuration::Exact { samples },
            ProbeTrim::Exact { priming, remainder },
        ))
    } else if track.media_duration != 0 && track.media_timescale != 0 {
        let samples = convert_units(
            track.media_duration,
            output_rate,
            track.media_timescale,
            "mdhd duration",
        )?;
        Ok((ProbeDuration::Exact { samples }, ProbeTrim::Unknown))
    } else {
        Ok((ProbeDuration::Unknown, ProbeTrim::Unknown))
    }
}
