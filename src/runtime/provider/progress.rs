//! Pure progress sequence/count/total monotonicity validator
//! (issue #390 CW-10, CW10-07 protocol half).
//!
//! One tracker owns one in-flight request's progress. The sequence starts at
//! one, increments by exactly one up to [`PROGRESS_SEQUENCE_MAX`]; when a total
//! is present a completed count must be too and may not exceed it; and neither
//! completed nor total may decrease. No process, state, effect, or persistence
//! lives here.

use super::error::{PROGRESS_SEQUENCE_MAX, ProgressFault, ProviderError};

/// Pure progress sequence/count/total monotonicity validator.
#[derive(Debug, Clone, Default)]
pub struct ProgressTracker {
    sequence: Option<u16>,
    completed: Option<u64>,
    total: Option<u64>,
}

impl ProgressTracker {
    /// Construct an empty tracker.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether no progress has been observed yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sequence.is_none()
    }

    /// Reset the tracker to its initial state.
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Validate and absorb one progress event.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::Progress`] for any sequence, count, or total
    /// monotonicity fault.
    pub fn observe(
        &mut self,
        sequence: u16,
        completed: Option<u64>,
        total: Option<u64>,
    ) -> Result<(), ProviderError> {
        self.validate_sequence(sequence)?;
        self.validate_counts(completed, total)?;
        self.sequence = Some(sequence);
        if completed.is_some() {
            self.completed = completed;
        }
        if total.is_some() {
            self.total = total;
        }
        Ok(())
    }

    fn validate_sequence(&self, sequence: u16) -> Result<(), ProviderError> {
        match self.sequence {
            None => {
                if sequence != 1 {
                    return Err(ProviderError::Progress(ProgressFault::BadStart {
                        observed: sequence,
                    }));
                }
            }
            Some(previous) => {
                if sequence <= previous {
                    return Err(ProviderError::Progress(
                        ProgressFault::SequenceNotIncreasing {
                            previous,
                            observed: sequence,
                        },
                    ));
                }
                if sequence != previous + 1 {
                    return Err(ProviderError::Progress(ProgressFault::SequenceGap {
                        expected: previous + 1,
                        observed: sequence,
                    }));
                }
            }
        }
        if sequence > PROGRESS_SEQUENCE_MAX {
            return Err(ProviderError::Progress(ProgressFault::SequenceOverMax {
                observed: sequence,
                max: PROGRESS_SEQUENCE_MAX,
            }));
        }
        Ok(())
    }

    fn validate_counts(
        &self,
        completed: Option<u64>,
        total: Option<u64>,
    ) -> Result<(), ProviderError> {
        if total.is_some() && completed.is_none() {
            return Err(ProviderError::Progress(
                ProgressFault::TotalWithoutCompleted,
            ));
        }
        if let (Some(completed), Some(total)) = (completed, total)
            && completed > total
        {
            return Err(ProviderError::Progress(
                ProgressFault::CompletedExceedsTotal { completed, total },
            ));
        }
        if let (Some(previous), Some(observed)) = (self.completed, completed)
            && observed < previous
        {
            return Err(ProviderError::Progress(ProgressFault::CompletedDecreased {
                previous,
                observed,
            }));
        }
        if let (Some(previous), Some(observed)) = (self.total, total)
            && observed < previous
        {
            return Err(ProviderError::Progress(ProgressFault::TotalDecreased {
                previous,
                observed,
            }));
        }
        Ok(())
    }
}
