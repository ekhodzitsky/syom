//! Movie-clock durations are quantized; media-time offsets must stay exact.

use crate::error::{AacError, Result};

pub(crate) fn available_samples(
    duration: u64,
    start: u64,
    rate: u32,
    timescale: u32,
) -> Result<Option<u64>> {
    if duration == 0 {
        return Ok(None);
    }
    duration
        .checked_sub(start)
        .map(|left| rescale_duration(left, rate, timescale, "mdhd duration after media_time"))
        .transpose()
}

pub(crate) fn duration_samples(
    duration: u64,
    rate: u32,
    timescale: u32,
    available: Option<u64>,
) -> Result<u64> {
    let samples = rescale_duration(duration, rate, timescale, "elst duration")?;
    Ok(match available {
        Some(n) if n < samples && rounded_duration_tail(duration, rate, timescale, n) => n,
        _ => samples,
    })
}

/// Compare the original rational duration, including its fractional sample,
/// against the delivered/available sample count. Both caps and indexed EOF
/// must use this rule: strictly less than one movie tick AND one millisecond.
pub(super) fn rounded_duration_tail(
    duration: u64,
    rate: u32,
    timescale: u32,
    samples: u64,
) -> bool {
    if rate == 0 || timescale == 0 {
        return false;
    }
    let requested = u128::from(duration) * u128::from(rate);
    let available = u128::from(samples) * u128::from(timescale);
    requested.checked_sub(available).is_some_and(|excess| {
        excess > 0
            && excess * u128::from(timescale.max(1000)) < u128::from(rate) * u128::from(timescale)
    })
}

/// Duration clocks can be quantized. Use a wide intermediate so a valid
/// quotient is not rejected merely because its unscaled product exceeds u64.
fn rescale_duration(value: u64, rate: u32, timescale: u32, what: &str) -> Result<u64> {
    if rate == 0 || timescale == 0 {
        return Err(AacError::format(format!("isomp4: {what} timescale is 0")));
    }
    let samples = u128::from(value) * u128::from(rate) / u128::from(timescale);
    u64::try_from(samples).map_err(|_| AacError::format(format!("isomp4: {what} overflow")))
}
