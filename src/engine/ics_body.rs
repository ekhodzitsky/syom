//! `individual_channel_stream()` — ISO/IEC 14496-3 Table 4.44 (LC, no gain control).

use super::bits::BitReader;
use super::error::{Error, Result};
use super::ics::IcsInfo;
use super::section::SectionData;
use super::sf::{self, ScaleFactors};
use super::spectrum::{self, PulseData};

/// One channel's decoded spectrum (window-major) plus the tools it needs.
#[derive(Debug, Clone)]
pub struct ChannelBody {
    /// ICS of this channel (shared with the pair when `common_window`).
    pub ics: IcsInfo,
    /// Section codebooks.
    pub sections: SectionData,
    /// Absolute scalefactors / IS / PNS energies.
    pub sf: ScaleFactors,
    /// Inverse-quantised, scalefactor-applied spectrum (PNS bands still 0).
    pub spec: Vec<f64>,
    /// Optional TNS payload.
    pub tns: Option<super::tns::TnsData>,
}

/// Parse one `individual_channel_stream(common_window, scale_flag=0)`.
pub fn parse_ics(
    br: &mut BitReader<'_>,
    fs_index: u8,
    _aot: u8,
    common: Option<&IcsInfo>,
) -> Result<ChannelBody> {
    let global_gain = br.read(8)? as u8;
    let ics = if let Some(shared) = common {
        shared.clone()
    } else {
        IcsInfo::parse(br, fs_index, false)?
    };
    let sections = SectionData::parse(br, &ics)?;
    let sf = sf::parse(br, &ics, &sections.sfb_cb, global_gain)?;
    let pulse_present = br.read_bit()?;
    let pulse = if pulse_present {
        if ics.window_sequence.is_eight_short() {
            return Err(Error::Format("pulse_data on eight_short"));
        }
        Some(PulseData::parse(br)?)
    } else {
        None
    };
    let tns_present = br.read_bit()?;
    let tns = if tns_present {
        Some(super::tns::TnsData::parse(br, &ics)?)
    } else {
        None
    };
    let gain_present = br.read_bit()?;
    if gain_present {
        return Err(Error::Format("gain_control_data is SSR, not LC"));
    }
    let mut quant = spectrum::parse_quant(br, &ics, &sections, fs_index)?;
    if let Some(p) = pulse.as_ref() {
        spectrum::apply_pulse(&mut quant, fs_index, p)?;
    }
    let spec = spectrum::rescale(&quant, &ics, &sections, &sf, fs_index)?;
    Ok(ChannelBody {
        ics,
        sections,
        sf,
        spec,
        tns,
    })
}
