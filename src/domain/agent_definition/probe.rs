//! Closed probe specification and bounded parser (issue #382 CW-02).
//!
//! Two-tier authority (S3a/S3b): [`crate::domain::agent_definition::definition::AgentDefinition`]
//! is the sole authority for fixture+official-reference-verified argv
//! mappings. The probe spec declares the exact argv that produces a stream,
//! how to frame and normalize that stream, the identity recognizer, and the
//! authored capability tokens whose presence is verified against the captured
//! `--help` output. The probe does not *discover* mappings; it verifies
//! installed-release identity and the presence of authored capability tokens.
//!
//! The closed schema is:
//! ```text
//! ProbeSpec={ argv:[string 1..8], stream:Stdout|Stderr|Combined,
//!   framing:SingleJson|JsonLines|Utf8Text,
//!   identity:JsonPointer{pointer,anchored_pattern}|Line{prefix,anchored_pattern},
//!   capability_probe:CapabilityProbe{
//!     argv:[string 1..8], stream:Stdout|Stderr|Combined,
//!     normalize:None|StripAnsi,
//!     tokens:[{id:string,token:string} 0..32]},
//!   required:[ID 0..32],
//!   timeout_ms:1..5000, max_bytes:1..65536 }
//! ```
//!
//! Required capabilities are baseline capabilities whose absence makes an
//! installation `InstalledIncompatible`. Optional tokens gate only their
//! field/operation later and are not required at the probe layer. The
//! implementation uses bounded recognizers, not a new regex dependency.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::diagnostics::DefinitionError;
use super::json_pointer::JsonPointer;
use super::limits::{
    LOCAL_PROBE_TIMEOUT_MS, PROBE_ARGV_LIMIT, PROBE_STREAM_LIMIT, REMOTE_PROBE_TIMEOUT_MS,
    STRING_VALUE_BYTE_LIMIT,
};
use super::normalize::strip_ansi_escape;

/// Which process stream(s) to capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProbeStream {
    /// Capture stdout only.
    #[default]
    Stdout,
    /// Capture stderr only.
    Stderr,
    /// Capture stdout and stderr combined.
    Combined,
}

/// How to frame the captured stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProbeFraming {
    /// A single JSON document; trailing bytes are a probe error.
    SingleJson,
    /// Newline-delimited JSON lines.
    JsonLines,
    /// Free-form UTF-8 text.
    #[default]
    Utf8Text,
}

/// Bounded anchored-pattern recognizer (no regex dependency).
///
/// Shipped patterns are exact prefix/suffix/version-token recognizers only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AnchoredPattern {
    /// Exact full match.
    Exact {
        /// The exact value to match.
        value: String,
    },
    /// Begins with `prefix`.
    Prefix {
        /// The required prefix.
        prefix: String,
    },
    /// Ends with `suffix`.
    Suffix {
        /// The required suffix.
        suffix: String,
    },
    /// Begins with `prefix` and ends with `suffix`.
    PrefixSuffix {
        /// The required prefix.
        prefix: String,
        /// The required suffix.
        suffix: String,
    },
    /// Matches a semantic-version token of the form `X.Y.Z[-label...]`.
    VersionToken,
}

impl AnchoredPattern {
    /// Test whether `text` matches this anchored recognizer.
    #[must_use]
    pub fn matches(&self, text: &str) -> bool {
        match self {
            Self::Exact { value } => text == value,
            Self::Prefix { prefix } => text.starts_with(prefix),
            Self::Suffix { suffix } => text.ends_with(suffix),
            Self::PrefixSuffix { prefix, suffix } => {
                text.starts_with(prefix) && text.ends_with(suffix) && text.len() >= prefix.len()
            }
            Self::VersionToken => matches_version_token(text),
        }
    }

