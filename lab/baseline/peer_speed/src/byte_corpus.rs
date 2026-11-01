//! Untimed byte identity corpus across encoder modes and frame histories.
use super::*;

pub(super) fn run(output: Option<&str>) -> i32 {
    if let Some(dir) = output {
        std::fs::create_dir_all(dir).expect("corpus output directory");
    }
    let row = |name: &str, pcm: &[Vec<f32>], rate, opts: EncodeOptions| {
        let bytes = encode_with(pcm, rate, &opts).expect(name);
        if let Some(dir) = output {
            std::fs::write(Path::new(dir).join(format!("{name}.bin")), &bytes)
                .expect("corpus output");
        }
        let hash = bytes.iter().fold(0xcbf2_9ce4_8422_2325u64, |hash, &byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
        });
        println!(
            "{{\"cell\":\"{name}\",\"bytes\":{},\"fnv1a\":\"{hash:016x}\"}}",
            bytes.len()
        );
    };
    for rate in [8000, 16000, 44100, 48000, 64000] {
        for ch in [1, 2] {
            row(
                &format!("lc-{rate}-{ch}"),
                &source(ch),
                rate,
                EncodeOptions::adts().with_bitrate_bps(24000),
            );
        }
    }
    for ch in [1, 2] {
        let pcm = source(ch);
        for bps in [24000, 64000, 128000, 192000] {
            row(
                &format!("abr-{ch}-{bps}"),
                &pcm,
                RATE,
                EncodeOptions::adts().with_bitrate_bps(bps),
            );
            row(
                &format!("cbr-{ch}-{bps}"),
                &pcm,
                RATE,
                EncodeOptions::adts().with_bitrate_bps(bps).with_cbr(true),
            );
        }
        for quality in [0, 5, 10] {
            row(
                &format!("q-{ch}-{quality}"),
                &pcm,
                RATE,
                EncodeOptions::adts().with_quality(quality),
            );
        }
        let base = EncodeOptions::adts().with_bitrate_bps(32000);
        let modes = [
            ("pns", base.clone().with_pns(true)),
            ("intensity", base.clone().with_intensity(true)),
            ("refine", base.clone().with_band_refine(true)),
            ("short-tns", base.clone().with_short_tns(true)),
            ("group", base.clone().with_short_group(true)),
            ("tonality", base.clone().with_tonality(true)),
            ("no-ath", base.clone().with_ath(false)),
            ("lookahead", base.clone().with_lookahead(true)),
            (
                "combined",
                base.with_pns(true)
                    .with_intensity(true)
                    .with_short_tns(true)
                    .with_short_group(true)
                    .with_band_refine(true)
                    .with_lookahead(true)
                    .with_tonality(true),
            ),
        ];
        for (name, opts) in modes {
            row(&format!("{name}-{ch}"), &pcm, RATE, opts);
        }
        row(
            &format!("ld-{ch}"),
            &pcm,
            RATE,
            EncodeOptions::m4a().with_ld(true),
        );
        row(
            &format!("he1-{ch}"),
            &pcm,
            RATE,
            EncodeOptions::adts().with_he(true).with_bitrate_bps(32000),
        );
    }
    row(
        "he2",
        &source(2),
        RATE,
        EncodeOptions::adts()
            .with_he_v2(true)
            .with_bitrate_bps(24000),
    );
    for ch in [3, 4, 5, 6, 8] {
        row(
            &format!("mc-{ch}"),
            &source(ch),
            RATE,
            EncodeOptions::adts().with_bitrate_bps(128000),
        );
    }
    0
}

fn source(channels: usize) -> Vec<Vec<f32>> {
    (0..channels)
        .map(|ch| {
            let mut seed = 42u32.wrapping_add(ch as u32);
            (0..8192)
                .map(|i| {
                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    let noise = seed as i32 as f32 / i32::MAX as f32;
                    let gain = if i < 2048 { 0.05 } else { 0.2 };
                    gain * (i as f32 * (0.13 + ch as f32 * 0.019)).sin()
                        + 0.08 * noise
                        + if i % 2048 < 2 { 0.5 } else { 0.0 }
                })
                .collect()
        })
        .collect()
}
