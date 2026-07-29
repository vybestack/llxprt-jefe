//! Validated identifiers and ordered input-context stacks.

use std::fmt;

pub const CONTEXT_ID_BYTE_LIMIT: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContextId(String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextIdError {
    pub raw: String,
    pub reason: ContextIdErrorReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextIdErrorReason {
    Length,
    Grammar,
}

impl fmt::Display for ContextIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let reason = match self.reason {
            ContextIdErrorReason::Length => "must be 1..=128 bytes",
            ContextIdErrorReason::Grammar => "must match [a-z][a-z0-9]*(?:[.-][a-z0-9]+)*",
        };
        write!(formatter, "invalid context id {:?}: {reason}", self.raw)
    }
}
impl std::error::Error for ContextIdError {}

impl ContextId {
    pub fn parse(value: &str) -> Result<Self, ContextIdError> {
        if value.is_empty() || value.len() > CONTEXT_ID_BYTE_LIMIT {
            return Err(ContextIdError {
                raw: value.to_owned(),
                reason: ContextIdErrorReason::Length,
            });
        }
        if !valid_context_id(value.as_bytes()) {
            return Err(ContextIdError {
                raw: value.to_owned(),
                reason: ContextIdErrorReason::Grammar,
            });
        }
        Ok(Self(value.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn valid_context_id(bytes: &[u8]) -> bool {
    let Some(first) = bytes.first() else {
        return false;
    };
    if !first.is_ascii_lowercase() {
        return false;
    }
    let mut separator = false;
    for byte in &bytes[1..] {
        if byte.is_ascii_lowercase() || byte.is_ascii_digit() {
            separator = false;
        } else if matches!(byte, b'.' | b'-') && !separator {
            separator = true;
        } else {
            return false;
        }
    }
    !separator
}

impl fmt::Display for ContextId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Build the modal-to-global search order without dropping malformed levels.
///
/// # Errors
///
/// Returns [`ContextIdError`] for the first invalid present level. Silently
/// omitting one would broaden resolution to a parent context.
pub fn resolve_context_stack(
    modal: Option<&str>,
    focused_editor_or_chooser: Option<&str>,
    focused_panel: Option<&str>,
    screen: Option<&str>,
    global: Option<&str>,
) -> Result<Vec<ContextId>, ContextIdError> {
    [
        modal,
        focused_editor_or_chooser,
        focused_panel,
        screen,
        global,
    ]
    .into_iter()
    .flatten()
    .map(ContextId::parse)
    .collect()
}