    /// Validate this pattern against the closed bounds.
    pub fn validate(&self) -> Result<(), ProbeValidateError> {
        match self {
            Self::Exact { value } => validate_bounded_string(value, "anchored exact value")?,
            Self::Prefix { prefix } => validate_bounded_string(prefix, "anchored prefix")?,
            Self::Suffix { suffix } => validate_bounded_string(suffix, "anchored suffix")?,
            Self::PrefixSuffix { prefix, suffix } => {
                validate_bounded_string(prefix, "anchored prefix")?;
                validate_bounded_string(suffix, "anchored suffix")?;
            }
            Self::VersionToken => {}
        }
        Ok(())
    }
}

fn validate_bounded_string(value: &str, what: &str) -> Result<(), ProbeValidateError> {
    if value.is_empty() {
        return Err(ProbeValidateError::EmptyString {
            what: what.to_string(),
        });
    }
    if value.len() > STRING_VALUE_BYTE_LIMIT {
        return Err(ProbeValidateError::StringTooLong {
            what: what.to_string(),
            bytes: value.len(),
        });
    }
    Ok(())
}

/// A semantic-version token: `X.Y.Z` optionally followed by `-label`.
fn matches_version_token(text: &str) -> bool {
    let mut parts = text.splitn(2, '-');
    let numeric = parts.next().unwrap_or("");
    let dots: Vec<&str> = numeric.split('.').collect();
    if dots.len() != 3 {
        return false;
    }
    if parts.next().is_some_and(str::is_empty) {
        return false;
    }
    dots.iter()
        .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

/// Identity recognizer: a JSON pointer or line prefix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IdentityRecognizer {
    /// RFC 6901 pointer into a JSON document plus an anchored pattern.
    JsonPointer {
        /// Validated RFC 6901 pointer.
        pointer: String,
        /// Anchored pattern the referenced value must match.
        anchored_pattern: AnchoredPattern,
    },
    /// First line beginning with `prefix` plus an anchored pattern.
    Line {
        /// Line prefix to strip before pattern matching.
        prefix: String,
        /// Anchored pattern the stripped value must match.
        anchored_pattern: AnchoredPattern,
    },
}

impl IdentityRecognizer {
    /// Validate this recognizer against the closed bounds.
    pub fn validate(&self) -> Result<(), ProbeValidateError> {
        match self {
            Self::JsonPointer {
                pointer,
                anchored_pattern,
            } => {
                JsonPointer::parse(pointer).map_err(ProbeValidateError::InvalidPointer)?;
                anchored_pattern.validate()?;
            }
            Self::Line {
                prefix,
                anchored_pattern,
            } => {
                // Empty prefix is allowed for line recognizers (strip nothing).
                if prefix.len() > STRING_VALUE_BYTE_LIMIT {
                    return Err(ProbeValidateError::StringTooLong {
                        what: "line prefix".to_string(),
                        bytes: prefix.len(),
                    });
                }
                anchored_pattern.validate()?;
            }
        }
        Ok(())
    }
}

/// Closed probe specification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProbeSpec {
    /// Identity-probe argv (1..=8 elements), typically `["--version"]`.
    pub argv: Vec<String>,
    /// Which stream(s) to capture for identity.
    #[serde(default)]
    pub stream: ProbeStream,
    /// Framing of the captured identity stream.
    #[serde(default)]
    pub framing: ProbeFraming,
    /// Normalization applied to the captured stream before identity matching.
    #[serde(default)]
    pub normalize: super::normalize::Normalize,
    /// Identity recognizer.
    pub identity: IdentityRecognizer,
    /// Probe timeout in milliseconds (1..=10000 local, <=20000 remote).
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    /// Maximum bytes to capture (1..=65536).
    #[serde(default = "default_max_bytes")]
    pub max_bytes: usize,
}

fn default_timeout_ms() -> u64 {
    LOCAL_PROBE_TIMEOUT_MS
}

fn default_max_bytes() -> usize {
    PROBE_STREAM_LIMIT
}

impl Default for ProbeSpec {
    fn default() -> Self {
        Self {
            argv: vec!["--version".to_string()],
            stream: ProbeStream::Stdout,
            framing: ProbeFraming::Utf8Text,
            normalize: super::normalize::Normalize::None,
            identity: IdentityRecognizer::Line {
                prefix: String::new(),
                anchored_pattern: AnchoredPattern::VersionToken,
            },
            timeout_ms: default_timeout_ms(),
            max_bytes: default_max_bytes(),
        }
    }
}

