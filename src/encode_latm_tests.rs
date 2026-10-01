//! TASK-94: LATM/LOAS output — frame syntax against the in-tree parser,
//! ADTS / raw / LATM equivalence, push parity, overhead, lavc goldens.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::encode_tests::{assert_decode_matches_lavc_pub, lavc_fixture};
use crate::engine::bits::BitReader;
use crate::engine::latm::{LOAS_SYNC, MuxCfg, read_payload};
use crate::{
    DecodeOptions, EncodeContainer, EncodeOptions, Encoder, ProbeContainer, ProbeProfile,
    decode_with, encode_with, encode_write, probe, sniff_is_latm, wrap_loas_au,
};

fn latm(opts: &EncodeOptions) -> EncodeOptions {
    opts.clone().with_container(EncodeContainer::Latm)
}

/// Split a LOAS stream into `(frame, AudioMuxElement body)` slices.
fn loas_frames(stream: &[u8]) -> Vec<(&[u8], &[u8])> {
    let mut out = Vec::new();
    let mut at = 0;
    while at + 3 <= stream.len() {
        let v = (u32::from(stream[at]) << 16)
            | (u32::from(stream[at + 1]) << 8)
            | u32::from(stream[at + 2]);
        assert_eq!(v >> 13, LOAS_SYNC, "sync at {at}");
        let len = (v & 0x1FFF) as usize;
        out.push((&stream[at..at + 3 + len], &stream[at + 3..at + 3 + len]));
        at += 3 + len;
    }
    assert_eq!(at, stream.len(), "no trailing bytes");
    out
}

/// Parse one AudioMuxElement with the decoder's structures.
fn parse_mux(body: &[u8]) -> (MuxCfg, Vec<u8>) {
    let mut br = BitReader::new(body);
    assert!(
        !br.read_bit().unwrap(),
        "useSameStreamMux = 0: config in every frame"
    );
    let cfg = MuxCfg::parse(&mut br).unwrap();
    let au = read_payload(&mut br, &cfg).unwrap();
    (cfg, au)
}

#[test]
fn every_frame_carries_a_parseable_config_and_the_raw_access_unit() {
    let pcm = lavc_fixture();
    for (opts, aot_ok) in [
        (EncodeOptions::adts(), 2u8),
        (EncodeOptions::adts().with_lookahead(true), 2),
        (EncodeOptions::adts().with_quality(7), 2),
        (EncodeOptions::low_rate(), 2),
    ] {
        let stream = encode_with(&pcm, 48_000, &latm(&opts)).unwrap();
        assert!(sniff_is_latm(&stream));
        let raw_aus: Vec<Vec<u8>> = {
            let mut enc = Encoder::new(
                48_000,
                2,
                &opts.clone().with_container(EncodeContainer::Raw),
            )
            .unwrap();
            let planes: Vec<&[f32]> = pcm.iter().map(Vec::as_slice).collect();
            let mut v = Vec::new();
            enc.feed(&planes, |f| {
                v.push(f.payload.to_vec());
                Ok(())
            })
            .unwrap();
            enc.finish(|f| {
                v.push(f.payload.to_vec());
                Ok(())
            })
            .unwrap();
            v
        };
        let frames = loas_frames(&stream);
        assert_eq!(
            frames.len(),
            raw_aus.len(),
            "one LOAS frame per access unit"
        );
        for ((_, body), au) in frames.iter().zip(&raw_aus) {
            let (cfg, payload) = parse_mux(body);
            assert_eq!(&payload, au, "payload is the raw AU");
            assert_eq!(cfg.asc.aot, aot_ok);
            assert_eq!(cfg.asc.channel_configuration, 2);
            assert_eq!(cfg.frame_length_type, 0);
            assert_eq!(cfg.num_sub_frames, 0);
            assert_eq!(cfg.asc.sbr_present, opts.he);
            assert_eq!(cfg.asc.output_sample_rate, 48_000);
            assert_eq!(cfg.asc.sample_rate, if opts.he { 24_000 } else { 48_000 });
        }
        let p = probe(&stream).unwrap();
        assert_eq!(p.container, ProbeContainer::Latm);
        assert_eq!(
            p.profile,
            if opts.he {
                ProbeProfile::HeAac
            } else {
                ProbeProfile::Lc
            }
        );
    }
}

