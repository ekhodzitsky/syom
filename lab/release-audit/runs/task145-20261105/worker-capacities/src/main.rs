#![allow(dead_code)]
use anyhow::Result;
struct Output {pcm:Vec<f32>,native:Vec<u32>,geometry:Vec<(usize,u32,usize)>}
#[path="snapshot/resample_original.rs"]mod resample;
mod audio {
 pub mod stream{#[derive(Clone,Copy)]pub enum ChannelSelect{Mono,One(usize)}}
 pub mod aac{
 use std::sync::mpsc::{self,Receiver,SyncSender};
 use std::thread::{self,JoinHandle};
 use anyhow::{Context,Result};
 use syom::{AacError,DecodeOptions,Frame};
 use super::stream::ChannelSelect;
 const MAX_SAMPLE_RATE:u32=192_000;
 #[derive(Debug)]enum GigasttError{AudioTooLong{observed_secs:f64,limit_secs:f64}}
 impl std::fmt::Display for GigasttError{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{write!(f,"{self:?}")}}
 impl std::error::Error for GigasttError{}
 static CAPACITY:std::sync::atomic::AtomicUsize=std::sync::atomic::AtomicUsize::new(1);
 #[path="/home/ekhodzitsky/Projects/syom/target/task145/worker-capacities/src/snapshot/aac/input.rs"]mod input;
 #[path="/home/ekhodzitsky/Projects/syom/target/task145/worker-capacities/src/snapshot/aac/pull.rs"]mod pull;
 #[path="/home/ekhodzitsky/Projects/syom/target/task145/worker-capacities/src/snapshot/aac/buffered.rs"]mod buffered;
 #[path="/home/ekhodzitsky/Projects/syom/target/task145/worker-capacities/src/snapshot/aac/probe.rs"]mod probe;
use input::{Reader,decode_reader};
const BATCH_FRAMES: usize = 24 * 1024;
type WorkerPacket = (AacBlock, Option<AacBlock>);
const WORKER_MESSAGE_CAPACITY: usize = 1;

pub(super) struct AacBlock {
    pub(super) frames: usize,
    pub(super) samples: Vec<f32>,
    rate: u32,
    channels: usize,
}

pub(super) enum AacRecv {
    Block(AacBlock),
    Eof,
}

/// Own the worker even during initialization and error exits.
struct Worker {
    rx: Option<Receiver<Result<WorkerPacket>>>,
    join: Option<JoinHandle<()>>,
    pending: Option<AacBlock>,
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.rx.take(); // Unblock a producer waiting to send before joining it.
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Worker {
    fn recv(&mut self) -> Result<Option<AacBlock>> {
        if let Some(block) = self.pending.take() {
            return Ok(Some(block));
        }
        let Some(rx) = &self.rx else { return Ok(None) };
        match rx.recv() {
            Ok(Ok((first, second))) => {
                self.pending = second;
                Ok(Some(first))
            }
            Ok(Err(error)) => Err(error),
            Err(_) => {
                self.rx.take();
                if let Some(join) = self.join.take() {
                    anyhow::ensure!(join.join().is_ok(), "AAC decode thread panicked");
                }
                Ok(None)
            }
        }
    }
}

// One queued packet holds at most two blocks. The producer holds at most two
// more (including a blocked send), and the receiver one pending second block.
// Including the actively consumed block, the bound is six logical blocks,
// versus four before packing: two extra blocks, still independent of duration.
struct BlockSender {
    tx: SyncSender<Result<WorkerPacket>>,
    held: Option<AacBlock>,
    first: bool,
}
impl BlockSender {
    fn send(&mut self, block: AacBlock) -> syom::Result<()> {
        if self.first {
            self.first = false;
            self.transmit((block, None))
        } else if let Some(first) = self.held.take() {
            self.transmit((first, Some(block)))
        } else {
            self.held = Some(block);
            Ok(())
        }
    }
    fn transmit(&self, packet: WorkerPacket) -> syom::Result<()> {
        self.tx
            .send(Ok(packet))
            .map_err(|_| AacError::decode("AAC consumer closed"))
    }
    fn flush(&mut self) -> syom::Result<()> {
        if let Some(block) = self.held.take() {
            self.transmit((block, None))?;
        }
        Ok(())
    }
    fn finish(mut self, result: syom::Result<()>, tail: Option<AacBlock>) {
        match result {
            Ok(()) => {
                if let Some(block) = tail
                    && self.send(block).is_err()
                {
                    return;
                }
                let _ = self.flush();
            }
            Err(error) => {
                // Completed logical blocks precede errors; incomplete PCM is discarded.
                if self.flush().is_ok() {
                    let _ = self.tx.send(Err(map_aac(error)));
                }
            }
        }
    }
}
fn spawn_worker(
    mut input: Reader,
    channel: ChannelSelect,
    max_audio_secs: Option<f64>,
) -> Result<Worker> {
    let (tx, rx) = mpsc::sync_channel(CAPACITY.load(std::sync::atomic::Ordering::Relaxed));
    let join = thread::Builder::new()
        .name("gigastt-aac".into())
        .spawn(move || {
            let mut sender = BlockSender {
                tx,
                held: None,
                first: true,
            };
            let mut batch: Option<AacBlock> = None;
            let result = decode_reader(&mut input, stream_options(max_audio_secs), |frame| {
                if let Some(block) = &batch
                    && (block.rate, block.channels) != (frame.sample_rate, frame.planar.len())
                {
                    return Err(AacError::decode("AAC rate or channel count changed"));
                }
                let block = batch.get_or_insert_with(|| AacBlock {
                    frames: 0,
                    samples: Vec::with_capacity(BATCH_FRAMES),
                    rate: frame.sample_rate,
                    channels: frame.planar.len(),
                });
                block.frames += frame.samples;
                append_plane(&mut block.samples, &frame, channel);
                if block.frames >= BATCH_FRAMES
                    && let Some(block) = batch.take()
                {
                    sender.send(block)?;
                }
                Ok(())
            });
            sender.finish(result, batch);
        })
        .context("Failed to start AAC decode thread")?;
    Ok(Worker {
        rx: Some(rx),
        join: Some(join),
        pending: None,
    })
}
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

#[cfg(test)]
mod worker_packet_tests {
    use super::*;

