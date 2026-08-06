//! Incremental line framing for the live provider stdout drain
//! (issue #390 CW-10, CW10-06).
//!
//! The supervisor reads a provider's stdout as a continuous byte stream and must
//! hand the protocol decoder complete JSONL frames (one UTF-8 JSON object plus a
//! single line feed). [`LineBuffer`] accumulates bytes and yields each complete
//! frame — including its terminating line feed, which is exactly what
//! [`super::framing::decode`] expects — while bounding memory: a run of bytes
//! with no terminating line feed past [`super::framing::MAX_LINE_BYTES`] is a
//! typed oversize fault rather than unbounded growth.
//!
//! No process, state, effect, or persistence lives here.

use super::error::{FramingFault, ProviderError};
use super::framing::MAX_LINE_BYTES;

/// A typed incremental line-framing fault.
///
/// Both variants surface to the supervisor as a `PLG-E502` framing fault.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineError {
    /// A run of bytes without a terminating line feed exceeded the byte bound.
    Oversize {
        /// The observed byte length of the un-terminated run.
        bytes: usize,
        /// The inclusive byte limit.
        limit: usize,
    },
}

impl LineError {
    /// Convert this byte-level fault into the closed protocol error.
    #[must_use]
    pub fn into_provider_error(self) -> ProviderError {
        match self {
            Self::Oversize { bytes, limit } => {
                ProviderError::Framing(FramingFault::Oversize { bytes, limit })
            }
        }
    }
}

/// An incremental line buffer that yields complete LF-terminated frames.
#[derive(Debug, Default)]
pub struct LineBuffer {
    partial: Vec<u8>,
}

