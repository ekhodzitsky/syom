//! TASK-124 unit tests: tfhd/tfdt/trun resolution across the three base
//! addressing modes, the defaults chain, the mdat fence, and the typed
//! rejection matrix (lab/fmp4/REPORT.md §2). Synthetic boxes; the golden
//! fixtures exercise the same code end to end (`stream_fmp4_tests`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::{Fmp4Init, FragResolver};
use crate::budgets::MemoryBudgets;
use crate::error::{AacError, MalformedKind, Result, UnsupportedFeature};

fn bx(typ: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut v = ((payload.len() + 8) as u32).to_be_bytes().to_vec();
    v.extend_from_slice(typ);
    v.extend_from_slice(payload);
    v
}

fn full(typ: &[u8; 4], version: u8, flags: u32, payload: &[u8]) -> Vec<u8> {
    let mut b = vec![
        version,
        (flags >> 16) as u8,
        (flags >> 8) as u8,
        flags as u8,
    ];
    b.extend_from_slice(payload);
    bx(typ, &b)
}

fn mfhd(seq: u32) -> Vec<u8> {
    full(b"mfhd", 0, 0, &seq.to_be_bytes())
}

/// `extra` holds the tfhd flag-selected fields in spec order (after track_ID).
fn tfhd(flags: u32, track: u32, extra: &[u8]) -> Vec<u8> {
    let mut p = track.to_be_bytes().to_vec();
    p.extend_from_slice(extra);
    full(b"tfhd", 0, flags, &p)
}

fn tfdt(v: u32) -> Vec<u8> {
    full(b"tfdt", 0, 0, &v.to_be_bytes())
}

fn trun(version: u8, flags: u32, count: u32, data_offset: Option<i32>, records: &[u8]) -> Vec<u8> {
    let mut p = count.to_be_bytes().to_vec();
    if flags & 0x1 != 0 {
        p.extend_from_slice(&data_offset.expect("data_offset").to_be_bytes());
    }
    if flags & 0x4 != 0 {
        p.extend_from_slice(&0u32.to_be_bytes());
    }
    p.extend_from_slice(records);
    full(b"trun", version, flags, &p)
}

fn moof(children: &[&[u8]]) -> Vec<u8> {
    bx(b"moof", &children.concat())
}

fn init() -> Fmp4Init {
    Fmp4Init {
        asc: vec![0x11, 0x90],
        track_id: 1,
        movie_timescale: 1000,
        media_timescale: 48_000,
        edit_start: 0,
        edit_duration: 0,
        has_elst: false,
        trex_duration: 1024,
        trex_size: 0,
    }
}

const MDAT: &[(u64, u64)] = &[(2000, 3000)];

/// Resolve one synthetic moof placed at `pos` (mdat payload at 2000..3000).
fn resolve(res: &mut FragResolver, moof_box: &[u8], pos: u64, init: &Fmp4Init) -> Result<()> {
    let content = &moof_box[8..];
    res.add_moof(
        content,
        pos,
        pos + moof_box.len() as u64,
        init,
        MDAT,
        &MemoryBudgets::default(),
    )
}

#[test]
fn base_modes_resolve_the_same_ranges() -> Result<()> {
    // explicit base-data-offset
    let mut r = FragResolver::default();
    let m = moof(&[
        &mfhd(1),
        &bx(
            b"traf",
            &[
                tfhd(0x1, 1, &2000u64.to_be_bytes()),
                trun(
                    0,
                    0x200,
                    2,
                    None,
                    &[100u32.to_be_bytes(), 150u32.to_be_bytes()].concat(),
                ),
            ]
            .concat(),
        ),
    ]);
    resolve(&mut r, &m, 1000, &init())?;
    assert_eq!(r.frames, [(2000, 100), (2100, 150)]);
    assert_eq!(r.total, 2048, "durations from trex default");

    // default-base-is-moof: trun data_offset is relative to the moof
    let mut r = FragResolver::default();
    let m = moof(&[
        &mfhd(1),
        &bx(
            b"traf",
            &[
                tfhd(0x20000, 1, &[]),
                trun(
                    0,
                    0x201,
                    2,
                    Some(1000),
                    &[100u32.to_be_bytes(), 150u32.to_be_bytes()].concat(),
                ),
            ]
            .concat(),
        ),
    ]);
    resolve(&mut r, &m, 1000, &init())?;
    assert_eq!(r.frames, [(2000, 100), (2100, 150)]);

    // implicit base: end of the previous fragment's data
    let m2 = moof(&[
        &mfhd(2),
        &bx(
            b"traf",
            &[
                tfhd(0, 1, &[]),
                trun(0, 0x200, 1, None, &90u32.to_be_bytes()),
            ]
            .concat(),
        ),
    ]);
    resolve(&mut r, &m2, 1500, &init())?;
    assert_eq!(r.frames[2], (2250, 90), "implicit base = end of fragment 1");
    Ok(())
}