    fn block(id: usize, frames: usize) -> AacBlock {
        AacBlock {
            frames,
            samples: vec![id as f32, -0.0],
            rate: 48_000,
            channels: 1,
        }
    }
    fn worker(count: usize, tail: bool, fail: bool) -> Worker {
        let (tx, rx) = mpsc::sync_channel(WORKER_MESSAGE_CAPACITY);
        let join = thread::spawn(move || {
            let mut sender = BlockSender {
                tx,
                held: None,
                first: true,
            };
            for i in 0..count {
                if sender.send(block(i, BATCH_FRAMES)).is_err() {
                    return;
                }
            }
            let result = if fail {
                Err(AacError::decode("original decoder failure"))
            } else {
                Ok(())
            };
            sender.finish(result, tail.then(|| block(count, 137)));
        });
        Worker {
            rx: Some(rx),
            join: Some(join),
            pending: None,
        }
    }

    #[test]
    fn test_aac_worker_packets_preserve_odd_even_full_partial_eof() {
        for count in 0..6 {
            for tail in [false, true] {
                let mut worker = worker(count, tail, false);
                for i in 0..count + usize::from(tail) {
                    let actual = worker.recv().unwrap().unwrap();
                    assert_eq!(actual.frames, if i < count { BATCH_FRAMES } else { 137 });
                    assert_eq!(
                        actual
                            .samples
                            .iter()
                            .map(|s| s.to_bits())
                            .collect::<Vec<_>>(),
                        [i as f32, -0.0].map(f32::to_bits)
                    );
                }
                assert!(worker.recv().unwrap().is_none());
            }
        }
    }

    #[test]
    fn test_aac_worker_packets_flush_completed_blocks_before_typed_error() {
        for count in 0..6 {
            let mut worker = worker(count, true, true);
            for i in 0..count {
                assert_eq!(worker.recv().unwrap().unwrap().samples[0], i as f32);
            }
            let error = worker.recv().err().unwrap();
            assert!(error.downcast_ref::<AacError>().is_some());
            assert!(error.to_string().contains("original decoder failure"));
            assert!(
                worker.recv().unwrap().is_none(),
                "partial tail must be discarded"
            );
        }
    }

