//! TASK-126: fMP4 timeline qualification against the committed
//! ffprobe-derived oracle `src/goldens/fmp4_timeline.txt` (minted offline
//! by `lab/fmp4/gen_timeline_oracle.py`, ffprobe 7.0.2 pinned in
//! lab/fmp4/PIN.md). Each packet's media dts and cts are pinned (cts
//! follows dts: in-envelope audio has no composition offset), including
//! tfdt-less accumulation. A `present` line records the ffprobe edit
//! shift: mov-muxer fMP4 is untrimmed, the DASH init `elst` trims priming.
//! mfhd sequence numbers are the oracle's; a gap is `Malformed`.
//! Product tests replay the committed oracle and never spawn ffprobe.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::{FragResolver, parse_init, read_box};
use crate::budgets::MemoryBudgets;
use crate::error::Result;
use crate::{AacError, DecodeOptions, MalformedKind, StreamInfo, decode_streaming, decode_with};

const ORACLE: &str = include_str!("goldens/fmp4_timeline.txt");

#[derive(Clone, Copy)]
struct Pkt {
    pos: u64,
    size: u32,
    dts: u64,
    cts: u64,
}

struct Frag {
    seq: u32,
    tfdt: Option<u64>,
    first: u64,
    end: u64,
    packets: Vec<Pkt>,
}

struct Present {
    elst: Option<u64>,
    seg_dur: Option<u64>,
    ffprobe_first_pts: i64,
    priming: Option<u64>,
}

struct OracleFixture {
    name: String,
    timescale: u32,
    present: Present,
    frags: Vec<Frag>,
}

fn parse_opt_u64(tok: &str) -> Option<u64> {
    (tok != "none").then(|| tok.parse().expect("u64"))
}

fn parse_oracle() -> Vec<OracleFixture> {
    let mut out: Vec<OracleFixture> = Vec::new();
    for line in ORACLE.lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let w: Vec<&str> = line.split(' ').collect();
        match w.as_slice() {
            ["fixture", name, "timescale", ts] => out.push(OracleFixture {
                name: (*name).to_string(),
                timescale: ts.parse().expect("timescale"),
                present: Present {
                    elst: None,
                    seg_dur: None,
                    ffprobe_first_pts: 0,
                    priming: None,
                },
                frags: Vec::new(),
            }),
            [
                "present",
                "elst",
                elst,
                "seg_dur",
                seg,
                "ffprobe_first_pts",
                pts,
                "priming",
                priming,
            ] => {
                let f = out.last_mut().expect("present without fixture");
                f.present = Present {
                    elst: parse_opt_u64(elst),
                    seg_dur: parse_opt_u64(seg),
                    ffprobe_first_pts: pts.parse().expect("ffprobe pts"),
                    priming: parse_opt_u64(priming),
                };
            }
            [
                "frag",
                "seq",
                seq,
                "tfdt",
                tfdt,
                "first",
                first,
                "end",
                end,
                "packets",
                _n,
            ] => {
                let f = out.last_mut().expect("frag without fixture");
                f.frags.push(Frag {
                    seq: seq.parse().expect("seq"),
                    tfdt: parse_opt_u64(tfdt),
                    first: first.parse().expect("first"),
                    end: end.parse().expect("end"),
                    packets: Vec::new(),
                });
            }
            ["pkt", pos, size, dts, cts] => {
                let f = out
                    .last_mut()
                    .and_then(|f| f.frags.last_mut())
                    .expect("pkt");
                f.packets.push(Pkt {
                    pos: pos.parse().expect("pos"),
                    size: size.parse().expect("size"),
                    dts: dts.parse().expect("dts"),
                    cts: cts.parse().expect("cts"),
                });
            }
            other => panic!("oracle line: {other:?}"),
        }
    }
    out
}

fn fixture_bytes(name: &str) -> Vec<u8> {
    match name {
        "fmp4_lc.mp4" => include_bytes!("goldens/fmp4_lc.mp4").to_vec(),
        "fmp4_lc_notfdt.mp4" => include_bytes!("goldens/fmp4_lc_notfdt.mp4").to_vec(),
        "fmp4_he.mp4" => include_bytes!("goldens/fmp4_he.mp4").to_vec(),
        "fmp4_bbb" => [
            include_bytes!("goldens/fmp4_bbb_init.m4a").as_slice(),
            include_bytes!("goldens/fmp4_bbb_seg1.m4a").as_slice(),
        ]
        .concat(),
        "fmp4_dash" => [
            include_bytes!("goldens/fmp4_dash_init.m4s").as_slice(),
            include_bytes!("goldens/fmp4_dash_seg1.m4s").as_slice(),
            include_bytes!("goldens/fmp4_dash_seg2.m4s").as_slice(),
        ]
        .concat(),
        other => panic!("oracle fixture not wired: {other}"),
    }
}

