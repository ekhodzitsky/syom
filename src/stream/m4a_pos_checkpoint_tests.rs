#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;
use std::io::Cursor;

fn payloads() -> Vec<Vec<u8>> {
    let data = include_bytes!("../goldens/pns48.adts");
    let mut at = 0;
    let mut result = Vec::new();
    while at < data.len() {
        let (h, off) = crate::engine::adts::AdtsHeader::parse(&data[at..]).unwrap();
        let end = at + usize::from(h.aac_frame_length);
        result.push(data[at + off..end].to_vec());
        at = end;
    }
    result
}

fn mux(payloads: &[Vec<u8>]) -> Vec<u8> {
    crate::m4a_write::mux_aac_lc(payloads, 3, 1, 48000, payloads.len() as u64 * 1024, 0).unwrap()
}

fn open(bytes: &[u8]) -> M4aSeek<Cursor<&[u8]>> {
    M4aSeek::open(Cursor::new(bytes), DecodeOptions::speech()).unwrap()
}

fn pause(source: &mut M4aSeek<Cursor<&[u8]>>) {
    assert!(
        source
            .decode(|_| Err(AacError::format("intentional callback pause")))
            .is_err()
    );
}

#[test]
fn persistent_checkpoint_preserves_actual_advanced_pns_rng() {
    let data = mux(&payloads());
    let mut source = open(&data);
    assert!(source.supports_checkpoint());
    assert!(source.checkpoint().is_none());
    pause(&mut source);
    assert_ne!(
        source.dec.rng,
        Lcg::new(),
        "fixture must actually advance PNS"
    );
    let checkpoint = source.checkpoint().unwrap();
    assert_eq!(checkpoint.len(), SIZE);
    let mut fresh = open(&data);
    fresh.restore_checkpoint(&checkpoint).unwrap();
    assert_eq!(fresh.dec.rng, source.dec.rng);
    let mut expected = Vec::new();
    source
        .decode(|f| {
            expected.extend(f.planar[0].iter().map(|v| v.to_bits()));
            Ok(())
        })
        .unwrap();
    let mut got = Vec::new();
    fresh
        .decode(|f| {
            got.extend(f.planar[0].iter().map(|v| v.to_bits()));
            Ok(())
        })
        .unwrap();
    assert_eq!(got, expected);
}

#[test]
fn malformed_checkpoint_rejection_is_transactional() {
    let data = mux(&payloads());
    let mut source = open(&data);
    pause(&mut source);
    let original = source.checkpoint().unwrap();
    for length in 0..SIZE {
        assert!(source.restore_checkpoint(&original[..length]).is_err());
    }
    let mut malformed = Vec::new();
    let mut bad = original.clone();
    bad.push(0);
    malformed.push(bad);
    for at in [
        0,
        8,
        40,
        HEADER,
        HEADER + 8,
        HEADER + 16,
        OVERLAP,
        OVERLAP + Filterbank::CHECKPOINT_BYTES,
    ] {
        let mut bad = original.clone();
        bad[at] = 255;
        malformed.push(bad);
    }
    for at in [OVERLAP + 1, OVERLAP + Filterbank::CHECKPOINT_BYTES + 1] {
        for number in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut bad = original.clone();
            bad[at..at + 4].copy_from_slice(&number.to_le_bytes());
            malformed.push(bad);
        }
    }
    for bad in malformed {
        assert!(source.restore_checkpoint(&bad).is_err());
        assert_eq!(source.checkpoint().unwrap(), original);
    }
}

#[test]
fn changed_options_and_structural_source_are_incompatible() {
    let data = mux(&payloads());
    let mut source = open(&data);
    pause(&mut source);
    let checkpoint = source.checkpoint().unwrap();
    let options = DecodeOptions::speech().with_max_duration_secs(1234.0);
    let mut different = M4aSeek::open(Cursor::new(&data), options).unwrap();
    assert!(different.restore_checkpoint(&checkpoint).is_err());
    let mut frames = payloads();
    frames.pop();
    let other_data = mux(&frames);
    assert!(open(&other_data).restore_checkpoint(&checkpoint).is_err());
}

fn silent_ics(w: &mut crate::engine::bits::BitWriter) {
    w.write(128, 8);
    w.write(0, 1);
    w.write(0, 2);
    w.write(0, 1);
    w.write(0, 6);
    for _ in 0..4 {
        w.write(0, 1);
    }
}