    #[test]
    fn test_aac_worker_packets_keep_first_block_immediate_and_drop_unblocks_sender() {
        let mut worker = worker(1000, true, false);
        assert_eq!(worker.recv().unwrap().unwrap().samples[0], 0.0);
        assert!(worker.pending.is_none(), "the first block is sent alone");
        let (done, completion) = mpsc::channel();
        thread::spawn(move || {
            drop(worker);
            done.send(()).unwrap();
        });
        completion
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
    }

    #[test]
    fn test_aac_worker_packet_queue_has_explicit_two_block_bound() {
        let (tx, _rx) = mpsc::sync_channel::<Result<WorkerPacket>>(WORKER_MESSAGE_CAPACITY);
        assert!(
            tx.try_send(Ok((block(0, BATCH_FRAMES), Some(block(1, BATCH_FRAMES)))))
                .is_ok()
        );
        assert!(matches!(
            tx.try_send(Ok((block(2, BATCH_FRAMES), None))),
            Err(mpsc::TrySendError::Full(_))
        ));
    }
}

 pub fn decode(path:&str,capacity:usize,verify:bool)->Result<crate::Output>{
  CAPACITY.store(capacity,std::sync::atomic::Ordering::Relaxed);
  let mut file=std::fs::File::open(path)?;
  anyhow::ensure!(probe::sniff_file(&mut file)?,"not AAC");
  let mut input=Reader::File(file);
  anyhow::ensure!(pull::try_open(&mut input,ChannelSelect::Mono,None)?.is_none(),"ADTS/LOAS only");
  let mut worker=spawn_worker(input,ChannelSelect::Mono,None)?;
  let first=worker.recv()?.context("no PCM")?;
  let mut resampler=crate::resample::ResampleTo16k::new(crate::resample::SampleRate(first.rate),None);
  let mut result=crate::Output{pcm:Vec::new(),native:Vec::new(),geometry:Vec::new()};
  let mut next=Some(first);
  while let Some(block)=next{
   if verify{
    result.native.extend(block.samples.iter().map(|v|v.to_bits()));
    result.geometry.push((block.frames,block.rate,block.channels));
   }
   if !block.samples.is_empty(){resampler.stage().extend_from_slice(&block.samples);resampler.flush_full()?;}
   resampler.drain_ready_into(&mut result.pcm);
   next=worker.recv()?;
  }
  resampler.finish_into(&mut result.pcm)?;
  Ok(result)
 }
 }
}
fn main()->Result<()>{
 let args:Vec<String>=std::env::args().collect();let mode=&args[1];let file=&args[2];
 if mode=="verify"{
  let base=audio::aac::decode(file,1,true)?;
  for capacity in [1,2,4]{
   let got=audio::aac::decode(file,capacity,true)?;
   anyhow::ensure!(base.geometry==got.geometry,"batch geometry");
   anyhow::ensure!(base.native==got.native,"native bits");
   anyhow::ensure!(base.pcm.len()==got.pcm.len(),"output length");
   anyhow::ensure!(base.pcm.iter().zip(&got.pcm).all(|(a,b)|a.to_bits()==b.to_bits()),"output bits");
   println!("{}",serde_json::json!({"capacity":capacity,"geometry":got.geometry,"native_samples":got.native.len(),"samples":got.pcm.len(),"bit_differences":0,"max_logical_blocks":2*capacity+4}));
   if let Some(prefix)=args.get(3){
    use std::io::Write;
    let mut out=std::io::BufWriter::new(std::fs::File::create(format!("{prefix}.cap{capacity}.f32"))?);
    for sample in got.pcm{out.write_all(&sample.to_le_bytes())?;}out.flush()?;
   }
  }
  return Ok(());
 }
 let capacity:usize=mode.parse()?;anyhow::ensure!([1,2,4].contains(&capacity),"capacity");
 let reps=args.get(3).map(|s|s.parse()).transpose()?.unwrap_or(21);
 let mut samples=Vec::new();for i in 0..reps+2{
  let start=std::time::Instant::now();std::hint::black_box(audio::aac::decode(file,capacity,false)?);
  if i>=2{samples.push(start.elapsed().as_secs_f64()*1e6);}
 }
 println!("{}",serde_json::json!({"capacity":capacity,"microseconds":samples,"max_logical_blocks":2*capacity+4}));Ok(())
}
