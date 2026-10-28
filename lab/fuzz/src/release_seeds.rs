//! Release 0.8 seed families. Authored profiles follow engine/profile_tests.rs.
use syom::{DecodeOptions, EncodeOptions};

pub fn generated() -> Vec<(String, Vec<u8>)> {
    let pcm: Vec<f32> = (0..4096).map(|i| 0.2 * (i as f32 * 0.07).sin()).collect();
    let mut out = vec![(
        "generated-cbr".into(),
        syom::encode_with(&[&pcm], 48_000, &EncodeOptions::adts().with_cbr(true)).unwrap(),
    )];
    for aot in [1, 3, 4] {
        let raw = profile_au(aot);
        let size = raw.len() + 7;
        let header = [
            0xff,
            0xf1,
            ((aot - 1) << 6) | (3 << 2),
            0x40 | ((size >> 11) as u8),
            (size >> 3) as u8,
            ((size & 7) as u8) << 5 | 0x1f,
            0xfc,
        ];
        let mut bytes = Vec::new();
        for _ in 0..4 {
            bytes.extend(header);
            bytes.extend_from_slice(&raw);
        }
        // Ensure these seeds reach reconstruction before random mutations.
        let dec = syom::decode_with(&bytes, &DecodeOptions::audio()).unwrap();
        assert_eq!(dec.channels[0].len(), 4096);
        out.push((format!("authored-aot-{aot}"), bytes));
    }
    out
}

pub fn paths() -> [&'static str; 3] {
    [
        "lab/frame960/vectors/lc960-m48.loas",
        "lab/frame960/vectors/lc960-s48.loas",
        "lab/frame960/vectors/lc960-s48.m4a",
    ]
}

pub fn raw_cases() -> Vec<(Vec<u8>, Vec<u8>)> {
    let mut asc = Bits::default();
    for (v, n) in [(23, 5), (3, 4), (1, 4), (1, 1), (0, 4)] {
        asc.put(v, n);
    }
    let mut au = Bits::default();
    for (v, n) in [
        (0, 4),
        (100, 8),
        (0, 1),
        (0, 2),
        (0, 1),
        (1, 6),
        (0, 1),
        (1, 4),
        (1, 5),
        (0, 1),
        (0, 1),
        (0, 1),
        (0, 1),
        (0b10000, 5),
    ] {
        au.put(v, n);
    }
    vec![(asc.0, au.0)]
}

#[derive(Default)]
struct Bits(Vec<u8>, usize);
impl Bits {
    fn put(&mut self, v: u32, n: usize) {
        for i in (0..n).rev() {
            if self.1 % 8 == 0 {
                self.0.push(0);
            }
            let last = self.0.len() - 1;
            self.0[last] |= (((v >> i) & 1) as u8) << (7 - self.1 % 8);
            self.1 += 1;
        }
    }
}

fn profile_au(aot: u8) -> Vec<u8> {
    let mut w = Bits::default();
    for (v, n) in [(0, 3), (0, 4), (140, 8), (0, 1), (0, 2), (0, 1), (1, 6)] {
        w.put(v, n);
    }
    w.put(u32::from(aot != 3), 1);
    if aot == 1 {
        w.put(0, 1);
        w.put(1, 1);
    }
    if aot == 4 {
        w.put(1, 1);
        w.put(512, 11);
        w.put(4, 3);
        w.put(1, 1);
    }
    for (v, n) in [
        (1, 4),
        (1, 5),
        (0, 1),
        (0, 1),
        (0, 1),
        (0, 1),
        (0b10000, 5),
        (7, 3),
    ] {
        w.put(v, n);
    }
    w.0
}
