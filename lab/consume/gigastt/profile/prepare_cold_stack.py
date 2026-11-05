#!/usr/bin/env python3
"""Prepare exact frozen consumer modules for a model-free 128 KiB cold-stack proof.

Preparation only: follow README for the explicitly scheduled isolated build/run.
"""
import argparse
import hashlib
import json
from pathlib import Path

TEMPLATE = r'''
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
        #[path=@@BUFFERED@@] mod buffered;
        #[path=@@INPUT@@] mod input;
        #[path=@@PULL@@] mod pull;
@@HELPERS@@
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
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--consumer", type=Path, default=Path("target/task145/gigastt"))
    parser.add_argument("--output", type=Path, default=Path("target/task145/stack-proof-repro"))
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[4]
    audio = args.consumer.resolve() / "crates/gigastt-core/src/inference/audio"
    output = args.output.resolve()
    output.joinpath("src").mkdir(parents=True, exist_ok=True)
    aac = audio.joinpath("aac.rs").read_text()
    helpers = aac[aac.index("fn append_plane("):aac.index("\n#[cfg(test)]\nmod mono_plane_tests")]
    source = TEMPLATE.replace("@@HELPERS@@", helpers)
    files = [audio / "aac.rs"]
    for module in ["buffered", "input", "pull"]:
        path = audio / "aac" / (module + ".rs")
        files.append(path)
        source = source.replace("@@" + module.upper() + "@@", json.dumps(str(path)))
    output.joinpath("src/main.rs").write_text(source)
    output.joinpath("Cargo.toml").write_text(
        '[package]\nname="syom-pull-stack-proof"\nversion="0.0.0"\nedition="2024"\npublish=false\n'
        '[workspace]\n[dependencies]\nsyom={path=' + json.dumps(str(root)) + '}\n'
        'anyhow="=1.0.104"\nbytes="=1.12.1"\n'
        '[profile.release]\nlto=true\ncodegen-units=1\nopt-level=3\n')
    output.joinpath("Cargo.lock").write_bytes(Path(__file__).with_name("cold_stack.lock").read_bytes())
    files.extend(root / "src/goldens" / fixture for fixture in ["he48.m4a", "he48em.m4a", "he2_48em.m4a", "ps48.m4a"])
    output.joinpath("source-manifest.json").write_text(json.dumps(
        {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in files}, indent=2) + "\n")
    print(output / "Cargo.toml")


if __name__ == "__main__":
    main()
