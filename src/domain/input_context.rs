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

/// Validated child-to-parent search order for one input state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextStack {
    contexts: Vec<ContextId>,
    terminal_capture: bool,
}

/// Failure to construct a complete ordered context stack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextStackError {
    InvalidContext(ContextIdError),
    DuplicateContext(ContextId),
    MissingTerminalContext,
}

impl fmt::Display for ContextStackError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid context stack: {self:?}")
    }
}
impl std::error::Error for ContextStackError {}

impl ContextStack {
    /// Parse and validate one child-to-parent order.
    pub fn from_ordered<'a>(
        values: impl IntoIterator<Item = &'a str>,
        terminal_capture: bool,
    ) -> Result<Self, ContextStackError> {
        let mut contexts = Vec::new();
        for value in values {
            let context = ContextId::parse(value).map_err(ContextStackError::InvalidContext)?;
            if contexts.contains(&context) {
                return Err(ContextStackError::DuplicateContext(context));
            }
            contexts.push(context);
        }
        if terminal_capture && contexts.is_empty() {
            return Err(ContextStackError::MissingTerminalContext);
        }
        Ok(Self {
            contexts,
            terminal_capture,
        })
    }

    /// Iterate contexts from highest-precedence child to final parent.
    pub fn iter(&self) -> impl Iterator<Item = &ContextId> {
        self.contexts.iter()
    }

    /// Whether terminal capture owns ordinary input for this stack.
    #[must_use]
    pub const fn is_terminal_capture(&self) -> bool {
        self.terminal_capture
    }

    /// Whether no context is active.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.contexts.is_empty()
    }
}
