//! Typed provider-protocol failures (issue #390 CW-10, Slice A).
//!
//! The action-provider JSONL protocol is closed, so every observable failure —
//! a malformed frame, an unknown field, a wrong direction, an out-of-order
//! handshake step, a non-monotonic progress event — is one operator condition:
//! this generation violated the contract. That condition carries the single
//! stable code [`PROTOCOL_FAILURE_CODE`] (`PLG-E502`), and the variant carries
//! the typed reason a supervisor or test can branch on without parsing a
//! message string.
//!
//! This module is the only error home for the protocol layer. It depends on the
//! shared bounded reader's [`BoundedJsonError`] and nothing else in this crate,
//! so the wire layer (`protocol`) can depend on it without a cycle.

use std::fmt;

use crate::domain::bounded_json::BoundedJsonError;

/// The single stable operator-visible code for every provider-protocol failure.
pub const PROTOCOL_FAILURE_CODE: &str = "PLG-E502";

/// The operator-visible code for a provider-runtime-unavailable condition.
///
/// Spawn, I/O, timeout, crash, or environment failures carry this code. It is
/// distinct from [`PROTOCOL_FAILURE_CODE`] so an operator can tell a
/// closed-protocol contract violation from a runtime condition.
pub const RUNTIME_UNAVAILABLE_CODE: &str = "PLG-E503";

/// The inclusive upper bound on a progress sequence value.
pub const PROGRESS_SEQUENCE_MAX: u16 = 256;

/// A closed, typed provider-protocol failure.
///
/// Every variant is one `PLG-E502` condition. [`Self::code`] always returns
/// [`PROTOCOL_FAILURE_CODE`]; [`Display`](std::fmt::Display) prefixes the
/// reason with it so an operator reading a log or recovery panel sees the code
/// and the cause together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderError {
    /// A byte-level framing fault before JSON parsing (CW10-06 framing).
    Framing(FramingFault),
    /// The shared bounded reader rejected the line: duplicate key, trailing
    /// data, non-UTF-8, oversize, inadmissible number, or syntax.
    Json(BoundedJsonError),
    /// An object named a field the closed schema does not admit.
    UnknownField {
        /// Dotted path to the offending object.
        path: String,
        /// The field key the schema does not name.
        field: String,
    },
    /// A required field was absent.
    MissingField {
        /// Dotted path to the offending object.
        path: String,
        /// The missing field key.
        field: String,
    },
    /// A value had the wrong JSON type.
    TypeMismatch {
        /// Dotted path to the offending value.
        path: String,
        /// The JSON type the schema expected.
        expected: &'static str,
    },
    /// A string was not one of the declared wire names.
    UnknownValue {
        /// Dotted path to the offending value.
        path: String,
        /// The spelling the schema does not accept.
        value: String,
    },
    /// A value passed structural mapping but failed its own validation.
    InvalidValue {
        /// Dotted path to the offending value.
        path: String,
        /// Why the value was rejected.
        reason: String,
    },
    /// A request id did not match `h-`/`p-` plus 6–20 ASCII digits.
    InvalidRequestId {
        /// The raw id text that failed validation.
        raw: String,
    },
    /// A generation was zero, negative, or changed within one process.
    InvalidGeneration {
        /// The offending generation value.
        value: u64,
    },
    /// A payload arrived from the wrong stream direction.
    InvalidDirection {
        /// The message type that was misplaced.
        kind: String,
        /// The stream it arrived on.
        stream: String,
    },
    /// A message arrived in the wrong lifecycle order.
    OutOfOrder {
        /// The lifecycle phase that was active.
        phase: String,
        /// The message type that was rejected.
        kind: String,
    },
    /// A progress event violated sequence, count, or total monotonicity.
    Progress(ProgressFault),
}

impl ProviderError {
    /// The stable operator-visible code for this failure.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        PROTOCOL_FAILURE_CODE
    }
}

impl fmt::Display for ProviderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{PROTOCOL_FAILURE_CODE}: ")?;
        match self {
            Self::Framing(fault) => write!(formatter, "{fault}"),
            Self::Json(error) => write!(formatter, "{error}"),
            Self::UnknownField { path, field } => {
                write!(formatter, "{path} has no field {field:?}")
            }
            Self::MissingField { path, field } => {
                write!(formatter, "{path} is missing required field {field:?}")
            }
            Self::TypeMismatch { path, expected } => {
                write!(formatter, "{path} must be {expected}")
            }
            Self::UnknownValue { path, value } => {
                write!(formatter, "{path} does not accept the value {value:?}")
            }
            Self::InvalidValue { path, reason } => write!(formatter, "{path}: {reason}"),
            Self::InvalidRequestId { raw } => {
                write!(
                    formatter,
                    "request id {raw:?} must be h-/p- plus 6-20 ASCII digits"
                )
            }
            Self::InvalidGeneration { value } => {
                write!(formatter, "generation {value} must be positive and fixed")
            }
            Self::InvalidDirection { kind, stream } => {
                write!(formatter, "{kind} is not legal on the {stream} stream")
            }
            Self::OutOfOrder { phase, kind } => {
                write!(formatter, "{kind} is not legal in the {phase} phase")
            }
            Self::Progress(fault) => write!(formatter, "{fault}"),
        }
    }
}

