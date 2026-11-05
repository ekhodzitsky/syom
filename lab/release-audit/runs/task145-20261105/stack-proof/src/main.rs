#![allow(dead_code)]
use anyhow::Result;
mod audio {
    pub const MAX_SAMPLE_RATE:u32=192_000;
    pub mod stream { #[derive(Clone,Copy)] pub enum ChannelSelect { Mono, One(usize) } }
    pub mod aac {
        use syom::{AacError, DecodeOptions, Frame};
        use super::{MAX_SAMPLE_RATE,stream::ChannelSelect};
        #[derive(Debug)]
        enum GigasttError { AudioTooLong { observed_secs:f64, limit_secs:f64 } }
        impl std::fmt::Display for GigasttError { fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result {write!(f,"{self:?}")} }
        impl std::error::Error for GigasttError {}
        const BATCH_FRAMES:usize=24*1024;
        pub struct AacBlock {frames:usize,samples:Vec<f32>,rate:u32,channels:usize}
        #[path="/home/ekhodzitsky/Projects/syom/target/task145/gigastt/crates/gigastt-core/src/inference/audio/aac/buffered.rs"] mod buffered;
        #[path="/home/ekhodzitsky/Projects/syom/target/task145/gigastt/crates/gigastt-core/src/inference/audio/aac/input.rs"] mod input;
        #[path="/home/ekhodzitsky/Projects/syom/target/task145/gigastt/crates/gigastt-core/src/inference/audio/aac/pull.rs"] mod pull;
fn append_plane(out: &mut Vec<f32>, frame: &Frame<'_>, channel: ChannelSelect) {
    match channel {
        ChannelSelect::One(index) => {
            if let Some(plane) = frame.planar.get(index) {
                out.extend_from_slice(plane);
            }
        }
        ChannelSelect::Mono => append_mono_planes(out, frame.planar),
    }
}

fn stream_options(max_audio_secs: Option<f64>) -> DecodeOptions {
    let mut opts = DecodeOptions::unbounded().with_max_sample_rate(MAX_SAMPLE_RATE);
    if let Some(secs) = max_audio_secs.filter(|secs| secs.is_finite() && *secs > 0.0) {
        opts = opts.with_max_duration_secs(secs);
    }
    opts
}

fn map_aac(error: AacError) -> anyhow::Error {
    match error {
        AacError::TooLong {
            observed_secs,
            max_secs,
        } => GigasttError::AudioTooLong {
            observed_secs,
            limit_secs: max_secs,
        }
        .into(),
        other => other.into(),
    }
}

// Preserve the existing add-then-divide order, with one output pass for stereo.
// Unequal planes retain the defensive historical zip behavior.
fn append_mono_planes(out: &mut Vec<f32>, planes: &[&[f32]]) {
    if let [left, right] = planes
        && left.len() == right.len()
    {
        out.extend(left.iter().zip(*right).map(|(a, b)| (*a + *b) / 2.0));
        return;
    }
    let Some(first) = planes.first() else { return };
    let start = out.len();
    out.extend_from_slice(first);
    for plane in planes.iter().skip(1) {
        for (sample, value) in out[start..].iter_mut().zip(plane.iter()) {
            *sample += value;
        }
    }
    let count = planes.len() as f32;
    for sample in &mut out[start..] {
        *sample /= count;
    }
}

        pub fn collect(path:&str,kind:&str)->anyhow::Result<Vec<(usize,u32,usize,Vec<u32>)>> {
            let mut reader=match kind {
                "file"=>input::Reader::File(std::fs::File::open(path)?),
                "bytes"=>input::Reader::bytes(bytes::Bytes::from(std::fs::read(path)?)),
                _=>anyhow::bail!("unknown reader kind")
            };
            let mut pull=Box::new(pull::try_open(&mut reader,ChannelSelect::Mono,None)?.ok_or_else(||anyhow::anyhow!("not M4A pull"))?);
            let mut blocks=Vec::new();
            while let Some(block)=pull.recv()? {
                anyhow::ensure!(block.samples.iter().all(|s|s.is_finite()),"nonfinite");
                blocks.push((block.frames,block.rate,block.channels,block.samples.into_iter().map(f32::to_bits).collect()));
            }
            anyhow::ensure!(!blocks.is_empty(),"empty decode");
            anyhow::ensure!(pull.recv()?.is_none(),"nonterminal EOF");
            Ok(blocks)
        }
    }
}
fn main()->Result<()> {
    let args:Vec<String>=std::env::args().collect();
    anyhow::ensure!(args.len()==3,"usage: fixture file|bytes");
    let (path,kind)=(args[1].clone(),args[2].clone());
    let cold=std::thread::Builder::new().name("cold-pull".into()).stack_size(128*1024)
        .spawn(move||audio::aac::collect(&path,&kind))?.join().map_err(|_|anyhow::anyhow!("cold thread panicked"))??;
    let warm=audio::aac::collect(&args[1],&args[2])?;
    anyhow::ensure!(cold==warm,"cold/warm native geometry or bits differ");
    println!("PASS stack_bytes=131072 fixture={} reader={} blocks={} samples={} bit_differences=0",args[1],args[2],cold.len(),cold.iter().map(|b|b.3.len()).sum::<usize>());
    Ok(())
}
