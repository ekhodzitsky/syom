//! Observable indexed presentation completion; unknown scope is never a proof.
use super::*;

/// Completion of a declared M4A presentation, distinct from container EOF.
/// A complete presentation does not authenticate the source or validate
/// optional indexed media after its declared end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PresentationCompletion {
    /// This API/container does not report a qualified presentation boundary.
    Unknown,
    /// No bounded presentation was declared (including open-ended fragments).
    Unbounded {
        /// All currently indexed AUs have been decoded. More fragments or a
        /// later external duration may still exist.
        indexed_end_reached: bool,
    },
    /// Required priming or declared presentation samples remain undelivered.
    Incomplete {
        /// Required native priming samples not yet discarded.
        priming_samples: u64,
        /// Remaining native presentation samples, or `None` for unknown scope.
        presentation_samples: Option<u64>,
    },
    /// The bounded presentation was delivered exactly; no padding was added.
    Complete,
    /// Every indexed AU was decoded and only sub-tick/sub-millisecond edit
    /// rounding remains. No missing samples were fabricated.
    Rounded {
        /// Undelivered native samples implied only by the rounded duration.
        missing_samples: u64,
    },
}

pub(crate) fn classify(
    track: &AacTrack,
    skip: u64,
    left: Option<u64>,
    index_end: bool,
    rate: u32,
) -> PresentationCompletion {
    if skip != 0 {
        return PresentationCompletion::Incomplete {
            priming_samples: skip,
            presentation_samples: left,
        };
    }
    match left {
        None => PresentationCompletion::Unbounded {
            indexed_end_reached: index_end,
        },
        Some(0) => PresentationCompletion::Complete,
        Some(left) if index_end && track.rounded_presentation_tail(left, rate) => {
            PresentationCompletion::Rounded {
                missing_samples: left,
            }
        }
        Some(left) => PresentationCompletion::Incomplete {
            priming_samples: 0,
            presentation_samples: Some(left),
        },
    }
}

impl<R: Read + Seek> M4aSeek<R> {
    /// Whether this source declares a bounded edit-list presentation. Missing
    /// or open-ended duration is not an independently verified source length.
    #[must_use]
    pub fn bounded_presentation(&self) -> bool {
        self.track.bounded_presentation()
    }

    /// Native output rate declared by this source's AudioSpecificConfig.
    #[must_use]
    pub fn output_sample_rate(&self) -> u32 {
        self.asc.output_sample_rate
    }

    /// Samples remaining to the indexed estimate/declaration. Inspect
    /// [`Self::bounded_presentation`] before treating this as a final duration.
    #[must_use]
    pub fn remaining_samples(&self) -> u64 {
        self.play_left
    }

    /// Inspect completion without reading, changing state or padding PCM.
    /// Merely seeking to the end changes presentation position; it does not
    /// authenticate previously unread source bytes. Failed exact state is
    /// never reported as complete.
    #[must_use]
    pub fn presentation_completion(&self) -> PresentationCompletion {
        if self.exact_failed {
            return PresentationCompletion::Incomplete {
                priming_samples: self.discard_left as u64,
                presentation_samples: Some(self.play_left),
            };
        }
        classify(
            &self.track,
            self.discard_left as u64,
            self.bounded_presentation().then_some(self.play_left),
            self.next_frame == self.track.frames.len(),
            self.asc.output_sample_rate,
        )
    }

    /// True only for an exact or qualified rounded bounded presentation.
    /// Open-ended scope remains false even at the currently indexed EOF.
    #[must_use]
    pub fn presentation_complete(&self) -> bool {
        matches!(
            self.presentation_completion(),
            PresentationCompletion::Complete | PresentationCompletion::Rounded { .. }
        )
    }

    pub(super) fn validate_presentation_end(&mut self) -> Result<()> {
        if matches!(
            self.presentation_completion(),
            PresentationCompletion::Incomplete { .. }
        ) {
            self.exact_failed = self.exact_mode;
            self.checkpoint_clean = false;
            return Err(AacError::format("incomplete M4A presentation"));
        }
        Ok(())
    }
}