fn be_u32(data: &[u8], at: usize) -> u32 {
    u32::from_be_bytes(data[at..at + 4].try_into().expect("u32"))
}

/// `(byte offset of the sequence field, sequence)` for each moof, in order.
fn mfhd_sequences(data: &[u8]) -> Result<Vec<(usize, u32)>> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    while let Some((hdr, typ)) = read_box(data, pos)? {
        if &typ == b"moof" {
            let mut c = hdr.content_start;
            while c < hdr.content_end {
                let Some((ch, ct)) = read_box(data, c)? else {
                    break;
                };
                if &ct == b"mfhd" {
                    let at = ch.content_start + 4;
                    out.push((at, be_u32(data, at)));
                    break;
                }
                c = ch.content_end;
            }
        }
        pos = hdr.content_end;
    }
    Ok(out)
}

/// Resolve `data` fragment by fragment and pin per-sample media dts/cts,
/// mfhd sequence continuity and the init edit against the ffprobe oracle.
fn check_timeline(f: &OracleFixture) -> Result<()> {
    let data = fixture_bytes(&f.name);
    let mem = MemoryBudgets::default();
    let mut moov = None;
    let mut moofs = Vec::new();
    let mut mdats = Vec::new();
    let mut pos = 0usize;
    while let Some((hdr, typ)) = read_box(&data, pos)? {
        match &typ {
            b"moov" => moov = Some((pos, hdr.content_end)),
            b"moof" => moofs.push((pos as u64, hdr.content_end as u64, hdr)),
            b"mdat" => mdats.push((hdr.content_start as u64, hdr.content_end as u64)),
            _ => {}
        }
        pos = hdr.content_end;
    }
    let (ms, me) = moov.expect("init moov");
    let init = parse_init(&data[ms..me], &mem)?;
    assert_eq!(init.media_timescale, f.timescale, "{} timescale", f.name);
    assert_eq!(moofs.len(), f.frags.len(), "{} fragment count", f.name);
    let seqs = mfhd_sequences(&data)?;
    assert_eq!(seqs.len(), f.frags.len(), "{} mfhd count", f.name);
    match f.present.elst {
        Some(mt) => {
            assert!(init.has_elst, "{} elst", f.name);
            assert_eq!(init.edit_start, mt, "{} elst media_time", f.name);
            assert_eq!(
                init.edit_duration,
                f.present.seg_dur.expect("seg_dur"),
                "{} elst duration",
                f.name
            );
        }
        None => {
            assert!(!init.has_elst, "{} has no elst", f.name);
            assert_eq!(init.edit_start, 0, "{} untrimmed", f.name);
        }
    }
    // ffprobe's first pts is the edit shift: 0 untrimmed, -priming when the
    // init elst skips encoder delay.
    match f.present.priming {
        None => assert_eq!(f.present.ffprobe_first_pts, 0, "{} ffprobe start", f.name),
        Some(p) => {
            let shift = i64::try_from(p).expect("priming fits i64");
            assert_eq!(
                f.present.ffprobe_first_pts, -shift,
                "{} ffprobe priming shift",
                f.name
            );
        }
    }

    let mut res = FragResolver::default();
    res.trace_dts = true;
    let mut done = 0usize;
    for (i, &(mpos, mend, ref hdr)) in moofs.iter().enumerate() {
        let want = &f.frags[i];
        assert_eq!(seqs[i].1, want.seq, "{} frag {i} mfhd seq", f.name);
        if i > 0 {
            assert_eq!(
                want.seq,
                f.frags[i - 1].seq.wrapping_add(1),
                "{} mfhd continuous",
                f.name
            );
        }
        match want.tfdt {
            Some(t) => assert_eq!(t, want.first, "{} frag {i} tfdt vs ffprobe", f.name),
            // Without tfdt the accumulated dts must reproduce the timeline.
            None => assert_eq!(res.dts, want.first, "{} frag {i} accumulated dts", f.name),
        }
        res.add_moof(
            &data[hdr.content_start..hdr.content_end],
            mpos,
            mend,
            &init,
            &mdats,
            &mem,
        )?;
        assert_eq!(res.dts, want.end, "{} frag {i} end dts", f.name);
        let got = &res.frames[done..];
        let traced = &res.sample_dts[done..];
        assert_eq!(got.len(), want.packets.len(), "{} frag {i} packets", f.name);
        assert_eq!(traced.len(), want.packets.len(), "{} frag {i} dts", f.name);
        for (k, ((g, d), p)) in got.iter().zip(traced).zip(&want.packets).enumerate() {
            assert_eq!(
                *g,
                (p.pos, p.size),
                "{} frag {i} sample {k} (pos, size)",
                f.name
            );
            assert_eq!(*d, p.dts, "{} frag {i} sample {k} dts", f.name);
            assert_eq!(p.cts, p.dts, "{} frag {i} sample {k} cts", f.name);
        }
        if let Some(t) = want.tfdt {
            assert_eq!(traced[0], t, "{} frag {i} tfdt applied", f.name);
        }
        done = res.frames.len();
    }
    let track = res.finish(init)?;
    assert_eq!(
        track.total_samples,
        f.frags.last().expect("frags").end,
        "{} total media duration",
        f.name
    );
    Ok(())
}

