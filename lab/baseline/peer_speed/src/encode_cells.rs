//! File-based one-shot timers and the original CORE_TUNE speech fixtures.
use super::*;

pub(super) fn prepare(dir: &str) -> i32 {
    let dir = Path::new(dir);
    std::fs::create_dir_all(dir).expect("fixture directory");
    let voice = vec![voice(3, 150.0), voice(4, 113.0)];
    write_planar(dir.join("voice-like.f32").to_str().unwrap(), &voice).expect("voice PCM");
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let lecture = decode_with(
        &std::fs::read(repo.join("src/goldens/lecture.m4a")).expect("lecture golden"),
        &DecodeOptions::unbounded(),
    )
    .expect("lecture reference")
    .channels;
    assert_eq!(lecture.len(), 2);
    assert!(lecture.iter().all(|plane| plane.len() == 12_000));
    let mono = vec![lecture[0]
        .iter()
        .zip(&lecture[1])
        .map(|(left, right)| 0.5 * (left + right))
        .collect()];
    for (name, pcm) in [("lecture.f32", lecture), ("lecture-m.f32", mono)] {
        write_planar(dir.join(name).to_str().unwrap(), &pcm).expect("speech PCM");
    }
    0
}

/// RATE CH BPS IN.f32 WARM REPS [OUT.adts]. File I/O and verification hashing
/// are outside the timed region. Every measured call includes open/finish/drop.
pub(super) fn file(args: &[String]) -> i32 {
    assert!(
        (6..=7).contains(&args.len()),
        "encode-pcm RATE CH BPS IN.f32 WARM REPS [OUT.adts]"
    );
    let rate = args[0].parse::<u32>().expect("rate");
    let channels = args[1].parse::<usize>().expect("channels");
    assert!((1..=2).contains(&channels));
    let bps = args[2].parse::<u32>().expect("bitrate");
    let raw = std::fs::read(&args[3]).expect("planar PCM");
    assert!(!raw.is_empty() && raw.len() % (4 * channels) == 0);
    let samples = raw.len() / 4 / channels;
    let pcm: Vec<Vec<f32>> = raw
        .chunks_exact(samples * 4)
        .map(|plane| {
            plane
                .chunks_exact(4)
                .map(|x| f32::from_le_bytes(x.try_into().unwrap()))
                .collect()
        })
        .collect();
    let warm = args[4].parse::<usize>().expect("warmups");
    let reps = args[5].parse::<usize>().expect("reps");
    assert!(reps > 0);
    let opts = EncodeOptions::adts().with_bitrate_bps(bps);
    let mut ms = Vec::with_capacity(reps);
    let mut last_len = 0;
    print_host();
    for i in 0..warm + reps {
        let begin = Instant::now();
        let bytes = encode_with(&pcm, rate, &opts).expect("encode PCM");
        last_len = bytes.len();
        std::hint::black_box(bytes.len());
        drop(bytes);
        let elapsed = begin.elapsed().as_secs_f64() * 1e3;
        if i >= warm {
            ms.push(elapsed);
        }
    }
    // An extra untimed call pins output identity; do not hash inside one
    // competitor's timed region while omitting the walk from the other.
    let verification = encode_with(&pcm, rate, &opts).expect("verification encode");
    assert_eq!(verification.len(), last_len);
    if let Some(path) = args.get(6) {
        std::fs::write(path, &verification).expect("verification ADTS");
    }
    ms.sort_by(f64::total_cmp);
    println!(
        "{{\"engine\":\"syom\",\"op\":\"encode\",\"cell\":\"file-lc\",\"profile\":\"release-thin-lto-codegen-units-1\",\"bitrate_bps\":{bps},\"warm\":{warm},\"reps\":{reps},\"median_ms\":{:.4},\"min_ms\":{:.4},\"max_ms\":{:.4},\"rate\":{rate},\"channels\":{channels},\"samples\":{samples},\"adts_bytes\":{last_len},\"pcm_fnv1a\":\"{:016x}\",\"adts_fnv1a\":\"{:016x}\",\"samples_ms\":{}}}",
        ms[reps / 2], ms[0], ms[reps - 1], fnv(&raw), fnv(&verification), fmt_reps(&ms)
    );
    0
}

fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}
