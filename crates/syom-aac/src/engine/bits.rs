//! MSB-first bit I/O for AAC (ISO/IEC 14496-3 bitstreams).

use super::error::{Error, Result};

/// MSB-first reader. High bit of each byte is consumed first.
#[derive(Clone, Copy)]
pub struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> BitReader<'a> {
    /// Start at bit 0 of `data`.
    #[must_use]
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    /// Bits already consumed.
    #[must_use]
    pub fn bit_position(&self) -> u64 {
        self.pos as u64
    }

    /// Remaining bits in the slice.
    #[must_use]
    pub fn bits_remaining(&self) -> u64 {
        (self.data.len() * 8).saturating_sub(self.pos) as u64
    }

    /// Next bit, or [`Error::UnexpectedEnd`].
    pub fn read_bit(&mut self) -> Result<bool> {
        let byte = self.pos / 8;
        if byte >= self.data.len() {
            return Err(Error::UnexpectedEnd);
        }
        let shift = 7 - (self.pos % 8);
        self.pos += 1;
        Ok(((self.data[byte] >> shift) & 1) == 1)
    }

    /// Next `n` bits as the low bits of a `u32` (`n` in `0..=32`).
    pub fn read(&mut self, n: u32) -> Result<u32> {
        if n == 0 {
            return Ok(0);
        }
        if n > 32 {
            return Err(Error::Format("bit read wider than 32"));
        }
        let mut v = 0u32;
        for _ in 0..n {
            v = (v << 1) | u32::from(self.read_bit()?);
        }
        Ok(v)
    }

    /// Skip `n` bits.
    pub fn skip(&mut self, n: u32) -> Result<()> {
        let n = n as usize;
        if self.bits_remaining() < n as u64 {
            return Err(Error::UnexpectedEnd);
        }
        self.pos += n;
        Ok(())
    }

    /// Pad to the next byte boundary (0–7 bits discarded).
    pub fn byte_align(&mut self) -> Result<()> {
        let rem = self.pos % 8;
        if rem != 0 {
            self.skip((8 - rem) as u32)?;
        }
        Ok(())
    }
}

/// MSB-first writer used by tests to build LC frames.
#[cfg(test)]
#[derive(Default)]
pub struct BitWriter {
    buf: Vec<u8>,
    acc: u8,
    bits: u8,
}

#[cfg(test)]
impl BitWriter {
    /// Empty writer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append the low `n` bits of `value`, MSB first.
    pub fn write(&mut self, value: u32, n: u32) {
        if n == 0 {
            return;
        }
        for i in (0..n).rev() {
            let bit = ((value >> i) & 1) as u8;
            self.acc = (self.acc << 1) | bit;
            self.bits += 1;
            if self.bits == 8 {
                self.buf.push(self.acc);
                self.acc = 0;
                self.bits = 0;
            }
        }
    }

    /// Append one bit.
    pub fn write_bit(&mut self, bit: bool) {
        self.write(u32::from(bit), 1);
    }

    /// Pad with zeros to a byte boundary and return the bytes.
    #[must_use]
    pub fn finish(mut self) -> Vec<u8> {
        if self.bits > 0 {
            self.acc <<= 8 - self.bits;
            self.buf.push(self.acc);
        }
        self.buf
    }
}