impl LineBuffer {
    /// Construct an empty buffer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Absorb `bytes` and return every newly complete frame, each including its
    /// terminating line feed.
    ///
    /// # Errors
    ///
    /// Returns [`LineError::Oversize`] when an un-terminated run exceeds
    /// [`MAX_LINE_BYTES`].
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<Vec<u8>>, LineError> {
        let mut frames = Vec::new();
        let mut cursor = 0;
        for (index, byte) in bytes.iter().enumerate() {
            if *byte == b'\n' {
                let mut frame = std::mem::take(&mut self.partial);
                frame.extend_from_slice(&bytes[cursor..=index]);
                // A complete LF-terminated frame is bounded the same way as a
                // trailing partial run: reject it before it is yielded so an
                // oversized line and its terminator cannot slip through in one
                // push or accumulate across pushes.
                if frame.len() > MAX_LINE_BYTES {
                    let bytes_seen = frame.len();
                    self.partial.clear();
                    return Err(LineError::Oversize {
                        bytes: bytes_seen,
                        limit: MAX_LINE_BYTES,
                    });
                }
                frames.push(frame);
                cursor = index + 1;
            }
        }
        // Absorb the trailing partial run.
        let tail = &bytes[cursor..];
        if !tail.is_empty() {
            self.partial.extend_from_slice(tail);
        }
        if self.partial.len() > MAX_LINE_BYTES {
            let bytes_seen = self.partial.len();
            // Drop the oversize partial so a later push does not keep growing.
            self.partial.clear();
            return Err(LineError::Oversize {
                bytes: bytes_seen,
                limit: MAX_LINE_BYTES,
            });
        }
        Ok(frames)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_complete_frame_is_yielded_with_its_line_feed() {
        let mut buffer = LineBuffer::new();
        let frames = buffer
            .push(b"{\"a\":1}\n")
            .unwrap_or_else(|error| panic!("push: {error:?}"));
        assert_eq!(frames, vec![b"{\"a\":1}\n".to_vec()]);
    }

    #[test]
    fn frames_split_across_pushes_are_reassembled() {
        let mut buffer = LineBuffer::new();
        let first = buffer
            .push(b"{\"a\":")
            .unwrap_or_else(|error| panic!("push: {error:?}"));
        assert!(first.is_empty());
        let second = buffer
            .push(b"1}\n")
            .unwrap_or_else(|error| panic!("push: {error:?}"));
        assert_eq!(second, vec![b"{\"a\":1}\n".to_vec()]);
    }

    #[test]
    fn multiple_frames_in_one_push_are_yielded_in_order() {
        let mut buffer = LineBuffer::new();
        let frames = buffer
            .push(b"a\nb\nc\n")
            .unwrap_or_else(|error| panic!("push: {error:?}"));
        assert_eq!(
            frames,
            vec![b"a\n".to_vec(), b"b\n".to_vec(), b"c\n".to_vec()]
        );
    }

    #[test]
    fn a_trailing_partial_is_retained_for_the_next_push() {
        let mut buffer = LineBuffer::new();
        let frames = buffer
            .push(b"first\npartial")
            .unwrap_or_else(|error| panic!("push: {error:?}"));
        assert_eq!(frames, vec![b"first\n".to_vec()]);
        let more = buffer
            .push(b"-end\n")
            .unwrap_or_else(|error| panic!("push: {error:?}"));
        assert_eq!(more, vec![b"partial-end\n".to_vec()]);
    }

    #[test]
    fn an_unterminated_run_past_the_limit_is_an_oversize_fault() {
        let mut buffer = LineBuffer::new();
        // Push exactly the limit with no terminator: still legal, but one more
        // byte crosses the bound.
        let at_limit = vec![b'A'; MAX_LINE_BYTES];
        let frames = buffer
            .push(&at_limit)
            .unwrap_or_else(|error| panic!("at limit is still partial: {error:?}"));
        assert!(frames.is_empty());
        let Err(error) = buffer.push(b"B") else {
            panic!("one past the limit overflows");
        };
        match error {
            LineError::Oversize { bytes, limit } => {
                assert_eq!(bytes, MAX_LINE_BYTES + 1);
                assert_eq!(limit, MAX_LINE_BYTES);
            }
        }
        // After an oversize fault the buffer is reusable.
        let frames = buffer
            .push(b"x\n")
            .unwrap_or_else(|error| panic!("push after fault: {error:?}"));
        assert_eq!(frames, vec![b"x\n".to_vec()]);
    }

    #[test]
    fn a_complete_frame_at_exactly_the_limit_is_accepted() {
        let mut buffer = LineBuffer::new();
        // Content of MAX_LINE_BYTES - 1 bytes plus the LF terminator equals the
        // limit exactly: the largest legal complete frame.
        let mut at_limit = vec![b'A'; MAX_LINE_BYTES - 1];
        at_limit.push(b'\n');
        assert_eq!(at_limit.len(), MAX_LINE_BYTES);
        let frames = buffer
            .push(&at_limit)
            .unwrap_or_else(|error| panic!("at-limit frame is legal: {error:?}"));
        assert_eq!(frames.len(), 1, "the at-limit frame is yielded");
        assert_eq!(frames[0].len(), MAX_LINE_BYTES);
    }

    #[test]
    fn a_complete_frame_one_past_the_limit_in_one_chunk_is_rejected() {
        let mut buffer = LineBuffer::new();
        // Content of MAX_LINE_BYTES bytes plus the LF terminator is one past the
        // limit; it must be rejected even though it is a complete frame.
        let mut over_limit = vec![b'A'; MAX_LINE_BYTES];
        over_limit.push(b'\n');
        assert_eq!(over_limit.len(), MAX_LINE_BYTES + 1);
        let Err(error) = buffer.push(&over_limit) else {
            panic!("a complete frame one past the limit must be rejected");
        };
        match error {
            LineError::Oversize { bytes, limit } => {
                assert_eq!(bytes, MAX_LINE_BYTES + 1);
                assert_eq!(limit, MAX_LINE_BYTES);
            }
        }
    }

    #[test]
    fn an_oversize_complete_frame_split_across_pushes_is_rejected() {
        let mut buffer = LineBuffer::new();
        // Push the limit in content with no terminator: exactly at the partial
        // bound, still legal as a partial run.
        let at_limit = vec![b'A'; MAX_LINE_BYTES];
        let frames = buffer
            .push(&at_limit)
            .unwrap_or_else(|error| panic!("at-limit partial is legal: {error:?}"));
        assert!(frames.is_empty());
        // The terminating LF assembles a frame one past the limit: rejected.
        let Err(error) = buffer.push(b"\n") else {
            panic!("the split oversize complete frame must be rejected");
        };
        match error {
            LineError::Oversize { bytes, limit } => {
                assert_eq!(bytes, MAX_LINE_BYTES + 1);
                assert_eq!(limit, MAX_LINE_BYTES);
            }
        }
    }

    #[test]
    fn an_oversize_fault_maps_to_the_protocol_framing_error() {
        let error = LineError::Oversize {
            bytes: 10,
            limit: MAX_LINE_BYTES,
        };
        let provider_error = error.into_provider_error();
        assert_eq!(
            provider_error.code(),
            super::super::error::PROTOCOL_FAILURE_CODE
        );
    }
}