#[test]
fn tfdt_and_defaults_chain() -> Result<()> {
    let mut r = FragResolver::default();
    let m = moof(&[
        &mfhd(1),
        &bx(
            b"traf",
            &[
                tfhd(
                    0x20018,
                    1,
                    &[2048u32.to_be_bytes(), 99u32.to_be_bytes()].concat(),
                ),
                tfdt(4096),
                // no per-sample fields: tfhd default size 99, duration 2048
                trun(0, 0x1, 2, Some(1000), &[]),
                // trun fields beat the tfhd defaults
                trun(
                    1,
                    0x300,
                    1,
                    None,
                    &[512u32.to_be_bytes(), 77u32.to_be_bytes()].concat(),
                ),
            ]
            .concat(),
        ),
    ]);
    resolve(&mut r, &m, 1000, &init())?;
    assert_eq!(r.frames, [(2000, 99), (2099, 99), (2198, 77)]);
    assert_eq!(r.total, 2048 * 2 + 512);
    assert_eq!(r.dts, 4096 + 2048 * 2 + 512);
    Ok(())
}

#[test]
fn mfhd_sequence_must_be_continuous() -> Result<()> {
    let ok = moof(&[
        &mfhd(7),
        &bx(
            b"traf",
            &[
                tfhd(0x20000, 1, &[]),
                trun(0, 0x201, 1, Some(1000), &50u32.to_be_bytes()),
            ]
            .concat(),
        ),
    ]);
    let mut r = FragResolver::default();
    resolve(&mut r, &ok, 1000, &init())?; // any first sequence
    let gap = moof(&[
        &mfhd(9),
        &bx(
            b"traf",
            &[
                tfhd(0x20000, 1, &[]),
                trun(0, 0x201, 1, Some(1000), &50u32.to_be_bytes()),
            ]
            .concat(),
        ),
    ]);
    assert!(matches!(
        resolve(&mut r, &gap, 1500, &init()),
        Err(AacError::Malformed(MalformedKind::Syntax))
    ));
    Ok(())
}

fn unsupported(moof_box: &[u8]) -> UnsupportedFeature {
    let mut r = FragResolver::default();
    match resolve(&mut r, moof_box, 1000, &init()) {
        Err(AacError::Unsupported(f)) => f,
        other => panic!("expected typed Unsupported, got {other:?}"),
    }
}

#[test]
fn rejections_are_typed_never_silent() {
    let one = trun(0, 0x201, 1, Some(1000), &50u32.to_be_bytes());
    // traf for a foreign track
    let m = moof(&[
        &mfhd(1),
        &bx(b"traf", &[tfhd(0x20000, 2, &[]), one.clone()].concat()),
    ]);
    assert!(matches!(
        unsupported(&m),
        UnsupportedFeature::FragmentedMp4(_)
    ));
    // two trafs in one moof
    let t1 = bx(b"traf", &[tfhd(0x20000, 1, &[]), one.clone()].concat());
    let m = moof(&[&mfhd(1), &t1, &t1]);
    assert!(matches!(
        unsupported(&m),
        UnsupportedFeature::FragmentedMp4(_)
    ));
    // sample-description switch via tfhd
    let m = moof(&[
        &mfhd(1),
        &bx(
            b"traf",
            &[tfhd(0x20002, 1, &2u32.to_be_bytes()), one.clone()].concat(),
        ),
    ]);
    assert!(matches!(
        unsupported(&m),
        UnsupportedFeature::FragmentedMp4(_)
    ));
    // nonzero composition-time offset (trun v1 signed cto)
    let cto = trun(
        1,
        0xA01,
        1,
        Some(1000),
        &[50u32.to_be_bytes(), 4u32.to_be_bytes()].concat(),
    );
    let m = moof(&[
        &mfhd(1),
        &bx(b"traf", &[tfhd(0x20000, 1, &[]), cto].concat()),
    ]);
    assert!(matches!(
        unsupported(&m),
        UnsupportedFeature::FragmentedMp4(_)
    ));
    // traf-level CENC auxiliary boxes
    for enc in [
        &bx(b"saiz", &[0u8; 9]),
        &bx(b"saio", &[0u8; 8]),
        &bx(b"senc", &[0u8; 4]),
    ] {
        let m = moof(&[
            &mfhd(1),
            &bx(
                b"traf",
                &[tfhd(0x20000, 1, &[]), enc.clone(), one.clone()].concat(),
            ),
        ]);
        assert!(matches!(
            unsupported(&m),
            UnsupportedFeature::FragmentedMp4(_)
        ));
    }
}

#[test]
fn zero_cto_and_trun_v1_are_accepted() -> Result<()> {
    let cto = trun(
        1,
        0xA01,
        1,
        Some(1000),
        &[50u32.to_be_bytes(), 0u32.to_be_bytes()].concat(),
    );
    let m = moof(&[
        &mfhd(1),
        &bx(b"traf", &[tfhd(0x20000, 1, &[]), cto].concat()),
    ]);
    let mut r = FragResolver::default();
    resolve(&mut r, &m, 1000, &init())?;
    assert_eq!(r.frames, [(2000, 50)]);
    Ok(())
}

