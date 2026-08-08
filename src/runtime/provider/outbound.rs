//! Bounded host-to-provider outbound envelope queue (issue #390 CW-10, CW10-06).
//!
//! The supervisor is the single writer to a provider's stdin, and every frame
//! it intends to send first passes through this bounded queue. The bound is the
//! protocol's maximum queued outbound envelopes (64): enqueue is rejected
//! **before** the frame is stored once the queue is full, so overflow is a
//! deterministic, typed failure rather than an unbounded buffer or a silent
//! drop. A closed queue rejects further enqueue so the staged shutdown can seal
//! new requests before draining.
//!
//! The queue stores raw, already-encoded JSONL frames (`Vec<u8>`); it owns no
//! protocol state and no process handle. Framing, ordering, and generation are
//! validated elsewhere; this is purely the bounded buffer with deterministic
//! overflow.

use std::collections::VecDeque;

/// The inclusive maximum number of outbound envelopes queued for one provider.
pub const MAX_QUEUED_ENVELOPES: usize = 64;

/// Why an enqueue was rejected.
///
/// No secret value can appear here: the queue stores opaque encoded frames and
/// the reasons name only the bound or the closed state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutboundError {
    /// The queue was full at the bound; the frame was not stored.
    Overflow,
    /// The queue was closed and accepts no further frames.
    Closed,
}

impl std::fmt::Display for OutboundError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Overflow => write!(
                formatter,
                "provider outbound queue is full at {MAX_QUEUED_ENVELOPES} envelopes"
            ),
            Self::Closed => formatter.write_str("provider outbound queue is closed"),
        }
    }
}

impl std::error::Error for OutboundError {}

/// A bounded FIFO queue of encoded outbound envelopes.
#[derive(Debug)]
pub struct OutboundQueue {
    frames: VecDeque<Vec<u8>>,
    closed: bool,
}

impl Default for OutboundQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl OutboundQueue {
    /// Construct an empty, open queue.
    #[must_use]
    pub fn new() -> Self {
        Self {
            frames: VecDeque::with_capacity(MAX_QUEUED_ENVELOPES),
            closed: false,
        }
    }

    /// The number of frames currently buffered.
    #[must_use]
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// Whether no frame is buffered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Whether the queue accepts no further enqueue.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.closed
    }

    /// The number of additional frames that can be enqueued before overflow.
    #[must_use]
    pub fn remaining_capacity(&self) -> usize {
        MAX_QUEUED_ENVELOPES.saturating_sub(self.frames.len())
    }

    /// Enqueue one encoded frame, failing deterministically before storage when
    /// the queue is full or closed.
    ///
    /// # Errors
    ///
    /// Returns [`OutboundError::Overflow`] when the queue already holds
    /// [`MAX_QUEUED_ENVELOPES`] frames, or [`OutboundError::Closed`] when the
    /// queue has been sealed.
    pub fn enqueue(&mut self, frame: Vec<u8>) -> Result<(), OutboundError> {
        if self.closed {
            return Err(OutboundError::Closed);
        }
        if self.frames.len() >= MAX_QUEUED_ENVELOPES {
            return Err(OutboundError::Overflow);
        }
        self.frames.push_back(frame);
        Ok(())
    }

    /// Dequeue the next frame, if any.
    #[must_use]
    pub fn dequeue(&mut self) -> Option<Vec<u8>> {
        self.frames.pop_front()
    }

    /// Drain every buffered frame in FIFO order without closing the queue.
    #[must_use]
    pub fn drain(&mut self) -> Vec<Vec<u8>> {
        self.frames.drain(..).collect()
    }

    /// Seal the queue against new enqueue. Buffered frames remain drainable.
    pub fn close(&mut self) {
        self.closed = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(n: u8) -> Vec<u8> {
        vec![n]
    }

    #[test]
    fn a_new_queue_is_empty_and_open() {
        let queue = OutboundQueue::new();
        assert!(queue.is_empty());
        assert!(!queue.is_closed());
        assert_eq!(queue.remaining_capacity(), MAX_QUEUED_ENVELOPES);
    }

    #[test]
    fn enqueue_then_dequeue_preserves_fifo_order() {
        let mut queue = OutboundQueue::new();
        queue
            .enqueue(frame(1))
            .unwrap_or_else(|error| panic!("enqueue 1: {error:?}"));
        queue
            .enqueue(frame(2))
            .unwrap_or_else(|error| panic!("enqueue 2: {error:?}"));
        assert_eq!(queue.len(), 2);
        assert_eq!(queue.dequeue(), Some(frame(1)));
        assert_eq!(queue.dequeue(), Some(frame(2)));
        assert!(queue.is_empty());
    }

    #[test]
    fn enqueue_at_the_bound_succeeds_and_one_more_overflows() {
        let mut queue = OutboundQueue::new();
        for index in 0..MAX_QUEUED_ENVELOPES {
            let marker = u8::try_from(index).unwrap_or(255);
            queue
                .enqueue(frame(marker))
                .unwrap_or_else(|error| panic!("enqueue {index} must succeed: {error}"));
        }
        assert_eq!(queue.len(), MAX_QUEUED_ENVELOPES);
        assert_eq!(queue.remaining_capacity(), 0);
        let Err(overflow) = queue.enqueue(frame(255)) else {
            panic!("64+1 must overflow");
        };
        assert_eq!(overflow, OutboundError::Overflow);
        assert_eq!(
            queue.len(),
            MAX_QUEUED_ENVELOPES,
            "overflow did not store the frame"
        );
    }

    #[test]
    fn draining_a_full_queue_restores_capacity() {
        let mut queue = OutboundQueue::new();
        for _ in 0..MAX_QUEUED_ENVELOPES {
            queue
                .enqueue(frame(0))
                .unwrap_or_else(|error| panic!("fill: {error:?}"));
        }
        let drained = queue.drain();
        assert_eq!(drained.len(), MAX_QUEUED_ENVELOPES);
        assert_eq!(queue.remaining_capacity(), MAX_QUEUED_ENVELOPES);
    }

    #[test]
    fn a_closed_queue_rejects_enqueue_but_still_drains() {
        let mut queue = OutboundQueue::new();
        queue
            .enqueue(frame(1))
            .unwrap_or_else(|error| panic!("enqueue before close: {error:?}"));
        queue.close();
        assert!(queue.is_closed());
        let Err(rejected) = queue.enqueue(frame(2)) else {
            panic!("closed rejects enqueue");
        };
        assert_eq!(rejected, OutboundError::Closed);
        assert_eq!(queue.drain(), vec![frame(1)], "buffered frames remain");
    }

    #[test]
    fn the_bound_is_exactly_sixty_four() {
        assert_eq!(MAX_QUEUED_ENVELOPES, 64);
    }
}
