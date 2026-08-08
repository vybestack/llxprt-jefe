//! Stream direction, closed message kinds, and validated request ids / env names
//! for the action-provider protocol (issue #390 CW-10, Slice A).
//!
//! These primitive identifiers carry no process, application state, effect, or
//! persistence. The readers that consume them later reach the same typed values
//! through [`super::protocol`]'s re-exports.

use std::fmt;

/// The single wire protocol version this layer accepts.
pub(super) const PROTOCOL_VERSION: u64 = 1;

/// Minimum ASCII digits after the `h-`/`p-` request-id prefix.
const REQUEST_ID_MIN_DIGITS: usize = 6;

/// Maximum ASCII digits after the `h-`/`p-` request-id prefix.
const REQUEST_ID_MAX_DIGITS: usize = 20;

/// Maximum bytes in an environment-variable name.
const ENV_NAME_BYTE_LIMIT: usize = 128;

/// The stream a message travels on: host-to-provider or provider-to-host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Messages the host sends to the provider.
    HostToProvider,
    /// Messages the provider sends to the host.
    ProviderToHost,
}

impl Direction {
    /// A stable, operator-readable name for diagnostics.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HostToProvider => "host-to-provider",
            Self::ProviderToHost => "provider-to-host",
        }
    }
}

/// Which of the eleven closed message kinds a frame carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageKind {
    /// Host greeting that opens a handshake.
    Hello,
    /// Provider greeting that completes the first handshake half.
    HelloAck,
    /// Host configuration hand-off.
    Configure,
    /// Provider readiness and declared capabilities.
    Ready,
    /// Host request to run one action.
    InvokeAction,
    /// Host request to cancel an in-flight request.
    Cancel,
    /// Provider progress on an in-flight request.
    Progress,
    /// Provider terminal result for an in-flight request.
    Outcome,
    /// Provider terminal failure for an in-flight request.
    Error,
    /// Host request to end the process.
    Shutdown,
    /// Provider acknowledgement of shutdown.
    ShutdownAck,
}

impl MessageKind {
    /// Every kind, in declaration order.
    pub const ALL: [Self; 11] = [
        Self::Hello,
        Self::HelloAck,
        Self::Configure,
        Self::Ready,
        Self::InvokeAction,
        Self::Cancel,
        Self::Progress,
        Self::Outcome,
        Self::Error,
        Self::Shutdown,
        Self::ShutdownAck,
    ];

    /// The lower-kebab-case wire name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hello => "hello",
            Self::HelloAck => "hello-ack",
            Self::Configure => "configure",
            Self::Ready => "ready",
            Self::InvokeAction => "invoke-action",
            Self::Cancel => "cancel",
            Self::Progress => "progress",
            Self::Outcome => "outcome",
            Self::Error => "error",
            Self::Shutdown => "shutdown",
            Self::ShutdownAck => "shutdown-ack",
        }
    }

    /// Resolve a wire name, exactly and case-sensitively.
    #[must_use]
    pub fn from_wire(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == value)
    }

    /// The stream this kind is legal on.
    #[must_use]
    pub const fn direction(self) -> Direction {
        match self {
            Self::Hello | Self::Configure | Self::InvokeAction | Self::Cancel | Self::Shutdown => {
                Direction::HostToProvider
            }
            Self::HelloAck
            | Self::Ready
            | Self::Progress
            | Self::Outcome
            | Self::Error
            | Self::ShutdownAck => Direction::ProviderToHost,
        }
    }
}

/// Which side originated a request id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestOrigin {
    /// A host-originated `h-` id.
    Host,
    /// A provider-originated `p-` id.
    Provider,
}

impl RequestOrigin {
    /// The single-character wire prefix.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Host => "h",
            Self::Provider => "p",
        }
    }
}

/// A validated `h-`/`p-` plus 6–20 ASCII digit request id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestId {
    origin: RequestOrigin,
    digits: String,
}

/// Why a request id failed validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestIdError {
    /// The raw id text that failed.
    pub raw: String,
}

impl RequestId {
    /// Parse and validate a request id.
    ///
    /// # Errors
    ///
    /// Returns [`RequestIdError`] when the value is not `h-`/`p-` plus 6–20
    /// ASCII digits.
    pub fn parse(raw: &str) -> Result<Self, RequestIdError> {
        let (origin, digits) = match raw.split_once('-') {
            Some(("h", digits)) => (RequestOrigin::Host, digits),
            Some(("p", digits)) => (RequestOrigin::Provider, digits),
            _ => return Err(Self::error(raw)),
        };
        let valid = (REQUEST_ID_MIN_DIGITS..=REQUEST_ID_MAX_DIGITS).contains(&digits.len())
            && digits.bytes().all(|byte| byte.is_ascii_digit());
        if !valid {
            return Err(Self::error(raw));
        }
        Ok(Self {
            origin,
            digits: digits.to_owned(),
        })
    }

    /// Construct a host-originated request id from a monotonic counter.
    ///
    /// The counter is zero-padded to at least 6 digits (the minimum) and must
    /// not exceed 20 digits (the maximum). This is the safe constructor for
    /// production code: the coordinator/worker owns the counter, guaranteeing
    /// uniqueness per in-flight request without string parsing.
    ///
    /// # Errors
    ///
    /// Returns [`RequestIdError`] when the counter exceeds 20 digits.
    pub fn new_host(counter: u64) -> Result<Self, RequestIdError> {
        let digits = format!("{counter}");
        if digits.len() > REQUEST_ID_MAX_DIGITS {
            return Err(RequestIdError {
                raw: format!("h-{digits}"),
            });
        }
        let padded = format!("{digits:0>REQUEST_ID_MIN_DIGITS$}");
        Ok(Self {
            origin: RequestOrigin::Host,
            digits: padded,
        })
    }

    fn error(raw: &str) -> RequestIdError {
        RequestIdError {
            raw: raw.to_owned(),
        }
    }

    /// Which side originated this id.
    #[must_use]
    pub const fn origin(&self) -> RequestOrigin {
        self.origin
    }

    /// Borrow the validated digit run.
    #[must_use]
    pub fn digits(&self) -> &str {
        &self.digits
    }

    /// Reconstruct the canonical wire text.
    #[must_use]
    pub fn as_str(&self) -> String {
        format!("{}-{}", self.origin.as_str(), self.digits)
    }
}

impl fmt::Display for RequestId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.as_str())
    }
}

/// A validated environment-variable name.
///
/// Used as the key of the configure `secrets` and `environment` maps, so a key
/// is never an arbitrary string reaching the provider environment.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EnvName(String);

/// Why an environment-variable name failed validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvNameError {
    /// The raw name that failed.
    pub raw: String,
}

impl EnvName {
    /// Parse and validate an environment-variable name.
    ///
    /// # Errors
    ///
    /// Returns [`EnvNameError`] when the value is not
    /// `[A-Z_][A-Z0-9_]{0,127}`.
    pub fn parse(value: &str) -> Result<Self, EnvNameError> {
        let mut bytes = value.bytes();
        let valid = value.len() <= ENV_NAME_BYTE_LIMIT
            && bytes
                .next()
                .is_some_and(|byte| byte.is_ascii_uppercase() || byte == b'_')
            && bytes.all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_');
        if valid {
            Ok(Self(value.to_owned()))
        } else {
            Err(EnvNameError {
                raw: value.to_owned(),
            })
        }
    }

    /// Borrow the validated name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for EnvName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl std::borrow::Borrow<str> for EnvName {
    fn borrow(&self) -> &str {
        &self.0
    }
}