impl std::error::Error for ProviderError {}

/// A byte-level framing fault (CW10-06 framing row).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FramingFault {
    /// The line was not terminated by a single line feed.
    MissingTerminator,
    /// The line contained a carriage return (CRLF is rejected).
    CarriageReturn,
    /// The line began with a UTF-8 byte-order mark.
    ByteOrderMark,
    /// The line was empty or only whitespace.
    BlankLine,
    /// The line contained more than one physical line (an interior line feed).
    InteriorLineFeed,
    /// The line exceeded the byte bound.
    Oversize {
        /// The observed byte length.
        bytes: usize,
        /// The inclusive byte limit.
        limit: usize,
    },
}

impl fmt::Display for FramingFault {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingTerminator => {
                formatter.write_str("a line must end with a single line feed")
            }
            Self::CarriageReturn => formatter.write_str("a line may not contain a carriage return"),
            Self::ByteOrderMark => {
                formatter.write_str("a line may not begin with a byte-order mark")
            }
            Self::BlankLine => formatter.write_str("a line may not be empty or only whitespace"),
            Self::InteriorLineFeed => {
                formatter.write_str("a frame must contain exactly one JSONL line")
            }
            Self::Oversize { bytes, limit } => {
                write!(formatter, "a line is {bytes} bytes, over the {limit} limit")
            }
        }
    }
}

/// A progress-event monotonicity fault (CW10-07 protocol half).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgressFault {
    /// The first sequence was not `1`.
    BadStart {
        /// The observed first sequence.
        observed: u16,
    },
    /// A sequence skipped a value (not exactly previous + 1).
    SequenceGap {
        /// The expected sequence value.
        expected: u16,
        /// The observed sequence value.
        observed: u16,
    },
    /// A sequence did not increase over the previous one.
    SequenceNotIncreasing {
        /// The previous sequence value.
        previous: u16,
        /// The observed sequence value.
        observed: u16,
    },
    /// A sequence exceeded [`PROGRESS_SEQUENCE_MAX`].
    SequenceOverMax {
        /// The observed sequence value.
        observed: u16,
        /// The inclusive maximum.
        max: u16,
    },
    /// A `total` was present without a `completed`.
    TotalWithoutCompleted,
    /// A `completed` exceeded its `total`.
    CompletedExceedsTotal {
        /// The completed count.
        completed: u64,
        /// The total count.
        total: u64,
    },
    /// A `completed` decreased from a previous event.
    CompletedDecreased {
        /// The previous completed count.
        previous: u64,
        /// The observed completed count.
        observed: u64,
    },
    /// A `total` decreased from a previous event.
    TotalDecreased {
        /// The previous total count.
        previous: u64,
        /// The observed total count.
        observed: u64,
    },
}

impl fmt::Display for ProgressFault {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadStart { observed } => {
                write!(
                    formatter,
                    "progress sequence must start at 1, not {observed}"
                )
            }
            Self::SequenceGap { expected, observed } => {
                write!(
                    formatter,
                    "progress sequence gap: expected {expected}, observed {observed}"
                )
            }
            Self::SequenceNotIncreasing { previous, observed } => {
                write!(
                    formatter,
                    "progress sequence must increase: previous {previous}, observed {observed}"
                )
            }
            Self::SequenceOverMax { observed, max } => {
                write!(
                    formatter,
                    "progress sequence {observed} exceeds the {max} maximum"
                )
            }
            Self::TotalWithoutCompleted => {
                formatter.write_str("progress total requires a completed count")
            }
            Self::CompletedExceedsTotal { completed, total } => {
                write!(
                    formatter,
                    "progress completed {completed} exceeds total {total}"
                )
            }
            Self::CompletedDecreased { previous, observed } => {
                write!(
                    formatter,
                    "progress completed must not decrease: previous {previous}, observed {observed}"
                )
            }
            Self::TotalDecreased { previous, observed } => {
                write!(
                    formatter,
                    "progress total must not decrease: previous {previous}, observed {observed}"
                )
            }
        }
    }
}
