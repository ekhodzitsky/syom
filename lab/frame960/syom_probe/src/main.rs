//! TASK-63 lab probe: current syom behavior on 960-frame (`frameLengthFlag=1`)
//! AAC-LC vectors in LATM/LOAS and M4A. Lab only; never run by cargo test.
//! Prints one JSON line per lane.

use std::fmt::Write as _;

fn err_name(e: &syom::AacError) -> String {
    format!("{e:?}")
}

fn probe_one(label: &str, path: &str) -> String {
    let data = std::fs::read(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let mut out = String::new();
    let _ = write!(out, "{{\"vector\":\"{label}\"");

    let _ = write!(
        out,
        ",\"sniff_adts\":{},\"sniff_latm\":{},\"sniff_isobmff\":{}",
        syom::sniff_is_adts(&data),
        syom::sniff_is_latm(&data),
        syom::sniff_is_isobmff(&data)
    );

    match syom::probe(&data) {
        Ok(p) => {
            let _ = write!(out, ",\"probe\":\"ok {:?}", p);
        }
        Err(e) => {
            let _ = write!(out, ",\"probe_error\":\"{}\"", err_name(&e));
        }
    }

    match syom::decode(&data) {
        Ok(d) => {
            let _ = write!(out, ",\"decode\":\"ok {} channels\"", d.channels.len());
        }
        Err(e) => {
            let _ = write!(out, ",\"decode_error\":\"{}\"", err_name(&e));
        }
    }

    // push Decoder lane
    let mut dec = syom::Decoder::new(syom::DecodeOptions::speech());
    let mut frames = 0usize;
    match dec.feed(&data, |_| {
        frames += 1;
        Ok(())
    }) {
        Ok(consumed) => {
            let _ = write!(out, ",\"push\":\"ok consumed={consumed} frames={frames}\"");
        }
        Err(e) => {
            let _ = write!(out, ",\"push_error\":\"{}\"", err_name(&e));
        }
    }

    out.push('}');
    out
}

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| "../vectors".into());
    for (label, file) in [
        ("lc960-m48.loas", "lc960-m48.loas"),
        ("lc960-s48.loas", "lc960-s48.loas"),
        ("lc960-s48.m4a", "lc960-s48.m4a"),
    ] {
        println!("{}", probe_one(label, &format!("{dir}/{file}")));
    }
}
