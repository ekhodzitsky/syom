//! `AudioSpecificConfig` — ISO/IEC 14496-3 §1.6.2.1 Table 1.15.
//!
//! v1 accepts AOT 2 (AAC-LC) only. AOT 5 / 29 (HE-AAC SBR / PS) is Media.

use super::adts::ADTS_SAMPLE_RATES_HZ;
use super::bits::BitReader;
use super::error::{Error, Result};

const AOT_LC: u8 = 2;
const AOT_SBR: u8 = 5;
const AOT_PS: u8 = 29;

/// Parsed LC `AudioSpecificConfig`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioSpecificConfig {
    /// Inner GA `audioObjectType` (2 = LC).
    pub aot: u8,
    /// Table 1.18 index (0..=12, or 0xf if explicit rate).
    pub sampling_frequency_index: u8,
    /// Resolved core sample rate in Hz.
    pub sample_rate: u32,
    /// Table 1.19 channel configuration.
    pub channel_configuration: u8,
}

impl AudioSpecificConfig {
    /// Parse an ASC byte blob. Returns the config and bits consumed.
    pub fn parse(data: &[u8]) -> Result<(Self, u64)> {
        let mut reader = BitReader::new(data);
        let asc = Self::parse_bits(&mut reader)?;
        Ok((asc, reader.bit_position()))
    }

    fn parse_bits(reader: &mut BitReader<'_>) -> Result<Self> {
        let outer_aot = read_aot(reader)?;
        if outer_aot == AOT_SBR || outer_aot == AOT_PS {
            return Err(Error::UnsupportedAot(outer_aot));
        }
        let sampling_frequency_index = reader.read(4)? as u8;
        let sample_rate = resolve_rate(sampling_frequency_index, reader)?;
        let channel_configuration = reader.read(4)? as u8;

        if outer_aot != AOT_LC {
            return Err(Error::UnsupportedAot(outer_aot));
        }
        parse_ga(reader, channel_configuration)?;
        // Trailing §1.6.5 implicit-SBR bits (syncExtensionType 0x2b7) are
        // ignored: v1 decodes the LC core. Outer AOT 5 / 29 stays Media.

        Ok(AudioSpecificConfig {
            aot: outer_aot,
            sampling_frequency_index,
            sample_rate,
            channel_configuration,
        })
    }
}

fn read_aot(reader: &mut BitReader<'_>) -> Result<u8> {
    let aot = reader.read(5)? as u8;
    if aot == 31 {
        Ok(32 + reader.read(6)? as u8)
    } else {
        Ok(aot)
    }
}

fn resolve_rate(index: u8, reader: &mut BitReader<'_>) -> Result<u32> {
    if index == 0x0f {
        let rate = reader.read(24)?;
        if rate == 0 {
            return Err(Error::UnsupportedSampleRateIndex(index));
        }
        return Ok(rate);
    }
    ADTS_SAMPLE_RATES_HZ
        .get(index as usize)
        .copied()
        .ok_or(Error::UnsupportedSampleRateIndex(index))
}

fn parse_ga(reader: &mut BitReader<'_>, channel_configuration: u8) -> Result<()> {
    let frame_length_flag = reader.read_bit()?;
    if frame_length_flag {
        return Err(Error::UnsupportedFrameLength);
    }
    let depends_on_core_coder = reader.read_bit()?;
    if depends_on_core_coder {
        let _core_coder_delay = reader.read(14)?;
    }
    let extension_flag = reader.read_bit()?;
    if channel_configuration == 0 {
        return Err(Error::Format("PCE channelConfiguration is Media in v1"));
    }
    if extension_flag {
        let extension_flag3 = reader.read_bit()?;
        if extension_flag3 {
            return Err(Error::Format("ASC extensionFlag3 is Media"));
        }
    }
    Ok(())
}
