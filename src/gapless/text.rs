//! Shared standard iTunSMPB text codec. Never resynchronise inside a field.
use super::Delay;

pub(crate) fn parse_text(text: &[u8]) -> Option<Delay> {
    let end = text.iter().position(|&b| b == 0).unwrap_or(text.len());
    let mut fields = text[..end]
        .split(u8::is_ascii_whitespace)
        .filter(|field| !field.is_empty());
    if hex_field(fields.next()?, 8)? != 0 {
        return None;
    }
    Some(Delay {
        priming: hex_field(fields.next()?, 8)?,
        remainder: hex_field(fields.next()?, 8)?,
        source: hex_field(fields.next()?, 16)?,
    })
}

fn hex_field(bytes: &[u8], width: usize) -> Option<u64> {
    if bytes.len() != width || !bytes.iter().all(u8::is_ascii_hexdigit) {
        return None;
    }
    u64::from_str_radix(std::str::from_utf8(bytes).ok()?, 16).ok()
}

/// Twelve standard fields, with the reserved zero before the sample counts.
pub(crate) fn itunsmpb_value(d: &Delay) -> String {
    format!(
        " 00000000 {:08X} {:08X} {:016X} 00000000 00000000 00000000 00000000 00000000 00000000 00000000 00000000",
        d.priming, d.remainder, d.source
    )
}