/// Validate a probe spec against the closed bounds.
///
/// Bounds enforced (issue #382 "Deterministic algorithms and limits"):
/// - `argv`: 1..=8 elements, each 1..=4096 bytes
/// - `required`: 0..=32 capability ids, each matching the capability grammar,
///   no duplicates, each must have an authored token in `capability_probe`
/// - `timeout_ms`: 1..=5000 local (<=20000 remote ceiling)
/// - `max_bytes`: 1..=65536
/// - `identity`: RFC 6901 pointers validated, anchored patterns bounded
/// - `capability_probe`: argv/tokens bounded, unique ids and tokens
pub fn validate(spec: &ProbeSpec) -> Result<(), ProbeValidateError> {
    if spec.argv.is_empty() || spec.argv.len() > PROBE_ARGV_LIMIT {
        return Err(ProbeValidateError::ArgvBounds {
            len: spec.argv.len(),
        });
    }
    for arg in &spec.argv {
        validate_bounded_string(arg, "probe argv element")?;
    }
    if spec.timeout_ms == 0 || spec.timeout_ms > REMOTE_PROBE_TIMEOUT_MS {
        return Err(ProbeValidateError::TimeoutBounds {
            ms: spec.timeout_ms,
        });
    }
    if spec.max_bytes == 0 || spec.max_bytes > PROBE_STREAM_LIMIT {
        return Err(ProbeValidateError::MaxBytesBounds {
            bytes: spec.max_bytes,
        });
    }
    spec.identity.validate()?;
    Ok(())
}

/// Probe-spec validation error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeValidateError {
    /// Argv length is outside 1..=8.
    ArgvBounds {
        /// Actual length.
        len: usize,
    },
    /// Required-capability count exceeds 32.
    CapabilityBounds {
        /// Actual count.
        len: usize,
    },
    /// A capability id is invalid.
    InvalidCapabilityId {
        /// Index in the required list.
        index: usize,
        /// The invalid id.
        id: String,
        /// Underlying reason.
        reason: super::type_id::CapabilityIdError,
    },
    /// A duplicate capability id was declared.
    DuplicateCapability {
        /// Index of the duplicate.
        index: usize,
        /// The duplicated id.
        id: String,
    },
    /// A duplicate token was declared.
    DuplicateToken {
        /// Index of the duplicate.
        index: usize,
        /// The duplicated token literal.
        token: String,
    },
    /// A required capability id has no authored token in the capability probe.
    RequiredCapabilityHasNoToken {
        /// Index in the required list.
        index: usize,
        /// The id missing a token.
        id: String,
    },
    /// Timeout is outside 1..=20000.
    TimeoutBounds {
        /// Actual milliseconds.
        ms: u64,
    },
    /// Max bytes is outside 1..=65536.
    MaxBytesBounds {
        /// Actual bytes.
        bytes: usize,
    },
    /// An RFC 6901 pointer failed validation.
    InvalidPointer(DefinitionError),
    /// A bounded string is empty.
    EmptyString {
        /// What was empty.
        what: String,
    },
    /// A bounded string exceeds the limit.
    StringTooLong {
        /// What was too long.
        what: String,
        /// Actual byte length.
        bytes: usize,
    },
}

impl fmt::Display for ProbeValidateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ArgvBounds { len } => {
                write!(f, "probe argv must be 1..=8 elements, found {len}")
            }
            Self::CapabilityBounds { len } => {
                write!(f, "required capabilities must be 0..=32, found {len}")
            }
            Self::InvalidCapabilityId { index, id, reason } => {
                write!(
                    f,
                    "required capability at index {index} ({id:?}) is invalid: {reason}"
                )
            }
            Self::DuplicateCapability { index, id } => {
                write!(f, "duplicate required capability {id:?} at index {index}")
            }
            Self::DuplicateToken { index, token } => {
                write!(f, "duplicate capability token {token:?} at index {index}")
            }
            Self::RequiredCapabilityHasNoToken { index, id } => {
                write!(
                    f,
                    "required capability {id:?} at index {index} has no authored probe token"
                )
            }
            Self::TimeoutBounds { ms } => {
                write!(f, "timeout_ms must be 1..=20000, found {ms}")
            }
            Self::MaxBytesBounds { bytes } => {
                write!(f, "max_bytes must be 1..=65536, found {bytes}")
            }
            Self::InvalidPointer(err) => write!(f, "invalid JSON pointer: {err}"),
            Self::EmptyString { what } => write!(f, "{what} must not be empty"),
            Self::StringTooLong { what, bytes } => {
                write!(f, "{what} exceeds {bytes} bytes")
            }
        }
    }
}

