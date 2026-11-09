//! Regressions found while auditing the iTunSMPB interoperability fix.
#![allow(clippy::unwrap_used)]
use super::test_support::{append_udta, m4a_tag_no_elst, metadata_box};
use super::{Delay, itunsmpb_in};
use crate::{DecodeOptions, ProbeDuration, ProbeTrim, Result, decode, probe};

fn tagged() -> Result<Vec<u8>> {
    m4a_tag_no_elst(
        &[0.1; 1124],
        &Delay {
            priming: 1024,
            remainder: 924,
            source: 1124,
        },
    )
}

fn field(file: &mut [u8], typ: &[u8; 4], offset: usize, value: u32) {
    let at = file.windows(4).position(|w| w == typ).unwrap() + offset;
    file[at..at + 4].copy_from_slice(&value.to_be_bytes());
}

#[test]
fn probe_uses_packet_count_even_when_mdhd_is_absent_or_stale() -> Result<()> {
    for duration in [0, 1124, 4096] {
        let mut file = tagged()?;
        field(&mut file, b"mdhd", 20, duration);
        let p = probe(&file)?;
        assert_eq!(p.duration, ProbeDuration::Exact { samples: 1124 });
        assert_eq!(
            p.trim,
            ProbeTrim::Exact {
                priming: 1024,
                remainder: 924
            }
        );
        assert_eq!(decode(&file)?.channels[0].len(), 1124);
    }
    Ok(())
}

#[test]
fn matching_mdhd_cannot_validate_a_tag_for_a_different_packet_count() -> Result<()> {
    let mut file = tagged()?;
    // Keep the sample table intact; only the tag and mdhd claim an extra AU.
    let at = file
        .windows(16)
        .position(|w| w == b"0000000000000464")
        .unwrap();
    file[at..at + 16].copy_from_slice(b"0000000000000864");
    field(&mut file, b"mdhd", 20, 4096);
    assert_eq!(decode(&file)?.priming, None);
    assert_eq!(probe(&file)?.trim, ProbeTrim::Unknown);
    Ok(())
}

#[test]
fn unrelated_metadata_cannot_supply_a_gapless_window() -> Result<()> {
    let file = tagged()?;
    let mut bare = file.clone();
    let at = bare.windows(8).position(|w| w == b"iTunSMPB").unwrap();
    bare[at..at + 8].copy_from_slice(b"notSMPB!");
    append_udta(
        &mut bare,
        &metadata_box(
            b"free",
            b"iTunSMPB 00000000 00000400 0000039C 0000000000000464",
        ),
    )?;
    assert_eq!(itunsmpb_in(&bare), None);
    assert_eq!(decode(&bare)?.priming, None);
    assert_eq!(probe(&bare)?.trim, ProbeTrim::Unknown);
    Ok(())
}

#[test]
fn a_large_moov_header_preserves_tagged_decode_and_probe() -> Result<()> {
    let mut file = tagged()?;
    let at = file.windows(4).position(|w| w == b"moov").unwrap() - 4;
    let old = u32::from_be_bytes(file[at..at + 4].try_into().unwrap());
    file[at..at + 4].copy_from_slice(&1u32.to_be_bytes());
    file.splice(at + 8..at + 8, (u64::from(old) + 8).to_be_bytes());
    // The encoder puts moov after mdat, so no chunk offset needs adjustment.
    assert_eq!(decode(&file)?.channels[0].len(), 1124);
    assert_eq!(
        probe(&file)?.duration,
        ProbeDuration::Exact { samples: 1124 }
    );
    let mut out = 0;
    crate::decode_seek_streaming(std::io::Cursor::new(&file), &DecodeOptions::audio(), |f| {
        out += f.samples;
        Ok(())
    })?;
    assert_eq!(out, 1124);
    Ok(())
}

fn id3(major: u8, id: &[u8; 4], text: &[u8]) -> Vec<u8> {
    let mut body = vec![0];
    if id == b"COMM" {
        body.extend_from_slice(b"eng");
    }
    body.extend_from_slice(b"iTunSMPB\0");
    body.extend_from_slice(text);
    let syncsafe = |n: usize| {
        [
            ((n >> 21) & 127) as u8,
            ((n >> 14) & 127) as u8,
            ((n >> 7) & 127) as u8,
            (n & 127) as u8,
        ]
    };
    let mut frame = id.to_vec();
    frame.extend_from_slice(&if major == 3 {
        (body.len() as u32).to_be_bytes()
    } else {
        syncsafe(body.len())
    });
    frame.extend_from_slice(&[0, 0]);
    frame.extend(body);
    let mut tag = vec![b'I', b'D', b'3', major, 0, 0];
    tag.extend_from_slice(&syncsafe(frame.len()));
    tag.extend(frame);
    tag
}

