//! `AudioSpecificConfig` — ISO/IEC 14496-3:2005/Amd 2 Table 1.13.
//!
//! LC core (AOT 2). Explicit AOT 5/29: core rate, then extension rate, then
//! inner LC. Implicit HE: `0x2b7` + `sbrPresentFlag` + extension rate, then
//! optional `0x548` PS.

use super::adts::ADTS_SAMPLE_RATES_HZ;
use super::bits::{BitReader, BitWriter};
use super::error::{Error, Result};

const AOT_LC: u8 = 2;
const AOT_SBR: u8 = 5;
const AOT_PS: u8 = 29;

/// Serialize a bare LC ASC: AOT 2 + rate index + channels + 3 zero GA bits
/// (`frameLengthFlag` / `dependsOnCoreCoder` / `extensionFlag`). Two bytes
/// for any table rate (explicit 24-bit rates are never emitted).
#[must_use]
pub fn write_lc(fs_index: u8, channel_configuration: u8) -> Vec<u8> {
    let mut w = BitWriter::new();
    w.write(u32::from(AOT_LC), 5);
    w.write(u32::from(fs_index), 4);
    w.write(u32::from(channel_configuration), 4);
    w.write(0, 3);
    w.finish()
}

/// Parsed `AudioSpecificConfig` (LC core, optional SBR/PS).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioSpecificConfig {
    /// Inner GA `audioObjectType` (2 = LC) after unwrapping SBR/PS.
    pub aot: u8,
    /// Table 1.18 index for the **core** rate (SWB tables).
    pub sampling_frequency_index: u8,
    /// Core sample rate in Hz.
    pub sample_rate: u32,
    /// Output rate (2× core when SBR is signalled).
    pub output_sample_rate: u32,
    /// Table 1.19 channel configuration.
    pub channel_configuration: u8,
    /// HE-AAC SBR present (outer AOT 5/29 or trailing 0x2b7).
    pub sbr_present: bool,
    /// HE-AAC v2 parametric stereo.
    pub ps_present: bool,
}

impl AudioSpecificConfig {
    /// Parse an ASC byte blob. Returns the config and bits consumed.
    pub fn parse(data: &[u8]) -> Result<(Self, u64)> {
        let mut reader = BitReader::new(data);
        Self::parse_from_reader(&mut reader)
    }

    /// Parse ASC from an already-positioned reader.
    pub fn parse_from_reader(reader: &mut BitReader<'_>) -> Result<(Self, u64)> {
        let start = reader.bit_position();
        let asc = Self::parse_bits(reader)?;
        Ok((asc, reader.bit_position().saturating_sub(start)))
    }

    fn parse_bits(reader: &mut BitReader<'_>) -> Result<Self> {
        // ISO/IEC 14496-3:2005/Amd 2 Table 1.13 + Amd 9 implicit 0x2b7/0x548.
        // Explicit AOT 5/29: first rate is the LC core, then channelConfiguration,
        // then extensionSamplingFrequencyIndex (SBR output), then inner AOT.
        let outer_aot = read_aot(reader)?;
        let sampling_frequency_index = reader.read(4)? as u8;
        let sample_rate = resolve_rate(sampling_frequency_index, reader)?;
        let channel_configuration = reader.read(4)? as u8;
        let mut sbr_present = outer_aot == AOT_SBR || outer_aot == AOT_PS;
        let mut ps_present = outer_aot == AOT_PS;
        let mut output_sample_rate = sample_rate;
        let aot = if sbr_present {
            let ext_idx = reader.read(4)? as u8;
            output_sample_rate = resolve_rate(ext_idx, reader)?;
            let inner = read_aot(reader)?;
            if inner != AOT_LC {
                return Err(Error::UnsupportedAot(inner));
            }
            parse_ga(reader)?;
            inner
        } else {
            if outer_aot != AOT_LC {
                return Err(Error::UnsupportedAot(outer_aot));
            }
            parse_ga(reader)?;
            parse_implicit_sbr(
                reader,
                &mut sbr_present,
                &mut ps_present,
                &mut output_sample_rate,
            )?;
            outer_aot
        };
        if sbr_present {
            check_sbr_rates(sample_rate, output_sample_rate)?;
        }
        Ok(AudioSpecificConfig {
            aot,
            sampling_frequency_index,
            sample_rate,
            output_sample_rate,
            channel_configuration,
            sbr_present,
            ps_present,
        })
    }
}

/// Implicit HE: `syncExtensionType` 0x2b7, AOT 5/29, `sbrPresentFlag`,
/// `extensionSamplingFrequencyIndex`, optional 0x548 `psPresentFlag`.
/// LC padding `56 e5 00` peeks as 0x2b7 + AOT 5 with flag 0 — leave LC.
fn parse_implicit_sbr(
    reader: &mut BitReader<'_>,
    sbr_present: &mut bool,
    ps_present: &mut bool,
    output_sample_rate: &mut u32,
) -> Result<()> {
    if reader.bits_remaining() < 16 || reader.peek_u32(11).ok() != Some(0x2b7) {
        return Ok(());
    }
    let _ = reader.read(11)?;
    let ext = read_aot(reader)?;
    if ext != AOT_SBR && ext != AOT_PS {
        return Ok(());
    }
    if !reader.read_bit()? {
        return Ok(());
    }
    *sbr_present = true;
    *ps_present = *ps_present || ext == AOT_PS;
    let ext_idx = reader.read(4)? as u8;
    *output_sample_rate = resolve_rate(ext_idx, reader)?;
    if reader.bits_remaining() >= 12 && reader.peek_u32(11).ok() == Some(0x548) {
        let _ = reader.read(11)?;
        *ps_present = reader.read_bit()?;
    }
    Ok(())
}

/// Dual-rate SBR is 2× core; downsampled SBR is 1×. Other ratios are not v1.
fn check_sbr_rates(core: u32, output: u32) -> Result<()> {
    if output == core || output == core.saturating_mul(2) {
        Ok(())
    } else {
        Err(Error::Format("SBR extension rate must be 1x or 2x core"))
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

fn parse_ga(reader: &mut BitReader<'_>) -> Result<()> {
    let frame_length_flag = reader.read_bit()?;
    if frame_length_flag {
        return Err(Error::UnsupportedFrameLength);
    }
    let depends_on_core_coder = reader.read_bit()?;
    if depends_on_core_coder {
        let _core_coder_delay = reader.read(14)?;
    }
    let extension_flag = reader.read_bit()?;
    // channel_configuration == 0 is allowed: the PCE is applied in-band.
    if extension_flag {
        let extension_flag3 = reader.read_bit()?;
        if extension_flag3 {
            return Err(Error::Format("ASC extensionFlag3 is Media"));
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "asc_tests.rs"]
mod asc_tests;
