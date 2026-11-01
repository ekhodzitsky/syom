//! Opt-in LC CBR qualification using the unchanged CORE_TUNE protocol.
use super::*;

pub(super) fn run(repo: &Path) -> ExitCode {
    let work = std::env::temp_dir().join(format!("syom-cbr-qualification-{}", std::process::id()));
    fs::create_dir_all(&work).expect("qualification work directory");
    let mut passed = true;
    let selected: Vec<_> = clips(repo)
        .into_iter()
        .filter(|clip| matches!(clip.name, "tremolo" | "lecture" | "lecture-m"))
        .collect();
    if selected.len() != 3 {
        eprintln!("required lecture/lecture-m fixture missing");
        return ExitCode::FAILURE;
    }
    for clip in &selected {
        let rates: &[u32] = if clip.name == "tremolo" {
            &[48_000, 64_000]
        } else {
            &[24_000]
        };
        for &bps in rates {
            let mut current = Vec::new();
            for cbr in [false, true] {
                let mode = if cbr { "cbr" } else { "abr" };
                let tag = format!("{}-{mode}-{bps}", clip.name);
                let opts = EncodeOptions::adts().with_bitrate_bps(bps).with_cbr(cbr);
                let adts =
                    encode_with(&clip.pcm, RATE, &opts).expect("encode qualification fixture");
                let path = work.join(format!("{tag}.adts"));
                fs::write(&path, &adts).expect("write temporary AAC");
                check_format(&path, clip.pcm.len());
                let decoded = strict_decode(&path, clip.pcm.len());
                assert_eq!(
                    fs::metadata(path.with_extension("dec.f32")).unwrap().len()
                        % (4 * clip.pcm.len() as u64),
                    0
                );
                assert_eq!(decoded.len(), clip.pcm.len());
                assert!(decoded.iter().all(|p| p.len() >= clip.pcm[0].len() + 1024));
                assert!(decoded.iter().flatten().all(|v| v.is_finite()));
                let score = strict_score(&clip.pcm, &decoded, adts.len());
                assert!(score.snr.is_finite() && score.lf.is_finite() && score.hf.is_finite());
                assert!(score.note.is_empty(), "alignment drift: {}", score.note);
                let (snr_floor, lf_floor) = match (clip.name, bps) {
                    ("tremolo", 48_000) => (Some(44.8), 54.7),
                    ("tremolo", 64_000) => (Some(46.2), 55.9),
                    ("lecture", _) => (None, 42.6),
                    ("lecture-m", _) => (None, 38.9),
                    _ => unreachable!(),
                };
                // Historical report publishes tenths; evaluate exactly that
                // reporting precision rather than inventing a tolerance band.
                let tenth = |v: f64| (v * 10.0).round() / 10.0;
                let gate = tenth(score.lf) >= lf_floor
                    && snr_floor.is_none_or(|floor| tenth(score.snr) >= floor);
                passed &= gate;
                println!(
                    "{{\"kind\":\"quality\",\"clip\":\"{}\",\"mode\":\"{mode}\",\"requested_bps\":{bps},\"actual_kbps\":{},\"lag\":{},\"snr_db\":{},\"lf_db\":{},\"hf_error_db\":{},\"lf_floor_db\":{lf_floor},\"snr_floor_db\":{},\"decoded_samples\":{},\"source_samples\":{},\"adts_bytes\":{},\"adts_sha256\":\"{}\",\"decode_finite\":true,\"historical_gate\":{gate}}}",
                    clip.name, score.actual_k, score.lag, score.snr, score.lf, score.hf,
                    snr_floor.map_or("null".to_string(), |v| v.to_string()), decoded[0].len(), clip.pcm[0].len(), adts.len(), digest(&path),
                );
                current.push(score);
            }
            assert_eq!(current.len(), 2);
            rate_cell(&work, clip, bps);
        }
    }
    fs::remove_dir_all(&work).expect("remove temporary media");
    if passed {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn check_format(path: &Path, channels: usize) {
    let probe = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "a:0",
            "-show_entries",
            "stream=sample_rate,channels,profile",
            "-of",
            "default=noprint_wrappers=1",
        ])
        .arg(path)
        .output()
        .expect("ffprobe");
    assert!(probe.status.success(), "ffprobe failed");
    let text = String::from_utf8(probe.stdout).expect("ffprobe UTF-8");
    let fields: std::collections::HashMap<_, _> = text
        .lines()
        .filter_map(|line| line.split_once('='))
        .collect();
    assert_eq!(fields.get("profile"), Some(&"LC"));
    assert_eq!(fields.get("sample_rate"), Some(&"48000"));
    assert_eq!(
        fields.get("channels").unwrap().parse::<usize>().unwrap(),
        channels
    );
}

fn strict_decode(path: &Path, channels: usize) -> Vec<Vec<f32>> {
    let out = path.with_extension("dec.f32");
    let status = Command::new("ffmpeg")
        .args(["-y", "-v", "error", "-xerror", "-i"])
        .arg(path)
        .args(["-f", "f32le", "-acodec", "pcm_f32le"])
        .arg(&out)
        .output()
        .expect("strict FFmpeg decode");
    assert!(
        status.status.success(),
        "FFmpeg rejected stream: {}",
        String::from_utf8_lossy(&status.stderr)
    );
    assert!(status.stderr.is_empty(), "FFmpeg reported errors");
    let raw = fs::read(out).expect("decoded PCM");
    assert_eq!(raw.len() % (4 * channels), 0);
    let mut pcm = vec![Vec::with_capacity(raw.len() / 4 / channels); channels];
    for frame in raw.chunks_exact(4 * channels) {
        for (c, plane) in pcm.iter_mut().enumerate() {
            plane.push(f32::from_le_bytes(
                frame[4 * c..4 * c + 4].try_into().unwrap(),
            ));
        }
    }
    pcm
}