#[test]
fn resolved_ranges_are_fenced_inside_the_following_mdat() {
    // past the mdat end
    let bad = moof(&[
        &mfhd(1),
        &bx(
            b"traf",
            &[
                tfhd(0x20000, 1, &[]),
                trun(0, 0x201, 1, Some(1000), &1500u32.to_be_bytes()),
            ]
            .concat(),
        ),
    ]);
    let mut r = FragResolver::default();
    assert!(matches!(
        resolve(&mut r, &bad, 1000, &init()),
        Err(AacError::Malformed(MalformedKind::Syntax))
    ));
    // no mdat after the moof at all
    let mut r = FragResolver::default();
    let content = &bad[8..];
    assert!(matches!(
        r.add_moof(
            content,
            1000,
            1000 + bad.len() as u64,
            &init(),
            &[],
            &MemoryBudgets::default()
        ),
        Err(AacError::Malformed(MalformedKind::Syntax))
    ));
}

#[test]
fn zero_duration_needs_duration_is_empty() -> Result<()> {
    // trex duration 0, no tfhd/trun duration: only legal with the tfhd
    // duration-is-empty flag.
    let tr = trun(0, 0x201, 1, Some(1000), &50u32.to_be_bytes());
    let m = moof(&[
        &mfhd(1),
        &bx(b"traf", &[tfhd(0x20000, 1, &[]), tr.clone()].concat()),
    ]);
    let mut r = FragResolver::default();
    let mut ini = init();
    ini.trex_duration = 0;
    assert!(matches!(
        resolve(&mut r, &m, 1000, &ini),
        Err(AacError::Malformed(MalformedKind::Syntax))
    ));
    let m = moof(&[
        &mfhd(1),
        &bx(b"traf", &[tfhd(0x30000, 1, &[]), tr].concat()),
    ]);
    let mut r = FragResolver::default();
    resolve(&mut r, &m, 1000, &ini)?;
    assert_eq!(r.frames, [(2000, 50)]);
    assert_eq!(r.total, 0);
    Ok(())
}

#[test]
fn zero_size_sample_outside_the_mdat_is_malformed() -> Result<()> {
    // trex sample_size is 0. An empty sample inside the payload is a
    // legal range; the same empty sample before the mdat is not.
    let inside = moof(&[
        &mfhd(1),
        &bx(
            b"traf",
            &[tfhd(0, 1, &[]), trun(0, 0x1, 1, Some(1000), &[])].concat(),
        ),
    ]);
    let mut r = FragResolver::default();
    resolve(&mut r, &inside, 1000, &init())?;
    assert_eq!(r.frames, [(2000, 0)]);

    let outside = moof(&[
        &mfhd(1),
        &bx(
            b"traf",
            &[tfhd(0, 1, &[]), trun(0, 0x1, 1, Some(0), &[])].concat(),
        ),
    ]);
    let mut r = FragResolver::default();
    assert!(matches!(
        resolve(&mut r, &outside, 1000, &init()),
        Err(AacError::Malformed(MalformedKind::Syntax))
    ));
    Ok(())
}

#[test]
fn structural_holes_are_malformed() {
    // moof without traf
    let m = moof(&[&mfhd(1)]);
    let mut r = FragResolver::default();
    assert!(matches!(
        resolve(&mut r, &m, 1000, &init()),
        Err(AacError::Malformed(MalformedKind::Syntax))
    ));
    // traf without tfhd
    let m = moof(&[&mfhd(1), &bx(b"traf", &trun(0, 0, 0, None, &[]))]);
    let mut r = FragResolver::default();
    assert!(matches!(
        resolve(&mut r, &m, 1000, &init()),
        Err(AacError::Malformed(MalformedKind::Syntax))
    ));
    // trun version > 1
    let m = moof(&[
        &mfhd(1),
        &bx(
            b"traf",
            &[
                tfhd(0x20000, 1, &[]),
                trun(2, 0x201, 1, Some(1000), &50u32.to_be_bytes()),
            ]
            .concat(),
        ),
    ]);
    let mut r = FragResolver::default();
    assert!(matches!(
        resolve(&mut r, &m, 1000, &init()),
        Err(AacError::Malformed(MalformedKind::Syntax))
    ));
    // empty index at finish
    assert!(FragResolver::default().finish(init()).is_err());
}

#[test]
fn parse_init_reads_the_lc_fixture_moov() -> Result<()> {
    let data = include_bytes!("goldens/fmp4_lc.mp4");
    let (hdr, typ) = super::read_box(data, 0)?.expect("ftyp");
    assert_eq!(&typ, b"ftyp");
    let (hdr2, typ2) = super::read_box(data, hdr.content_end)?.expect("moov");
    assert_eq!(&typ2, b"moov");
    let init = super::parse_init(
        &data[hdr.content_end..hdr2.content_end],
        &MemoryBudgets::default(),
    )?;
    assert!(init.asc.starts_with(&[0x11, 0x90]), "LC 48 kHz stereo ASC");
    assert_eq!(init.media_timescale, 48_000);
    assert!(!init.has_elst, "mov-muxer fMP4 carries no elst");
    Ok(())
}
