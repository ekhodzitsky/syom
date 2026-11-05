#![allow(dead_code)]
mod adapter {
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread::{self, JoinHandle};
use anyhow::Result;
use syom::AacError;
#[derive(Debug)]
enum GigasttError { AudioTooLong { observed_secs: f64, limit_secs: f64 } }
impl std::fmt::Display for GigasttError {
 fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{self:?}") }
}
impl std::error::Error for GigasttError {}
// Logical cuts never change: the resampler observes each block individually.
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


}