fn unsupported_frame(pce: bool) -> Vec<u8> {
    let mut w = crate::engine::bits::BitWriter::new();
    if pce {
        w.write(5, 3);
        w.write(0, 4);
        w.write(1, 2);
        w.write(3, 4);
        for (value, bits) in [
            (1, 4),
            (0, 4),
            (0, 4),
            (0, 2),
            (0, 3),
            (0, 4),
            (0, 1),
            (0, 1),
            (0, 1),
            (0, 1),
            (0, 4),
        ] {
            w.write(value, bits);
        }
        while !w.bit_len().is_multiple_of(8) {
            w.write(0, 1);
        }
        w.write(0, 8);
    }
    w.write(0, 3);
    w.write(0, 4);
    silent_ics(&mut w);
    if !pce {
        w.write(2, 3);
        w.write(0, 4);
        w.write(1, 1);
        w.write(0, 3);
        w.write(0, 1);
        w.write(0, 4);
        w.write(0, 1);
        w.write(0, 1);
        w.write(0, 2);
        silent_ics(&mut w);
    }
    w.write(7, 3);
    w.finish()
}

#[test]
fn observed_pce_and_coupling_history_stay_ineligible_after_seek() {
    for pce in [true, false] {
        let mut frames = payloads();
        frames.insert(0, unsupported_frame(pce));
        let data = mux(&frames);
        let mut source = open(&data);
        pause(&mut source);
        assert_eq!(
            source.position(),
            1024,
            "authored unsupported frame must decode"
        );
        assert!(!source.checkpoint_compatible());
        assert!(source.checkpoint().is_none());
        source.seek(0).unwrap();
        assert!(!source.supports_checkpoint());
    }
}

#[test]
fn imported_finite_extreme_overlap_cannot_emit_nonfinite_pcm() {
    let pcm = vec![vec![0.1; 8192], vec![0.2; 8192]];
    let data = crate::encode_with(&pcm, 48000, &crate::EncodeOptions::m4a()).unwrap();
    let mut source = open(&data);
    pause(&mut source);
    let mut checkpoint = source.checkpoint().unwrap();
    for at in [OVERLAP + 1, OVERLAP + Filterbank::CHECKPOINT_BYTES + 1] {
        checkpoint[at..at + 4].copy_from_slice(&f32::MAX.to_le_bytes());
    }
    let mut fresh = open(&data);
    fresh.restore_checkpoint(&checkpoint).unwrap();
    let mut emitted = false;
    assert!(
        fresh
            .decode(|_| {
                emitted = true;
                Ok(())
            })
            .is_err()
    );
    assert!(!emitted);
    assert!(fresh.checkpoint().is_none());
}

#[test]
fn structural_tag_does_not_authenticate_same_geometry_media_payloads() {
    let data = mux(&payloads());
    let mut source = open(&data);
    pause(&mut source);
    let checkpoint = source.checkpoint().unwrap();
    let mut different = data.clone();
    let payload = source.track.frames[source.next_frame].0 as usize;
    different[payload] ^= 1;
    let mut other = open(&different);
    // Deliberately demonstrates the documented caller responsibility: source
    // contents with identical ASC/index geometry cannot be authenticated here.
    other.restore_checkpoint(&checkpoint).unwrap();
    assert_eq!(other.position(), source.position());
}

#[test]
fn read_fault_disqualifies_this_handle_even_after_seek() {
    struct FaultReader<'a> {
        cursor: Cursor<&'a [u8]>,
        fail: bool,
    }
    impl Read for FaultReader<'_> {
        fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
            if self.fail {
                Err(std::io::Error::other("injected read fault"))
            } else {
                self.cursor.read(bytes)
            }
        }
    }
    impl Seek for FaultReader<'_> {
        fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
            self.cursor.seek(position)
        }
    }
    let data = mux(&payloads());
    let mut valid = open(&data);
    pause(&mut valid);
    let checkpoint = valid.checkpoint().unwrap();
    let reader = FaultReader {
        cursor: Cursor::new(&data),
        fail: false,
    };
    let mut source = M4aSeek::open(reader, DecodeOptions::speech()).unwrap();
    source.reader.fail = true;
    assert!(source.decode(|_| Ok(())).is_err());
    assert!(source.restore_checkpoint(&checkpoint).is_err());
    source.reader.fail = false;
    source.seek(0).unwrap();
    assert!(!source.supports_checkpoint());
}
