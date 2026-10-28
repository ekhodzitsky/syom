//! Decode-only example compiled in an isolated snapshot of the actual consumer.
use std::time::Instant;
use bytes::Bytes;
use gigastt_core::inference::audio::{AudioChunks, decode_audio_bytes_shared};
use serde_json::json;

fn update(h: &mut u64, p: &[f32]) {
    for &x in p {
        assert!(x.is_finite());
        for b in x.to_bits().to_le_bytes() { *h=(*h ^ u64::from(b)).wrapping_mul(0x100000001b3); }
    }
}
fn percentile(v: &[f64], q: f64) -> f64 {
    let mut s=v.to_vec();s.sort_by(f64::total_cmp);s[((s.len()-1) as f64*q).ceil() as usize]
}
fn main() -> anyhow::Result<()> {
    let args: Vec<_>=std::env::args().collect();
    let mode=args.get(1).ok_or_else(||anyhow::anyhow!("MODE INPUT OUT [REPS]"))?;
    let input=args.get(2).ok_or_else(||anyhow::anyhow!("missing INPUT"))?;
    let out=args.get(3).ok_or_else(||anyhow::anyhow!("missing OUT"))?;
    let data=Bytes::from(std::fs::read(input)?);
    let mut hash=0xcbf29ce484222325;
    let mut samples=0;
    let mut times=Vec::new();
    let started=Instant::now();
    let mut first=None;
    if mode=="stream" {
        let mut chunks=AudioChunks::from_bytes(data.clone(),16000,None)?;
        loop {
            let t=Instant::now();
            let Some(p)=chunks.next_chunk()? else { break; };
            times.push(t.elapsed().as_secs_f64()*1000.0);
            if first.is_none() { first=Some(started.elapsed().as_secs_f64()*1000.0); }
            samples+=p.len();update(&mut hash,p);
        }
    } else {
        let pcm=decode_audio_bytes_shared(data.clone())?;
        samples=pcm.len();update(&mut hash,&pcm);drop(pcm);
        let reps=args.get(4).map(|s|s.parse::<usize>()).transpose()?.unwrap_or(101);
        for i in 0..reps+4 {
            let t=Instant::now();
            let pcm=std::hint::black_box(decode_audio_bytes_shared(data.clone())?);
            assert_eq!(pcm.len(),samples);drop(pcm);
            if i>=4 { times.push(t.elapsed().as_secs_f64()*1000.0); }
        }
    }
    let memory=std::fs::read_to_string("/proc/self/status")?.lines()
        .filter(|l|l.starts_with("VmRSS:")||l.starts_with("VmHWM:"))
        .collect::<Vec<_>>().join("; ");
    let result=json!({"mode":mode,"source_bytes":data.len(),"samples_16k":samples,
        "hash":format!("{hash:016x}"),"first_output_ms":first,"process_memory":memory,
        "reps":times.len(),"median_ms":percentile(&times,0.5),
        "p95_ms":percentile(&times,0.95),"p99_ms":percentile(&times,0.99),"raw_ms":times});
    std::fs::write(out,serde_json::to_vec_pretty(&result)?)?;
    println!("{}",json!({"mode":mode,"samples":samples,"hash":format!("{hash:016x}")}));
    Ok(())
}
