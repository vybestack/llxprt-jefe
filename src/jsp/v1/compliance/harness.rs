//! Deterministic compliance-harness clock and sequence utilities.

/// A deterministic clock advanced explicitly by a trace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FakeClock {
    now_ms: u64,
}

impl FakeClock {
    #[must_use]
    pub const fn new(now_ms: u64) -> Self {
        Self { now_ms }
    }

    #[must_use]
    pub const fn now_ms(self) -> u64 {
        self.now_ms
    }

    pub fn set_ms(&mut self, now_ms: u64) -> Result<(), ClockError> {
        if now_ms < self.now_ms {
            return Err(ClockError::MovedBackward {
                previous: self.now_ms,
                actual: now_ms,
            });
        }
        self.now_ms = now_ms;
        Ok(())
    }

    #[must_use]
    pub fn lease_expired(self, observed_ms: u64, lease_ms: u64) -> bool {
        self.now_ms.saturating_sub(observed_ms) > lease_ms
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockError {
    MovedBackward { previous: u64, actual: u64 },
}

/// A cursor-based exact-next sequence checker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SequenceGenerator {
    last_applied: u64,
}

impl SequenceGenerator {
    #[must_use]
    pub const fn after_cursor(cursor: u64) -> Self {
        Self {
            last_applied: cursor,
        }
    }

    #[must_use]
    pub const fn last_applied(self) -> u64 {
        self.last_applied
    }

    #[must_use]
    pub const fn expected_next(self) -> u64 {
        self.last_applied.saturating_add(1)
    }

    pub fn apply(&mut self, sequence: u64) -> SequenceDisposition {
        if sequence <= self.last_applied {
            return SequenceDisposition::Noop;
        }
        let expected = self.expected_next();
        if sequence != expected {
            return SequenceDisposition::Gap {
                expected,
                actual: sequence,
            };
        }
        self.last_applied = sequence;
        SequenceDisposition::Applied
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SequenceDisposition {
    Applied,
    Noop,
    Gap { expected: u64, actual: u64 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequence_starts_after_snapshot_cursor() {
        let mut sequence = SequenceGenerator::after_cursor(41);
        assert_eq!(sequence.expected_next(), 42);
        assert_eq!(sequence.apply(42), SequenceDisposition::Applied);
        assert_eq!(sequence.last_applied(), 42);
    }

    #[test]
    fn sequence_distinguishes_noop_and_gap() {
        let mut sequence = SequenceGenerator::after_cursor(5);
        assert_eq!(sequence.apply(5), SequenceDisposition::Noop);
        assert_eq!(
            sequence.apply(7),
            SequenceDisposition::Gap {
                expected: 6,
                actual: 7
            }
        );
    }

    #[test]
    fn fake_clock_is_monotonic_and_drives_lease() {
        let mut clock = FakeClock::new(10);
        assert!(clock.set_ms(20).is_ok());
        assert!(clock.lease_expired(10, 5));
        assert!(matches!(
            clock.set_ms(19),
            Err(ClockError::MovedBackward { .. })
        ));
    }
}