#[test]
fn independent_id3v23_and_v24_comments_and_text_trim_in_all_chunkings() -> Result<()> {
    let encoded = crate::encode_with(&[vec![0.1; 1124]], 48_000, &crate::EncodeOptions::adts())?;
    let raw = super::strip_id3(&encoded);
    let text = b" 00000000 00000400 0000039c 0000000000000464 00000000 00000000 00000000 00000000 00000000 00000000 00000000 00000000";
    for major in [3, 4] {
        for id in [b"COMM", b"TXXX"] {
            let mut file = id3(major, id, text);
            file.extend_from_slice(raw);
            let one = decode(&file)?;
            assert_eq!(one.channels[0].len(), 1124, "ID3v2.{major} {id:?}");
            assert_eq!(
                probe(&file)?.trim,
                ProbeTrim::Exact {
                    priming: 1024,
                    remainder: 924
                }
            );
            for chunk in [1, 9, 127] {
                let mut dec = crate::Decoder::new(DecodeOptions::audio());
                let mut out = Vec::new();
                for part in file.chunks(chunk) {
                    dec.feed(part, |f| {
                        out.extend_from_slice(f.planar[0]);
                        Ok(())
                    })?;
                }
                let info = dec.finish(|f| {
                    out.extend_from_slice(f.planar[0]);
                    Ok(())
                })?;
                assert_eq!(out, one.channels[0]);
                assert_eq!(info.priming, Some(1024));
            }
        }
    }
    Ok(())
}

#[test]
fn presentation_seek_uses_the_same_tag_window_as_one_shot() -> Result<()> {
    let file = tagged()?;
    let opts = DecodeOptions::audio();
    let one = crate::decode_with(&file, &opts)?;
    let mut source = crate::M4aSeek::open(std::io::Cursor::new(&file), opts)?;
    let mut all = Vec::new();
    assert!(source.bounded_presentation());
    let info = source.decode(|f| {
        all.extend_from_slice(f.planar[0]);
        Ok(())
    })?;
    assert_eq!(info.priming, Some(1024));
    assert_eq!(info.remainder, Some(924));
    assert!(source.presentation_complete());
    assert_eq!(all, one.channels[0]);
    source.seek(1)?;
    let mut tail = Vec::new();
    source.decode(|f| {
        tail.extend_from_slice(f.planar[0]);
        Ok(())
    })?;
    assert_eq!(tail.len(), 1123);
    assert_eq!(tail, one.channels[0][1..]);
    Ok(())
}

#[test]
fn a_960_core_uses_its_actual_packet_grid_for_tag_validation() -> Result<()> {
    let mut file = include_bytes!("../../lab/frame960/vectors/lc960-s48.m4a").to_vec();
    if let Some(at) = file.windows(4).position(|w| w == b"elst") {
        file[at..at + 4].copy_from_slice(b"free");
    }
    let opts = DecodeOptions::audio();
    let raw = crate::decode_with(&file, &opts)?;
    let samples = raw.channels[0].len();
    super::test_support::append_tag(
        &mut file,
        format!(" 00000000 00000011 00000013 {:016X}", samples - 36,).as_bytes(),
    )?;
    let one = crate::decode_with(&file, &opts)?;
    assert_eq!(
        probe(&file)?.duration,
        ProbeDuration::Exact {
            samples: (samples - 36) as u64
        }
    );
    assert_eq!(one.priming, Some(17));
    for (got, padded) in one.channels.iter().zip(raw.channels.iter()) {
        assert_eq!(got, &padded[17..samples - 19]);
    }
    Ok(())
}

#[test]
fn duplicate_items_and_wrong_namespace_or_data_type_are_ignored() -> Result<()> {
    let mut duplicate = tagged()?;
    super::test_support::append_tag(
        &mut duplicate,
        b" 00000000 00000400 0000039C 0000000000000464",
    )?;
    assert_eq!(itunsmpb_in(&duplicate), None);
    assert_eq!(probe(&duplicate)?.trim, ProbeTrim::Unknown);
    for (needle, replacement) in [
        (
            b"com.apple.iTunes".as_slice(),
            b"com.other.iTunes".as_slice(),
        ),
        (b"iTunSMPB".as_slice(), b"otherkey".as_slice()),
        (b"data\0\0\0\x01".as_slice(), b"data\0\0\0\x02".as_slice()),
    ] {
        let mut file = tagged()?;
        let at = file
            .windows(needle.len())
            .position(|w| w == needle)
            .unwrap();
        file[at..at + needle.len()].copy_from_slice(replacement);
        assert_eq!(itunsmpb_in(&file), None, "{needle:?}");
    }
    Ok(())
}

#[test]
fn an_item_cannot_borrow_text_from_a_sibling_box() -> Result<()> {
    let mut file = tagged()?;
    let at = file.windows(4).position(|w| w == b"data").unwrap();
    let size = u32::from_be_bytes(file[at - 4..at].try_into().unwrap());
    // Split the data item just before its text; the next box carries text
    // that the old 180-byte scan would have accepted.
    file[at - 4..at].copy_from_slice(&16u32.to_be_bytes());
    file[at + 12..at + 16].copy_from_slice(&(size - 16).to_be_bytes());
    file[at + 16..at + 20].copy_from_slice(b"free");
    assert_eq!(itunsmpb_in(&file), None);
    assert_eq!(decode(&file)?.priming, None);
    Ok(())
}

