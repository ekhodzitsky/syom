//! Read only Apple's freeform gapless item, bounded by its MP4 boxes.
use super::BoxIter;
use crate::gapless::{Delay, Tag, Tags, parse_text};

pub(crate) fn itunsmpb_in(data: &[u8]) -> Option<Delay> {
    let mut found = Tags::default();
    for child in children(data) {
        let (typ, moov) = child?;
        if &typ != b"moov" {
            continue;
        }
        for child in children(moov) {
            let (typ, udta) = child?;
            if &typ == b"udta" {
                read_udta(udta, &mut found)?;
            }
        }
    }
    found.delay()
}

fn read_udta(udta: &[u8], found: &mut Tags) -> Option<()> {
    for child in children(udta) {
        let (typ, meta) = child?;
        if &typ != b"meta" {
            continue;
        }
        if meta.get(..4)? != [0; 4] {
            continue;
        }
        for child in children(&meta[4..]) {
            let (typ, ilst) = child?;
            if &typ != b"ilst" {
                continue;
            }
            for child in children(ilst) {
                let (typ, item) = child?;
                if &typ == b"----"
                    && let Some(tag) = read_item(item)
                {
                    found.add(tag);
                }
            }
        }
    }
    Some(())
}

fn read_item(item: &[u8]) -> Option<Tag> {
    let (mut mean_seen, mut name_seen, mut data_seen) = (false, false, false);
    let (mut apple_mean, mut smpb_name) = (false, false);
    let mut value = None;
    let mut usable = true;
    for child in children(item) {
        let Some((typ, body)) = child else {
            usable = false;
            break;
        };
        match &typ {
            b"mean" => {
                usable &= !mean_seen && body.get(..4) == Some(&[0; 4]);
                mean_seen = true;
                apple_mean |= body.get(4..) == Some(b"com.apple.iTunes".as_slice());
            }
            b"name" => {
                usable &= !name_seen && body.get(..4) == Some(&[0; 4]);
                name_seen = true;
                smpb_name |= body.get(4..) == Some(b"iTunSMPB".as_slice());
            }
            b"data" => {
                usable &= !data_seen && body.get(..4) == Some(&[0, 0, 0, 1]);
                data_seen = true;
                value = body.get(8..);
            }
            _ => {}
        }
    }
    if !apple_mean || !smpb_name {
        return None;
    }
    // Identity survives bad values/duplicate fields so an invalid second
    // recognised item cannot disappear from the shared ambiguity policy.
    Some(Tag {
        delay: value.filter(|_| usable).and_then(parse_text),
    })
}

fn children(data: &[u8]) -> impl Iterator<Item = Option<([u8; 4], &[u8])>> {
    BoxIter::new(data, 0, data.len()).map(|item| {
        let (hdr, typ) = item.ok()?;
        Some((typ, &data[hdr.content_start..hdr.content_end]))
    })
}
