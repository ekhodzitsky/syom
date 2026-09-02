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

    /// Construct a reader starting at `byte_pos`.
    #[must_use]
    pub fn with_position(data: &'a [u8], byte_pos: usize) -> Self {
        Self {
            data,
            pos: byte_pos.saturating_mul(8),
        }
    }

    /// Byte index of the next bit.
    #[must_use]
    pub fn byte_position(&self) -> usize {
        self.pos / 8
    }

    /// True when `bit_position` is a multiple of 8.
    #[must_use]
    pub fn is_byte_aligned(&self) -> bool {
        self.pos % 8 == 0
    }

    /// Alias of [`Self::byte_align`].
    pub fn align_to_byte(&mut self) -> Result<()> {
        self.byte_align()
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

    /// Alias of [`Self::read`].
    pub fn read_u32(&mut self, n: u32) -> Result<u32> {
        self.read(n)
    }

    /// One-bit unsigned.
    pub fn read_u1(&mut self) -> Result<u32> {
        self.read(1)
    }

    /// Signed `n`-bit two's complement.
    pub fn read_i32(&mut self, n: u32) -> Result<i32> {
        let u = self.read(n)?;
        if n == 0 {
            return Ok(0);
        }
        let sign = 1u32 << (n - 1);
        if u & sign == 0 {
            Ok(u as i32)
        } else {
            Ok((u as i32) - (1i32 << n))
        }
    }

    /// Up to 64 bits (reads in 32-bit chunks).
    pub fn read_u64(&mut self, n: u32) -> Result<u64> {
        if n <= 32 {
            return Ok(u64::from(self.read(n)?));
        }
        let hi = u64::from(self.read(n - 32)?);
        let lo = u64::from(self.read(32)?);
        Ok((hi << 32) | lo)
    }

    /// Look ahead without consuming.
    pub fn peek_u32(&mut self, n: u32) -> Result<u32> {
        let saved = self.pos;
        let v = self.read(n);
        self.pos = saved;
        v
    }

    /// Alias of [`Self::skip`].
    pub fn consume(&mut self, n: u32) -> Result<()> {
        self.skip(n)
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

/// MSB-first writer used to build LC / SBR test frames.
#[derive(Default)]
pub struct BitWriter {
    buf: Vec<u8>,
    acc: u8,
    bits: u8,
}

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

    /// Alias of [`Self::write`].
    pub fn write_u32(&mut self, value: u32, n: u32) {
        self.write(value, n)
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
