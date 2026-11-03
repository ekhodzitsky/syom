//! Push completion has no complete lifetime AU index: never infer a rounded end.
use super::*;
use crate::stream::{Container, PresentationCompletion};

impl Decoder {
    pub(in crate::stream) fn presentation_completion_status(&self) -> PresentationCompletion {
        if matches!(self.container, Container::Fmp4) {
            self.fmp4_completion()
        } else {
            PresentationCompletion::Unknown
        }
    }

    pub(in crate::stream) fn fmp4_completion(&self) -> PresentationCompletion {
        let f = &self.fmp4;
        if f.skip_left != 0 || f.play_left.is_some_and(|left| left != 0) {
            return PresentationCompletion::Incomplete {
                priming_samples: f.skip_left as u64,
                presentation_samples: f.play_left.map(|left| left as u64),
            };
        }
        if f.play_left.is_some() {
            PresentationCompletion::Complete
        } else {
            // Per-fragment indices are discarded as the stream progresses;
            // even EOF cannot prove that a later external fragment is absent.
            PresentationCompletion::Unbounded {
                indexed_end_reached: false,
            }
        }
    }

    pub(super) fn validate_fmp4_completion(&self) -> Result<()> {
        if matches!(
            self.fmp4_completion(),
            PresentationCompletion::Incomplete { .. }
        ) {
            return Err(AacError::format("incomplete M4A presentation"));
        }
        Ok(())
    }
}