fn collect(data: &[u8]) -> Result<(StreamInfo, Vec<Vec<f32>>)> {
    let mut planes: Vec<Vec<f32>> = Vec::new();
    let info = decode_streaming(data, &DecodeOptions::audio(), |f| {
        if planes.is_empty() {
            planes = f.planar.iter().map(|p| p.to_vec()).collect();
        } else {
            for (dst, src) in planes.iter_mut().zip(f.planar.iter()) {
                dst.extend_from_slice(src);
            }
        }
        Ok(())
    })?;
    Ok((info, planes))
}

fn by_name<'a>(fixtures: &'a [OracleFixture], name: &str) -> &'a OracleFixture {
    fixtures.iter().find(|f| f.name == name).expect(name)
}

#[test]
fn timeline_matches_ffprobe_oracle() -> Result<()> {
    let fixtures = parse_oracle();
    assert_eq!(fixtures.len(), 5, "oracle coverage drifted");
    for f in &fixtures {
        check_timeline(f)?;
    }
    Ok(())
}

/// mov-muxer fMP4 has no elst (ffprobe first pts 0, priming presented).
/// The DASH init elst trims that same priming (ffprobe first pts negative).
#[test]
fn elst_priming_trim_versus_untrimmed_presentation() -> Result<()> {
    let fixtures = parse_oracle();
    for f in &fixtures {
        let (info, _) = collect(&fixture_bytes(&f.name))?;
        assert_eq!(
            info.priming, f.present.priming,
            "{} priming vs ffprobe edit",
            f.name
        );
    }
    let lc = by_name(&fixtures, "fmp4_lc.mp4");
    let dash = by_name(&fixtures, "fmp4_dash");
    assert_eq!(lc.present.priming, None, "mov-muxer presents priming");
    let priming = dash.present.priming.expect("dash elst priming") as usize;
    let (uinfo, upcm) = collect(&fixture_bytes(&lc.name))?;
    let (tinfo, tpcm) = collect(&fixture_bytes(&dash.name))?;
    assert_eq!(uinfo.priming, None);
    assert_eq!(tinfo.priming, Some(priming as u64));
    let n = tinfo.samples as usize;
    assert!(uinfo.samples as usize >= priming + n);
    for ch in 0..tpcm.len() {
        assert_eq!(
            &tpcm[ch][..],
            &upcm[ch][priming..priming + n],
            "dash ch{ch} is the untrimmed stream after the elst skip"
        );
    }
    Ok(())
}

#[test]
fn mfhd_gap_against_oracle_sequence_is_malformed() -> Result<()> {
    let fixtures = parse_oracle();
    let lc = by_name(&fixtures, "fmp4_lc.mp4");
    assert!(lc.frags.len() >= 2, "gap needs two fragments");
    let mut data = fixture_bytes(&lc.name);
    let seqs = mfhd_sequences(&data)?;
    assert_eq!(
        seqs.iter().map(|(_, s)| *s).collect::<Vec<_>>(),
        lc.frags.iter().map(|f| f.seq).collect::<Vec<_>>(),
        "file sequences are the oracle sequences"
    );
    let (at, seq) = seqs[1];
    let gap = seq.wrapping_add(5);
    assert_ne!(gap, seqs[0].1.wrapping_add(1));
    data[at..at + 4].copy_from_slice(&gap.to_be_bytes());
    match decode_with(&data, &DecodeOptions::audio()) {
        Err(AacError::Malformed(MalformedKind::Syntax)) => Ok(()),
        other => panic!("mfhd gap: {other:?}"),
    }
}