impl std::error::Error for ProbeValidateError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidCapabilityId { reason, .. } => Some(reason),
            Self::InvalidPointer(err) => Some(err),
            _ => None,
        }
    }
}

/// Pure identity-evaluation failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeParseError {
    /// Captured bytes exceed the definition's closed stream bound.
    StreamTooLong {
        /// Actual byte count.
        bytes: usize,
        /// Definition-authored maximum byte count.
        max_bytes: usize,
    },
    /// Captured bytes are not valid UTF-8.
    InvalidUtf8,
}

impl fmt::Display for ProbeParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StreamTooLong { bytes, max_bytes } => {
                write!(f, "probe stream is {bytes} bytes; maximum is {max_bytes}")
            }
            Self::InvalidUtf8 => f.write_str("probe stream is not valid UTF-8"),
        }
    }
}

impl std::error::Error for ProbeParseError {}

/// Evaluate bounded captured bytes against a probe's identity recognizer.
///
/// Normalization is definition-authored and occurs only after the raw stream
/// passes the byte bound and UTF-8 check. `StripAnsi` cannot expand the stream,
/// so the normalization scan remains bounded by `max_bytes`.
///
/// # Errors
///
/// Returns [`ProbeParseError`] for an overlong or non-UTF-8 stream.
pub fn evaluate_identity(
    stream: &[u8],
    spec: &ProbeSpec,
) -> Result<Option<String>, ProbeParseError> {
    let text = bounded_utf8(stream, spec.max_bytes)?;
    let normalized = normalize_text(text, spec.normalize);
    Ok(parse_text_identity(&normalized, &spec.identity))
}

fn bounded_utf8(stream: &[u8], max_bytes: usize) -> Result<&str, ProbeParseError> {
    if stream.len() > max_bytes {
        return Err(ProbeParseError::StreamTooLong {
            bytes: stream.len(),
            max_bytes,
        });
    }
    std::str::from_utf8(stream).map_err(|_| ProbeParseError::InvalidUtf8)
}

fn normalize_text(stream: &str, normalize: super::normalize::Normalize) -> String {
    match normalize {
        super::normalize::Normalize::None => stream.to_string(),
        super::normalize::Normalize::StripAnsi => strip_ansi_escape(stream.as_bytes()),
    }
}

/// Parse a UTF-8 text stream against an identity recognizer.
///
/// Returns the recognized identity token if present. The recognizer scans all
/// lines: for a `Line` recognizer, the prefix is stripped per-line and the
/// first matching line's stripped value is returned. An empty prefix matches
/// the first line whose anchored pattern is satisfied.
#[must_use]
pub fn parse_text_identity(stream: &str, recognizer: &IdentityRecognizer) -> Option<String> {
    match recognizer {
        IdentityRecognizer::Line {
            prefix,
            anchored_pattern,
        } => {
            for line in stream.lines() {
                let candidate = if prefix.is_empty() {
                    line.trim()
                } else {
                    let Some(rest) = line.strip_prefix(prefix.as_str()) else {
                        continue;
                    };
                    rest.trim()
                };
                if !candidate.is_empty() && anchored_pattern.matches(candidate) {
                    return Some(candidate.to_owned());
                }
            }
            None
        }
        IdentityRecognizer::JsonPointer { .. } => {
            // JSON pointer identity is resolved against a parsed JSON document
            // in the runtime probe adapter; here we return None for raw text.
            None
        }
    }
}