#[test]
fn missing_or_incomplete_size_table_does_not_prove_tag_counts() -> Result<()> {
    for offset in [0, 12] {
        let mut file = tagged()?;
        let at = file.windows(4).position(|w| w == b"stsz").unwrap();
        if offset == 0 {
            file[at..at + 4].copy_from_slice(b"free");
        } else {
            // Variable-size table declares an extra entry without room for it.
            file[at + 12..at + 16].copy_from_slice(&4u32.to_be_bytes());
        }
        assert_eq!(probe(&file)?.trim, ProbeTrim::Unknown);
    }
    Ok(())
}

#[test]
fn tag_only_seek_keeps_persistent_checkpoints_excluded() -> Result<()> {
    let file = super::test_support::m4a_tag_no_elst(
        &[0.1; 5000],
        &Delay {
            priming: 1024,
            source: 5000,
            remainder: 120,
        },
    )?;
    let mut source = crate::M4aSeek::open(std::io::Cursor::new(&file), DecodeOptions::speech())?;
    assert!(source.bounded_presentation());
    let _ = source.decode(|_| Err(crate::AacError::NotAac));
    assert_eq!(source.remaining_samples(), 5000 - 1024);
    assert!(source.checkpoint().is_none());
    Ok(())
}

#[test]
fn a_malformed_second_mp4_tag_is_still_ambiguous() -> Result<()> {
    for text in [
        b"bad".as_slice(),
        b" 00000000 00000400 0000039C 0000000000000464",
    ] {
        let mut file = tagged()?;
        super::test_support::append_tag(&mut file, text)?;
        assert_eq!(itunsmpb_in(&file), None);
        assert_eq!(probe(&file)?.trim, ProbeTrim::Unknown);
        assert_eq!(decode(&file)?.priming, None);
    }
    Ok(())
}

#[test]
fn duplicate_id3_tags_are_not_selected_by_frame_order() -> Result<()> {
    let encoded = crate::encode_with(&[vec![0.1; 1124]], 48_000, &crate::EncodeOptions::adts())?;
    let good = b" 00000000 00000400 0000039C 0000000000000464";
    for major in [3, 4] {
        for (first, second) in [
            (good.as_slice(), b"bad".as_slice()),
            (b"bad".as_slice(), good.as_slice()),
            (good.as_slice(), good.as_slice()),
        ] {
            let mut file = id3(major, b"COMM", first);
            file.extend_from_slice(&id3(major, b"TXXX", second)[10..]);
            let n = file.len() - 10;
            file[6..10].copy_from_slice(&[
                ((n >> 21) & 127) as u8,
                ((n >> 14) & 127) as u8,
                ((n >> 7) & 127) as u8,
                (n & 127) as u8,
            ]);
            file.extend_from_slice(super::strip_id3(&encoded));
            assert_eq!(decode(&file)?.priming, None, "v2.{major}");
            assert_eq!(probe(&file)?.trim, ProbeTrim::Unknown);
        }
    }
    Ok(())
}

#[test]
fn malformed_fields_cannot_hide_a_recognised_second_mp4_item() -> Result<()> {
    use super::test_support::{append_udta, metadata_box as bx};
    for extra in [
        bx(b"name", b"\0\0\0\0iTunSMPB"),
        bx(b"mean", b"\0\0\0\0com.other.iTunes"),
        bx(b"data", b"\0\0\0\x02\0\0\0\0bad"),
        bx(b"data", b""),
        b"\0\0\0\x40data".to_vec(),
        Vec::new(),
    ] {
        for invalid_first in [false, true] {
            let mut file = if invalid_first {
                // A single encoded file with no recognised gapless item yet.
                let mut file = tagged()?;
                let at = file.windows(8).position(|w| w == b"iTunSMPB").unwrap();
                file[at..at + 8].copy_from_slice(b"notSMPB!");
                file
            } else {
                tagged()?
            };
            let mut item = bx(b"mean", b"\0\0\0\0com.apple.iTunes");
            item.extend(bx(b"name", b"\0\0\0\0iTunSMPB"));
            item.extend(extra.clone());
            let mut meta = vec![0; 4];
            meta.extend(bx(b"ilst", &bx(b"----", &item)));
            append_udta(&mut file, &bx(b"meta", &meta))?;
            if invalid_first {
                super::test_support::append_tag(
                    &mut file,
                    b" 00000000 00000400 0000039C 0000000000000464",
                )?;
            }
            assert_eq!(itunsmpb_in(&file), None);
            assert_eq!(decode(&file)?.priming, None);
        }
    }
    Ok(())
}
