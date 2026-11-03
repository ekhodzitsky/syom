//! Fixed LC overlap/window state; scratch is overwritten by the next AU.
use super::*;

impl Filterbank {
    pub(crate) const CHECKPOINT_BYTES: usize = 1 + 1024 * 4;

    pub(crate) fn write_checkpoint(&self, out: &mut Vec<u8>) -> Option<()> {
        if self.overlap.iter().any(|v| !v.is_finite()) {
            return None;
        }
        out.push(match self.prev_shape {
            None => 0,
            Some(WindowShape::Sine) => 1,
            Some(WindowShape::Kbd) => 2,
        });
        for sample in self.overlap {
            out.extend_from_slice(&sample.to_le_bytes());
        }
        Some(())
    }

    pub(crate) fn restore_checkpoint(&mut self, bytes: &[u8]) -> Option<()> {
        if bytes.len() != Self::CHECKPOINT_BYTES {
            return None;
        }
        let shape = match bytes[0] {
            0 => None,
            1 => Some(WindowShape::Sine),
            2 => Some(WindowShape::Kbd),
            _ => return None,
        };
        let number = |chunk: &[u8]| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        if bytes[1..].chunks_exact(4).any(|v| !number(v).is_finite()) {
            return None;
        }
        for (sample, chunk) in self.overlap.iter_mut().zip(bytes[1..].chunks_exact(4)) {
            *sample = number(chunk);
        }
        self.prev_shape = shape;
        self.scratch.fill(0.0);
        Some(())
    }
}
