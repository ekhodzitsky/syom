use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    fs::File,
    io::{self, BufReader, Cursor, Read, Seek, SeekFrom},
    rc::Rc,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};
use syom::{ChannelMode, DecodeOptions, M4aSeek};
static ENABLE: AtomicBool = AtomicBool::new(false);
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);
static REALLOCS: AtomicUsize = AtomicUsize::new(0);
struct Counter;
unsafe impl GlobalAlloc for Counter {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        if ENABLE.load(Ordering::Relaxed) {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
            BYTES.fetch_add(l.size(), Ordering::Relaxed);
        }
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        if ENABLE.load(Ordering::Relaxed) {
            REALLOCS.fetch_add(1, Ordering::Relaxed);
            BYTES.fetch_add(n, Ordering::Relaxed);
        }
        unsafe { System.realloc(p, l, n) }
    }
}
#[global_allocator]
static A: Counter = Counter;
#[derive(Default)]
struct Calls {
    reads: Cell<usize>,
    seeks: Cell<usize>,
    bytes: Cell<usize>,
}
struct Count<R> {
    inner: R,
    calls: Rc<Calls>,
}
impl<R: Read> Read for Count<R> {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        self.calls.reads.set(self.calls.reads.get() + 1);
        let n = self.inner.read(b)?;
        self.calls.bytes.set(self.calls.bytes.get() + n);
        Ok(n)
    }
}
impl<R: Seek> Seek for Count<R> {
    fn seek(&mut self, p: SeekFrom) -> io::Result<u64> {
        self.calls.seeks.set(self.calls.seeks.get() + 1);
        self.inner.seek(p)
    }
}
// Diagnostic adapter: rebase absolute seeks through known logical position.
// BufReader::seek_relative preserves its buffer for in-window seeks.
struct Smart<R> {
    inner: BufReader<R>,
    position: u64,
}
impl<R: Read> Read for Smart<R> {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(b)?;
        self.position += n as u64;
        Ok(n)
    }
}
impl<R: Read + Seek> Seek for Smart<R> {
    fn seek(&mut self, p: SeekFrom) -> io::Result<u64> {
        if let SeekFrom::Start(at) = p {
            let delta = i128::from(at) - i128::from(self.position);
            if let Ok(delta) = i64::try_from(delta) {
                self.inner.seek_relative(delta)?;
                self.position = at;
                return Ok(at);
            }
        }
        self.position = self.inner.seek(p)?;
        Ok(self.position)
    }
}
fn run<R: Read + Seek>(reader: R, mode: &str, calls: Rc<Calls>) {
    ALLOCS.store(0, Ordering::Relaxed);
    BYTES.store(0, Ordering::Relaxed);
    REALLOCS.store(0, Ordering::Relaxed);
    ENABLE.store(true, Ordering::SeqCst);
    let mut opts = DecodeOptions::unbounded();
    if mode == "mono" {
        opts.channel_mode = ChannelMode::Mono;
    }
    let mut src = M4aSeek::open(reader, opts).unwrap();
    let mut out = Vec::with_capacity(24576);
    let info = src
        .decode(|f| {
            if mode == "manual" {
                let at = out.len();
                out.extend_from_slice(f.planar[0]);
                for plane in f.planar.iter().skip(1) {
                    for (x, y) in out[at..].iter_mut().zip(plane.iter()) {
                        *x += y;
                    }
                }
                let n = f.planar.len() as f32;
                for x in &mut out[at..] {
                    *x /= n;
                }
            } else if mode == "mono" {
                out.extend_from_slice(f.planar[0]);
            } else {
                std::hint::black_box(f.planar);
            }
            if out.len() >= 24576 {
                std::hint::black_box(&out);
                out.clear();
            }
            Ok(())
        })
        .unwrap();
    ENABLE.store(false, Ordering::SeqCst);
    println!("{{\"allocs\":{},\"reallocs\":{},\"allocated_bytes\":{},\"reader_reads\":{},\"reader_seeks\":{},\"reader_bytes\":{},\"aac_frames\":{},\"samples\":{}}}",ALLOCS.load(Ordering::Relaxed),REALLOCS.load(Ordering::Relaxed),BYTES.load(Ordering::Relaxed),calls.reads.get(),calls.seeks.get(),calls.bytes.get(),info.aac_frames,info.samples);
}
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let file = &args[1];
    let kind = &args[2];
    let mode = &args[3];
    let calls = Rc::new(Calls::default());
    if kind == "bytes" {
        run(
            Count {
                inner: Cursor::new(std::fs::read(file).unwrap()),
                calls: calls.clone(),
            },
            mode,
            calls,
        );
    } else {
        let src = Count {
            inner: File::open(file).unwrap(),
            calls: calls.clone(),
        };
        match kind.as_str() {
            "file" => run(src, mode, calls),
            "buffer8k" => run(BufReader::with_capacity(8192, src), mode, calls),
            "smart8k" => run(
                Smart {
                    inner: BufReader::with_capacity(8192, src),
                    position: 0,
                },
                mode,
                calls,
            ),
            "smart64k" => run(
                Smart {
                    inner: BufReader::with_capacity(65536, src),
                    position: 0,
                },
                mode,
                calls,
            ),
            _ => panic!("kind"),
        }
    }
}
