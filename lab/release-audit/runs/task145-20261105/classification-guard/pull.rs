//! Bounded synchronous M4A pull, preserving the existing logical batch cadence.
use anyhow::Result;
use syom::{AacError, M4aSeek};

use super::super::stream::ChannelSelect;
use super::{AacBlock, BATCH_FRAMES, append_plane, input::Reader, map_aac, stream_options};

pub(super) struct Pull {
    source: M4aSeek<Reader>,
    channel: ChannelSelect,
    finished: bool,
}

/// The caller has already classified a bare, seekable M4A prefix.
pub(super) fn try_open(
    input: &mut Reader,
    channel: ChannelSelect,
    max_audio_secs: Option<f64>,
) -> Result<Option<Pull>> {
    let candidate = input.clone_for_pull()?;
    match M4aSeek::open(candidate, stream_options(max_audio_secs)) {
        Ok(source) => Ok(Some(Pull {
            source,
            channel,
            finished: false,
        })),
        Err(AacError::Unsupported(_)) => {
            // A failed open consumed the cloned descriptor's shared OS cursor.
            // Restore the original reader before the existing push fallback.
            input.force_rewind()?;
            Ok(None)
        }
        Err(error) => Err(map_aac(error)),
    }
}

impl Pull {
    pub(super) fn recv(&mut self) -> Result<Option<AacBlock>> {
        if self.finished {
            return Ok(None);
        }
        let mut block: Option<AacBlock> = None;
        let mut yielded = false;
        let result = self.source.decode(|frame| {
            if let Some(block) = &block
                && (block.rate, block.channels) != (frame.sample_rate, frame.planar.len())
            {
                return Err(AacError::decode("AAC rate or channel count changed"));
            }
            let block = block.get_or_insert_with(|| AacBlock {
                frames: 0,
                samples: Vec::with_capacity(BATCH_FRAMES),
                rate: frame.sample_rate,
                channels: frame.planar.len(),
            });
            block.frames += frame.samples;
            append_plane(&mut block.samples, &frame, self.channel);
            if block.frames >= BATCH_FRAMES {
                yielded = true;
                return Err(AacError::decode("AAC logical batch boundary"));
            }
            Ok(())
        });
        match result {
            Err(_) if yielded => Ok(block),
            Err(error) => {
                self.finished = true;
                Err(map_aac(error))
            }
            Ok(_) => {
                self.finished = true;
                Ok(block)
            }
        }
    }
}


#[cfg(test)] mod tests {
use super::*;
use bytes::Bytes;
use std::io::Write;
    fn fixture(rate: u32, channels: usize, he: bool) -> Vec<u8> {
        let n = rate as usize * 2 + 13;
        let planes: Vec<Vec<f32>> = (0..channels)
            .map(|ch| {
                (0..n)
                    .map(|i| 0.2 * (i as f32 * (0.07 + ch as f32 * 0.03)).sin())
                    .collect()
            })
            .collect();
        syom::encode_with(
            &planes,
            rate,
            &syom::EncodeOptions::m4a()
                .with_pns(true)
                .with_he(he)
                .with_bitrate_bps(48_000.min(rate * channels as u32 * 4)),
        )
        .unwrap()
    }
    fn collect(
        mut next: impl FnMut() -> Result<Option<AacBlock>>,
    ) -> Vec<(usize, u32, usize, Vec<u32>)> {
        let mut result = Vec::new();
        while let Some(block) = next().unwrap() {
            result.push((
                block.frames,
                block.rate,
                block.channels,
                block.samples.iter().map(|v| v.to_bits()).collect(),
            ));
        }
        result
    }
    #[test]
    fn test_aac_transport_classification_preserves_public_raw_id3_file_bytes_batches() {
        let pcm: Vec<_> = (0..96_013).map(|i| (i as f32 * 0.03).sin() * 0.1).collect();
        let tagged = syom::encode(&[&pcm], 48_000).unwrap();
        let at = if tagged.starts_with(b"ID3") {
            10 + tagged[6..10]
                .iter()
                .fold(0usize, |n, b| (n << 7) | usize::from(*b))
        } else {
            0
        };
        for bytes in [tagged[at..].to_vec(), tagged] {
            let mut worker = super::super::spawn_worker(
                Reader::bytes(Bytes::from(bytes.clone())),
                ChannelSelect::Mono,
                None,
            )
            .unwrap();
            let expected = collect(|| worker.recv());
            let mut file = tempfile::NamedTempFile::new().unwrap();
            file.write_all(&bytes).unwrap();
            let sources = [
                super::super::try_open_bytes(Bytes::from(bytes), ChannelSelect::Mono, None)
                    .unwrap()
                    .unwrap(),
                super::super::try_open_path(
                    file.path().to_str().unwrap(),
                    ChannelSelect::Mono,
                    None,
                )
                .unwrap()
                .unwrap(),
            ];
            for mut source in sources {
                let actual = collect(|| {
                    source.recv_block().map(|b| match b {
                        super::super::AacRecv::Block(block) => Some(block),
                        super::super::AacRecv::Eof => None,
                    })
                });
                assert_eq!(actual, expected);
            }
        }
    }
    #[test]
    fn test_aac_transport_classification_keeps_non_aac_mp4_fallthrough() {
        let mut bytes = fixture(48_000, 1, false);
        let at = bytes.windows(4).position(|w| w == b"mp4a").unwrap();
        bytes[at..at + 4].copy_from_slice(b"avc1");
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(&bytes).unwrap();
        assert!(
            super::super::try_open_bytes(Bytes::from(bytes), ChannelSelect::Mono, None)
                .unwrap()
                .is_none()
        );
        assert!(
            super::super::try_open_path(file.path().to_str().unwrap(), ChannelSelect::Mono, None)
                .unwrap()
                .is_none()
        );
    }
}