fn rate_cell(work: &Path, clip: &Clip, bps: u32) {
    // Repeat the unchanged historical fixture only for the >=10s rate check.
    // The quality cells above retain the original duration and boundaries.
    let n = RATE as usize * 10;
    let pcm: Vec<Vec<f32>> = clip
        .pcm
        .iter()
        .map(|plane| plane.iter().copied().cycle().take(n).collect())
        .collect();
    let adts = encode_with(
        &pcm,
        RATE,
        &EncodeOptions::adts().with_bitrate_bps(bps).with_cbr(true),
    )
    .expect("10s CBR encode");
    let path = work.join(format!("{}-10s-{bps}.adts", clip.name));
    fs::write(&path, &adts).expect("write rate stream");
    check_format(&path, pcm.len());
    let decoded = strict_decode(&path, pcm.len());
    assert!(decoded.iter().all(|p| p.len() >= n + 1024 && rms(p) > 1e-6));
    assert!(decoded.iter().flatten().all(|v| v.is_finite()));
    assert_eq!(&adts[..3], b"ID3");
    let tag_len = adts[6..10]
        .iter()
        .fold(0usize, |n, b| (n << 7) | usize::from(*b));
    let mut at = 10 + tag_len;
    let (mut payload_bytes, mut max_au_bits, mut frames) = (0usize, 0usize, 0usize);
    while at < adts.len() {
        let h = &adts[at..];
        assert_eq!(h[0], 0xff);
        assert_eq!(h[1] & 0xf6, 0xf0); // ADTS sync and layer.
        let len = (usize::from(h[3] & 3) << 11) | (usize::from(h[4]) << 3) | usize::from(h[5] >> 5);
        let header = if h[1] & 1 == 1 { 7 } else { 9 };
        assert!(len >= header && at + len <= adts.len());
        let fullness = (u16::from(h[5] & 31) << 6) | u16::from(h[6] >> 2);
        assert_ne!(fullness, 0x7ff);
        let bits = (len - header) * 8;
        assert!(bits <= 6144 * pcm.len());
        payload_bytes += len - header;
        max_au_bits = max_au_bits.max(bits);
        frames += 1;
        at += len;
    }
    assert_eq!(decoded[0].len(), frames * 1024);
    let actual_bps = payload_bytes as f64 * 8.0 / 10.0;
    let error_percent = 100.0 * (actual_bps / f64::from(bps) - 1.0);
    let rate_gate = error_percent.abs() <= 3.0;
    println!("{{\"kind\":\"rate\",\"clip\":\"{}\",\"mode\":\"cbr\",\"requested_bps\":{bps},\"duration_seconds\":10,\"fixture_repeated\":true,\"payload_bps\":{actual_bps},\"error_percent\":{error_percent},\"frames\":{frames},\"max_au_bits\":{max_au_bits},\"cap_bits\":{},\"adts_bytes\":{},\"adts_sha256\":\"{}\",\"decoded_samples\":{},\"decode_finite\":true,\"fullness_vbr_markers\":0,\"rate_gate\":{rate_gate}}}", clip.name, 6144 * pcm.len(), adts.len(), digest(&path), decoded[0].len());
    assert!(rate_gate, "CBR payload rate outside documented ±3%");
}

fn digest(path: &Path) -> String {
    let output = Command::new("sha256sum")
        .arg(path)
        .output()
        .expect("sha256sum");
    assert!(output.status.success());
    let hash = String::from_utf8(output.stdout)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .to_string();
    assert!(hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()));
    hash
}

fn strict_score(pcm: &[Vec<f32>], dec: &[Vec<f32>], bytes: usize) -> CellScore {
    // Exact row()/score_adts() arithmetic, but consume the already validated
    // -xerror decode rather than starting a second permissive decoder.
    let lag = refine_lag(&pcm[0], &dec[0], 1024, 128);
    let (mut ps, mut pe, mut lf, mut hf) = (0.0, 0.0, 0.0, 0.0);
    for c in 0..pcm.len() {
        let (w, g) = aligned(&pcm[c], &dec[c], lag);
        assert_eq!(w.len(), pcm[c].len());
        for i in 0..w.len() {
            let s = f64::from(w[i]);
            let er = s - f64::from(g[i]);
            ps += s * s;
            pe += er * er;
        }
        let (l, h) = band_measures(w, g);
        lf += l / pcm.len() as f64;
        hf += h / pcm.len() as f64;
    }
    CellScore {
        actual_k: bytes as f64 * 8.0 / (pcm[0].len() as f64 / RATE as f64) / 1000.0,
        lag,
        snr: if pe == 0.0 {
            200.0
        } else {
            10.0 * (ps / pe).log10()
        },
        lf,
        hf,
        note: if (lag - 1024).abs() > 96 {
            format!("align drift declared=1024 used={lag}")
        } else {
            String::new()
        },
    }
}
