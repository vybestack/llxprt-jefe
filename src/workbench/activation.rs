//! What a screen's route accepts, and which actions it requests (issue #385).
//!
//! These are the two parts of a screen definition that nothing in this issue
//! renders or executes, and they are lowered anyway. Lowering happens exactly
//! once, and the composed registry is the only thing later capabilities read: a
//! consumer that had to reach back to the external syntax would be a second
//! parser for a grammar that is supposed to have one.
//!
//! - The activation schema is what a route validates an activation against
//!   before it navigates to the screen. The navigation capability builds its
//!   route declaration from a screen's `route` plus this schema.
//! - The binding references are the actions a screen asks the keymap to make
//!   reachable while it is focused.
//!
//! Both are resolved against immutable registries during lowering, so a
//! definition can request an action that exists and cannot describe a field kind
//! the host does not implement. Neither carries a value: an activation field
//! declares a *shape*, and there is deliberately no secret kind, so no part of
//! this can hold a secret.

use crate::domain::Id;
use crate::domain::action_registry::ActionId;
use crate::domain::input_context::ContextId;

/// The closed set of value kinds a route activation field may declare.
///
/// There is no secret kind. A screen definition is a file a user can share, and
/// a schema that could name a secret field would invite one into it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ActivationKind {
    /// A required boolean.
    Boolean,
    /// A boolean that may be absent.
    OptionalBoolean,
    /// Free text.
    Text,
    /// A signed integer.
    Integer,
    /// One of a declared set of strings.
    Enumerated {
        /// The permitted values, in declaration order.
        permitted: Vec<String>,
    },
    /// A filesystem path.
    Path,
    /// A list of strings.
    TextList,
}

impl ActivationKind {
    /// The stable text naming this kind, which is also its external spelling.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Boolean => "boolean",
            Self::OptionalBoolean => "optional-boolean",
            Self::Text => "string",
            Self::Integer => "integer",
            Self::Enumerated { .. } => "enum",
            Self::Path => "path",
            Self::TextList => "string-list",
        }
    }
}

/// One field a screen's route accepts when something navigates to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivationField {
    /// Field name within the screen's namespace.
    pub name: Id,
    /// The value kind this field carries.
    pub kind: ActivationKind,
}

/// One action a screen asks to be reachable in a given input context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenBinding {
    /// The input context the request applies in.
    pub context: ContextId,
    /// The action requested, which the immutable inventory already publishes.
    pub action: ActionId,
}
