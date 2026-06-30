//! Typed errors for the harness scenario layer.
//!
//! @plan PLAN-20260629-TMUX-HARNESS.P01
//! @requirement REQ-TMUX-HARNESS-001

/// Errors produced by scenario parsing, validation, and macro expansion.
///
/// Every variant carries structured context so callers can render actionable
/// operator feedback without parsing error strings.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScenarioError {
    /// Malformed JSON or a JSON value that does not match the expected shape.
    Json { message: String },
    /// A step object used an unrecognized kind key (e.g. `{"fly": true}`).
    UnknownStepKind { kind: String },
    /// A required field was missing from the JSON document.
    MissingField { field: String, context: String },
    /// A config field was out of its valid range.
    InvalidConfig { field: String, reason: String },
    /// A macro definition was structurally valid JSON but invalid as a model.
    InvalidMacro { name: String, reason: String },
    /// A macro invocation referenced a name that is not defined.
    UnknownMacro { name: String },
    /// A macro invocation supplied the wrong number of arguments.
    MacroArityMismatch {
        name: String,
        expected: usize,
        provided: usize,
    },
    /// A macro invocation is missing a required argument.
    MissingMacroArg { name: String, param: String },
    /// Macro expansion detected a cycle (a macro directly or transitively
    /// invokes itself).
    MacroCycle { chain: Vec<String> },
    /// A step had a structurally valid shape but an invalid argument value.
    InvalidStep { reason: String },
}

impl std::fmt::Display for ScenarioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json { message } => write!(f, "scenario JSON error: {message}"),
            Self::UnknownStepKind { kind } => write!(f, "unknown step kind: '{kind}'"),
            Self::MissingField { field, context } => {
                write!(f, "missing required field '{field}' in {context}")
            }
            Self::InvalidConfig { field, reason } => {
                write!(f, "invalid config field '{field}': {reason}")
            }
            Self::InvalidMacro { name, reason } => {
                write!(f, "invalid macro '{name}': {reason}")
            }
            Self::UnknownMacro { name } => write!(f, "unknown macro: '{name}'"),
            Self::MacroArityMismatch {
                name,
                expected,
                provided,
            } => {
                write!(
                    f,
                    "macro '{name}' expects {expected} argument(s), got {provided}"
                )
            }
            Self::MissingMacroArg { name, param } => {
                write!(f, "macro '{name}' is missing argument '{param}'")
            }
            Self::MacroCycle { chain } => {
                write!(f, "macro cycle detected: {}", chain.join(" -> "))
            }
            Self::InvalidStep { reason } => write!(f, "invalid step: {reason}"),
        }
    }
}

impl std::error::Error for ScenarioError {}