#[test]
fn latm_adts_and_raw_decode_to_the_same_pcm_and_overhead_is_documented() {
    let pcm = lavc_fixture();
    for opts in [EncodeOptions::adts(), EncodeOptions::low_rate()] {
        let adts = encode_with(&pcm, 48_000, &opts).unwrap();
        let loas = encode_with(&pcm, 48_000, &latm(&opts)).unwrap();
        let da = decode_with(&adts, &DecodeOptions::unbounded()).unwrap();
        let dl = decode_with(&loas, &DecodeOptions::unbounded()).unwrap();
        assert_eq!(
            (da.sample_rate, da.channels.len()),
            (dl.sample_rate, dl.channels.len())
        );
        let skip = da.priming.unwrap_or(0) as usize;
        for (a, lch) in da.channels.iter().zip(&dl.channels) {
            assert_eq!(
                a.as_slice(),
                &lch[skip..skip + a.len()],
                "identical access units → identical PCM"
            );
        }
        let adts_au = crate::gapless::strip_id3(&adts);
        let n = loas_frames(&loas).len();
        let per_frame = (loas.len() as f64 - (adts_au.len() - 7 * n) as f64) / n as f64;
        println!(
            "{}: LATM framing {per_frame:.1} B/frame vs ADTS 7 B ({} vs {} B total)",
            if opts.he { "HE" } else { "LC" },
            loas.len(),
            adts.len()
        );
        assert!(
            per_frame < 12.0,
            "LOAS 3 + StreamMuxConfig + lengths + align"
        );
    }
}

#[test]
fn push_encoder_latm_matches_one_shot_and_wrap_loas_au() {
    let pcm = lavc_fixture();
    let planes: Vec<&[f32]> = pcm.iter().map(Vec::as_slice).collect();
    for opts in [
        latm(&EncodeOptions::adts()),
        latm(&EncodeOptions::low_rate()),
        latm(&EncodeOptions::adts().with_lookahead(true)),
    ] {
        let one = encode_with(&pcm, 48_000, &opts).unwrap();
        for chunk in [333usize, 4096, 100_000] {
            let mut enc = Encoder::new(48_000, 2, &opts).unwrap();
            let asc = enc.asc().to_vec();
            let mut got = Vec::new();
            let mut rewrapped = Vec::new();
            let mut cb = |f: crate::EncodedFrame<'_>| {
                got.extend_from_slice(f.au);
                rewrapped.extend_from_slice(&wrap_loas_au(f.payload, &asc)?);
                assert!(sniff_is_latm(f.au));
                Ok(())
            };
            let n = planes[0].len();
            let mut at = 0;
            while at < n {
                let end = (at + chunk).min(n);
                let c: Vec<&[f32]> = planes.iter().map(|p| &p[at..end]).collect();
                enc.feed(&c, &mut cb).unwrap();
                at = end;
            }
            let info = enc.finish(&mut cb).unwrap();
            assert_eq!(got, one, "chunk {chunk}");
            assert_eq!(rewrapped, one, "payload + asc rewrap == frames");
            assert_eq!(info.bytes as usize, one.len());
        }
        let mut sink = Vec::new();
        encode_write(&mut sink, &planes, 48_000, &opts).unwrap();
        assert_eq!(sink, one, "encode_write on LATM");
    }
}

#[test]
fn oversized_mux_elements_and_bad_configs_are_typed() {
    let big = vec![0u8; 0x2000];
    assert!(
        wrap_loas_au(&big, &[0x11, 0x90]).is_err(),
        "AudioMuxElement over 8191 bytes"
    );
    let ok = vec![0u8; 8_100]; // + 32 length bytes + config still under 8191
    assert!(wrap_loas_au(&ok, &[0x11, 0x90]).is_ok());
    assert!(
        wrap_loas_au(&[0u8; 10], &[0xFF]).is_err(),
        "unparseable ASC"
    );
}

/// Offline mint: `MINT_GOLDENS=1 cargo test --lib mint_lavc_latm_goldens`, then
/// `ffmpeg -y -i src/goldens/enc48lt.latm -f s16le src/goldens/enc48lt.lavc.s16`
/// and the same for `he48elt`.
#[test]
fn mint_lavc_latm_goldens() {
    if std::env::var("MINT_GOLDENS").is_err() {
        return;
    }
    let pcm = lavc_fixture();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/goldens");
    std::fs::write(
        dir.join("enc48lt.latm"),
        encode_with(&pcm, 48_000, &latm(&EncodeOptions::adts())).unwrap(),
    )
    .unwrap();
    std::fs::write(
        dir.join("he48elt.latm"),
        encode_with(&pcm, 48_000, &latm(&EncodeOptions::low_rate())).unwrap(),
    )
    .unwrap();
}

#[test]
fn lavc_decodes_our_loas_lc_and_he() {
    let pcm = lavc_fixture();
    let lc = encode_with(&pcm, 48_000, &latm(&EncodeOptions::adts())).unwrap();
    assert_eq!(
        lc.as_slice(),
        &include_bytes!("goldens/enc48lt.latm")[..],
        "LC LOAS golden drifted; re-mint"
    );
    assert_decode_matches_lavc_pub(&lc, include_bytes!("goldens/enc48lt.lavc.s16"));
    let he = encode_with(&pcm, 48_000, &latm(&EncodeOptions::low_rate())).unwrap();
    assert_eq!(
        he.as_slice(),
        &include_bytes!("goldens/he48elt.latm")[..],
        "HE LOAS golden drifted; re-mint"
    );
    assert_decode_matches_lavc_pub(&he, include_bytes!("goldens/he48elt.lavc.s16"));
}
