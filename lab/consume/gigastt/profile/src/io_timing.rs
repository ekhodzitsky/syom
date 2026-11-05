use std::{
    fs::File,
    hint::black_box,
    io::{self, BufReader, Cursor, Read, Seek, SeekFrom},
    time::Instant,
};
struct Smart<R> {
    inner: BufReader<R>,
    position: Option<u64>,
}
impl<R: Read> Read for Smart<R> {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        match self.inner.read(b) {
            Ok(n) => {
                self.position = self.position.and_then(|p| p.checked_add(n as u64));
                Ok(n)
            }
            Err(e) => {
                self.position = None;
                Err(e)
            }
        }
    }
}
impl<R: Read + Seek> Seek for Smart<R> {
    fn seek(&mut self, p: SeekFrom) -> io::Result<u64> {
        let result = match (p, self.position) {
            (SeekFrom::Start(at), Some(now)) => {
                let delta = i128::from(at) - i128::from(now);
                match i64::try_from(delta) {
                    Ok(delta) => self.inner.seek_relative(delta).map(|()| at),
                    Err(_) => self.inner.seek(p),
                }
            }
            _ => self.inner.seek(p),
        };
        match result {
            Ok(at) => {
                self.position = Some(at);
                Ok(at)
            }
            Err(e) => {
                self.position = None;
                Err(e)
            }
        }
    }
}
fn smart<R: Read + Seek>(r: R, n: usize) -> Smart<R> {
    Smart {
        inner: BufReader::with_capacity(n, r),
        position: Some(0),
    }
}
fn consume<R: Read + Seek>(mut reader: R, collect: bool) -> Vec<f32> {
    let mut out = Vec::with_capacity(24576);
    let mut prefix = [0; 64];
    let n = reader.read(&mut prefix).unwrap();
    reader.seek(SeekFrom::Start(0)).unwrap();
    let mut emit = |f: syom::Frame<'_>| {
        let at = out.len();
        out.extend_from_slice(f.planar[0]);
        for plane in f.planar.iter().skip(1) {
            for (x, y) in out[at..].iter_mut().zip(plane.iter()) {
                *x += y;
            }
        }
        let count = f.planar.len() as f32;
        for x in &mut out[at..] {
            *x /= count;
        }
        if !collect && out.len() >= 24576 {
            black_box(&out);
            out.clear();
        }
        Ok(())
    };
    if syom::sniff_is_isobmff(&prefix[..n]) {
        let mut src = syom::M4aSeek::open(reader, syom::DecodeOptions::unbounded()).unwrap();
        src.decode(&mut emit).unwrap();
    } else {
        syom::decode_read_streaming(reader, &syom::DecodeOptions::unbounded(), emit).unwrap();
    }
    black_box(&out);
    out
}
fn decode(file: &str, data: &[u8], kind: &str, collect: bool) -> Vec<f32> {
    match kind {
        "file" => consume(File::open(file).unwrap(), collect),
        "bytes" => consume(Cursor::new(data), collect),
        "buffer8k" => consume(
            BufReader::with_capacity(8192, File::open(file).unwrap()),
            collect,
        ),
        "smart8k" => consume(smart(File::open(file).unwrap(), 8192), collect),
        "smart64k" => consume(smart(File::open(file).unwrap(), 65536), collect),
        _ => panic!("kind"),
    }
}
fn seek_tests() {
    let bytes: Vec<u8> = (0..200).map(|v| v as u8).collect();
    let mut reference = Cursor::new(&bytes);
    let mut actual = smart(Cursor::new(&bytes), 32);
    for step in [
        SeekFrom::Start(0),
        SeekFrom::Start(4),
        SeekFrom::Current(-2),
        SeekFrom::End(-12),
        SeekFrom::Start(4),
        SeekFrom::Current(74),
        SeekFrom::Start(199),
    ] {
        assert_eq!(reference.seek(step).unwrap(), actual.seek(step).unwrap());
        let (mut a, mut b) = ([0; 8], [0; 8]);
        let n = reference.read(&mut a).unwrap();
        let m = actual.read(&mut b).unwrap();
        assert_eq!(n, m);
        assert_eq!(a, b);
    }
    struct Fail {
        cursor: Cursor<Vec<u8>>,
        read_error: bool,
        seek_error: bool,
    }
    impl Read for Fail {
        fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
            if std::mem::take(&mut self.read_error) {
                return Err(io::Error::other("injected read"));
            }
            self.cursor.read(b)
        }
    }
    impl Seek for Fail {
        fn seek(&mut self, p: SeekFrom) -> io::Result<u64> {
            if std::mem::take(&mut self.seek_error) {
                return Err(io::Error::other("injected seek"));
            }
            self.cursor.seek(p)
        }
    }
    let mut s = smart(
        Fail {
            cursor: Cursor::new(bytes),
            read_error: true,
            seek_error: false,
        },
        32,
    );
    assert!(s.read(&mut [0; 1]).is_err());
    assert!(s.position.is_none());
    assert_eq!(s.seek(SeekFrom::Start(4)).unwrap(), 4);
    let mut b = [0; 2];
    s.read_exact(&mut b).unwrap();
    assert_eq!(b, [4, 5]);
    s.inner.get_mut().seek_error = true;
    assert!(s.seek(SeekFrom::End(-1)).is_err());
    assert!(s.position.is_none());
    assert_eq!(s.seek(SeekFrom::Start(4)).unwrap(), 4);
    s.read_exact(&mut b).unwrap();
    assert_eq!(b, [4, 5]);
}
fn main() {
    seek_tests();
    let args: Vec<String> = std::env::args().collect();
    let file = &args[2];
    let data = std::fs::read(file).unwrap();
    if args[1] == "verify" {
        let base = decode(file, &data, "file", true);
        for kind in ["bytes", "buffer8k", "smart8k", "smart64k"] {
            let other = decode(file, &data, kind, true);
            let diff = base
                .iter()
                .zip(&other)
                .filter(|(a, b)| a.to_bits() != b.to_bits())
                .count();
            assert_eq!(base.len(), other.len());
            assert_eq!(diff, 0);
            println!("{{\"file\":\"{file}\",\"kind\":\"{kind}\",\"samples\":{},\"differing_bits\":{diff},\"seek_error_tests\":\"pass\"}}",base.len());
        }
        return;
    }
    let kind = &args[1];
    let mut times = Vec::new();
    for i in 0..9 {
        let now = Instant::now();
        black_box(decode(file, &data, kind, false));
        if i >= 2 {
            times.push(now.elapsed().as_secs_f64() * 1e3);
        }
    }
    let raw = times
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",");
    times.sort_by(f64::total_cmp);
    println!(
        "{{\"kind\":\"{kind}\",\"median_ms\":{},\"samples_ms\":[{raw}]}}",
        times[3]
    );
}
