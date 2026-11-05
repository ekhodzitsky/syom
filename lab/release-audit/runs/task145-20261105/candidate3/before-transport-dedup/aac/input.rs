//! Shared callback driver for windowed decode and synchronous channel scans.
use bytes::Bytes;
use std::io::{Cursor, Read, Seek, SeekFrom};
use syom::{AacError, DecodeOptions, Decoder, Frame, M4aSeek};

use super::buffered::SmartBuffered;

pub(super) enum Reader {
    File(std::fs::File),
    Buffered(SmartBuffered<std::fs::File>),
    Mem(Cursor<Bytes>),
}
impl Reader {
    pub(super) fn bytes(data: Bytes) -> Self {
        Self::Mem(Cursor::new(data))
    }

    pub(super) fn clone_for_pull(&self) -> std::io::Result<Self> {
        let mut copy = match self {
            Self::Mem(cursor) => Self::bytes(cursor.get_ref().clone()),
            Self::File(file) => Self::Buffered(SmartBuffered::new(file.try_clone()?)),
            Self::Buffered(file) => Self::Buffered(SmartBuffered::new(file.get_ref().try_clone()?)),
        };
        copy.force_rewind()?;
        Ok(copy)
    }

    // File::try_clone shares the OS cursor. Always issue an absolute rewind,
    // even when a buffer's cached logical position already claims to be zero.
    pub(super) fn force_rewind(&mut self) -> std::io::Result<()> {
        match self {
            Self::Buffered(file) => file.force_rewind(),
            other => other.seek(SeekFrom::Start(0)).map(|_| ()),
        }
    }
}
impl Read for Reader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::File(file) => file.read(buf),
            Self::Buffered(file) => file.read(buf),
            Self::Mem(cursor) => cursor.read(buf),
        }
    }
}
impl Seek for Reader {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        match self {
            Self::File(file) => file.seek(pos),
            Self::Buffered(file) => file.seek(pos),
            Self::Mem(cursor) => cursor.seek(pos),
        }
    }
}

pub(super) fn decode_reader(
    reader: &mut Reader,
    opts: DecodeOptions,
    mut emit: impl FnMut(Frame<'_>) -> syom::Result<()>,
) -> syom::Result<()> {
    let mut prefix = [0; 64];
    let n = reader.read(&mut prefix)?;
    reader.seek(SeekFrom::Start(0))?;
    if syom::sniff_is_isobmff(&prefix[..n]) {
        match M4aSeek::open(&mut *reader, opts.clone()) {
            Ok(mut seek) => return seek.decode(&mut emit).map(|_| ()),
            Err(AacError::Unsupported(_)) => {} // Bounded fragmented MP4 uses the push decoder.
            Err(error) => return Err(error),
        }
        reader.seek(SeekFrom::Start(0))?;
    }
    let mut decoder = Decoder::new(opts);
    decoder.feed_read(reader, &mut emit)?;
    decoder.finish(emit).map(|_| ())
}
