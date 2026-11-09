//! Isolated Apple-container consumer. No native tools run in cargo test.
use std::{error::Error, io::Cursor};
use syom::{DecodeOptions, ProbeDuration, ProbeTrim};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn append(planes: &mut Vec<Vec<f32>>, frame: syom::Frame<'_>) {
    if planes.is_empty() {
        planes.resize_with(frame.planar.len(), Vec::new);
    }
    assert_eq!(planes.len(), frame.planar.len());
    for (dst, src) in planes.iter_mut().zip(frame.planar.iter()) {
        dst.extend_from_slice(src);
    }
}

fn identical(got: &[Vec<f32>], expected: &[Vec<f32>], start: usize) {
    assert_eq!(got.len(), expected.len());
    for (channel, (got, expected)) in got.iter().zip(expected.iter()).enumerate() {
        let expected = &expected[start..];
        assert_eq!(got.len(), expected.len(), "channel {channel} length");
        let mismatch = got
            .iter()
            .zip(expected.iter())
            .position(|(a, b)| a.to_bits() != b.to_bits());
        assert_eq!(mismatch, None, "channel {channel}: first different sample");
    }
}

fn verify(file: &[u8], priming: u64, remainder: u64, samples: u64, rate: u32) -> Result<()> {
    let opts = DecodeOptions::audio();
    let probe = syom::probe(file)?;
    assert_eq!(probe.duration, ProbeDuration::Exact { samples });
    assert_eq!(probe.trim, ProbeTrim::Exact { priming, remainder });
    let one = syom::decode_with(file, &opts)?;
    assert_eq!(one.sample_rate, rate);
    assert_eq!(one.priming, Some(priming));
    assert_eq!(one.remainder, Some(remainder));
    assert!(one.channels.iter().all(|p| p.len() as u64 == samples));
    let mut streamed = Vec::new();
    let info = syom::decode_streaming(file, &opts, |f| {
        append(&mut streamed, f);
        Ok(())
    })?;
    identical(&streamed, &one.channels, 0);
    assert_eq!(info.priming, Some(priming));
    let mut streamed = Vec::new();
    let info = syom::decode_seek_streaming(Cursor::new(file), &opts, |f| {
        append(&mut streamed, f);
        Ok(())
    })?;
    identical(&streamed, &one.channels, 0);
    assert_eq!(info.remainder, Some(remainder));
    let mut source = syom::M4aSeek::open(Cursor::new(file), opts)?;
    assert!(source.bounded_presentation());
    let mut streamed = Vec::new();
    let info = source.decode(|f| {
        append(&mut streamed, f);
        Ok(())
    })?;
    identical(&streamed, &one.channels, 0);
    assert_eq!(info.priming, Some(priming));
    assert_eq!(info.remainder, Some(remainder));
    assert!(source.presentation_complete());
    source.seek_exact((samples / 2) as i64, u64::MAX)?;
    let mut tail = Vec::new();
    source.decode(|f| {
        append(&mut tail, f);
        Ok(())
    })?;
    identical(&tail, &one.channels, samples as usize / 2);
    println!(
        "{{\"samples\":{samples},\"priming\":{priming},\"remainder\":{remainder},\"rate\":{rate},\"channels\":{},\"paths_bit_identical\":true}}",
        one.channels.len(),
    );
    Ok(())
}

fn bx(typ: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut out = ((body.len() + 8) as u32).to_be_bytes().to_vec();
    out.extend_from_slice(typ);
    out.extend_from_slice(body);
    out
}

fn self_test() -> Result<()> {
    // Authored input exercises the consumer on Linux without claiming Apple execution.
    let mut file = syom::encode_with(&[vec![0.1; 1124]], 48_000, &syom::EncodeOptions::m4a())?;
    let edit = file
        .windows(4)
        .position(|w| w == b"elst")
        .ok_or("no elst")?;
    file[edit..edit + 4].copy_from_slice(b"free");
    let mut item = bx(b"mean", b"\0\0\0\0com.apple.iTunes");
    item.extend(bx(b"name", b"\0\0\0\0iTunSMPB"));
    item.extend(bx(
        b"data",
        b"\0\0\0\x01\0\0\0\0 00000000 00000400 0000039C 0000000000000464",
    ));
    let mut meta = vec![0; 4];
    meta.extend(bx(b"ilst", &bx(b"----", &item)));
    let udta = bx(b"udta", &bx(b"meta", &meta));
    let moov = file
        .windows(4)
        .position(|w| w == b"moov")
        .ok_or("no moov")?
        - 4;
    let size = u32::from_be_bytes(file[moov..moov + 4].try_into()?) as usize;
    assert_eq!(moov + size, file.len());
    file[moov..moov + 4].copy_from_slice(&((size + udta.len()) as u32).to_be_bytes());
    file.extend(udta);
    verify(&file, 1024, 924, 1124, 48_000)
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args == ["--self-test"] {
        return self_test();
    }
    if args.len() != 5 {
        return Err("expected: FILE PRIMING REMAINDER VALID_SAMPLES RATE".into());
    }
    verify(
        &std::fs::read(&args[0])?,
        args[1].parse()?,
        args[2].parse()?,
        args[3].parse()?,
        args[4].parse()?,
    )
}
