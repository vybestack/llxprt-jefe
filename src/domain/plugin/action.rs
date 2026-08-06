//! Action declarations contributed by a package
//! (issue #389 CW-09, acceptance rows D4 and D5).
//!
//! An [`Action`] is validated on construction. Everything checkable from the
//! declaration alone is checked here: context and argument bounds, the timeout
//! range, duplicate contexts, arguments and outcomes, and the rule that a
//! destructive action must confirm before it runs.
//!
//! Nothing here invokes anything. Owner-prefix rules and reference resolution
//! need the whole manifest, so they belong to manifest validation.

use std::fmt;

use super::field::Field;
use super::limits::{
    ACTION_ARGUMENT_LIMIT, ACTION_CONTEXT_LIMIT, ACTION_CONTEXT_MINIMUM,
    ACTION_TIMEOUT_SECONDS_LIMIT, ACTION_TIMEOUT_SECONDS_MINIMUM,
};
use crate::domain::Id;

/// Whether and how an action confirms before it runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ActionConfirmation {
    /// The action runs without confirmation.
    None,
    /// The host confirms before the provider is invoked at all.
    HostBeforeInvoke,
    /// The provider asks for confirmation part-way through.
    ProviderContinuation,
}

impl ActionConfirmation {
    /// Every confirmation kind, in declaration order.
    pub const ALL: [Self; 3] = [
        Self::None,
        Self::HostBeforeInvoke,
        Self::ProviderContinuation,
    ];

    /// The lower-kebab-case name used on the wire.
    #[must_use]
    pub const fn as_wire(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::HostBeforeInvoke => "host-before-invoke",
            Self::ProviderContinuation => "provider-continuation",
        }
    }

    /// Resolve a wire name, exactly and case-sensitively.
    #[must_use]
    pub fn from_wire(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|value_| value_.as_wire() == value)
    }

    /// Whether the operator is asked before the provider is invoked.
    #[must_use]
    pub const fn confirms_before_invoke(self) -> bool {
        matches!(self, Self::HostBeforeInvoke | Self::ProviderContinuation)
    }
}

/// What an action is permitted to ask the host to do when it completes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ActionOutcome {
    /// Navigate to a route the package declared.
    NavigateDeclaredRoute,
    /// Refresh the resource currently in view.
    RefreshCurrentResource,
    /// Show a notice.
    Notice,
    /// Replace a panel the package owns.
    ReplaceOwnedPanel,
    /// Ask the host to confirm.
    RequestHostConfirmation,
    /// Close a panel the package owns.
    CloseOwnedPanel,
}

impl ActionOutcome {
    /// Every outcome, in declaration order.
    pub const ALL: [Self; 6] = [
        Self::NavigateDeclaredRoute,
        Self::RefreshCurrentResource,
        Self::Notice,
        Self::ReplaceOwnedPanel,
        Self::RequestHostConfirmation,
        Self::CloseOwnedPanel,
    ];

    /// The lower-kebab-case name used on the wire.
    #[must_use]
    pub const fn as_wire(self) -> &'static str {
        match self {
            Self::NavigateDeclaredRoute => "navigate-declared-route",
            Self::RefreshCurrentResource => "refresh-current-resource",
            Self::Notice => "notice",
            Self::ReplaceOwnedPanel => "replace-owned-panel",
            Self::RequestHostConfirmation => "request-host-confirmation",
            Self::CloseOwnedPanel => "close-owned-panel",
        }
    }

    /// Resolve a wire name, exactly and case-sensitively.
    #[must_use]
    pub fn from_wire(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|value_| value_.as_wire() == value)
    }
}

/// An unvalidated action declaration, as read from a manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionDraft {
    /// Action identifier, owned by the declaring package.
    pub id: Id,
    /// Short operator-facing name.
    pub label: String,
    /// Longer operator-facing description.
    pub description: String,
    /// Grouping category.
    pub category: Id,
    /// Input contexts the action appears in.
    pub contexts: Vec<Id>,
    /// Arguments collected before invocation.
    pub arguments: Vec<Field>,
    /// Invocation timeout in seconds.
    pub timeout_seconds: u32,
    /// Whether the action destroys something.
    pub destructive: bool,
    /// Whether and how the action confirms.
    pub confirmation: ActionConfirmation,
    /// Provider-side handler name.
    pub handler: Id,
    /// Outcomes the action may request.
    pub allowed_outcomes: Vec<ActionOutcome>,
}

/// A validated action declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    draft: ActionDraft,
}

impl Action {
    /// Validate an action declaration.
    ///
    /// # Errors
    ///
    /// Returns [`ActionError`] when text is blank, a bound is exceeded, an id
    /// or outcome repeats, the timeout is out of range, or a destructive action
    /// declares no confirmation.
    pub fn parse(draft: ActionDraft) -> Result<Self, ActionError> {
        validate_text(&draft)?;
        validate_contexts(&draft.contexts)?;
        validate_arguments(&draft.arguments)?;
        validate_timeout(draft.timeout_seconds)?;
        validate_outcomes(&draft.allowed_outcomes)?;
        if draft.destructive && !draft.confirmation.confirms_before_invoke() {
            return Err(ActionError::DestructiveWithoutConfirmation);
        }
        Ok(Self { draft })
    }

    /// The action identifier.
    #[must_use]
    pub const fn id(&self) -> &Id {
        &self.draft.id
    }

    /// The operator-facing label.
    #[must_use]
    pub fn label(&self) -> &str {
        &self.draft.label
    }

    /// The operator-facing description.
    #[must_use]
    pub fn description(&self) -> &str {
        &self.draft.description
    }

    /// The grouping category.
    #[must_use]
    pub const fn category(&self) -> &Id {
        &self.draft.category
    }

    /// Input contexts this action appears in.
    #[must_use]
    pub fn contexts(&self) -> &[Id] {
        &self.draft.contexts
    }

    /// Arguments collected before invocation.
    #[must_use]
    pub fn arguments(&self) -> &[Field] {
        &self.draft.arguments
    }

    /// Invocation timeout in seconds.
    #[must_use]
    pub const fn timeout_seconds(&self) -> u32 {
        self.draft.timeout_seconds
    }

    /// Whether the action destroys something.
    #[must_use]
    pub const fn destructive(&self) -> bool {
        self.draft.destructive
    }

    /// Whether and how the action confirms.
    #[must_use]
    pub const fn confirmation(&self) -> ActionConfirmation {
        self.draft.confirmation
    }

    /// The provider-side handler name.
    #[must_use]
    pub const fn handler(&self) -> &Id {
        &self.draft.handler
    }

    /// Outcomes this action may request.
    #[must_use]
    pub fn allowed_outcomes(&self) -> &[ActionOutcome] {
        &self.draft.allowed_outcomes
    }
}

fn validate_text(draft: &ActionDraft) -> Result<(), ActionError> {
    if draft.label.trim().is_empty() {
        return Err(ActionError::BlankLabel);
    }
    if draft.description.trim().is_empty() {
        return Err(ActionError::BlankDescription);
    }
    Ok(())
}

fn validate_contexts(contexts: &[Id]) -> Result<(), ActionError> {
    if contexts.len() < ACTION_CONTEXT_MINIMUM {
        return Err(ActionError::NoContexts);
    }
    if contexts.len() > ACTION_CONTEXT_LIMIT {
        return Err(ActionError::TooManyContexts {
            len: contexts.len(),
        });
    }
    for (index, context) in contexts.iter().enumerate() {
        if contexts[..index].contains(context) {
            return Err(ActionError::DuplicateContext {
                id: context.as_str().to_owned(),
            });
        }
    }
    Ok(())
}

fn validate_arguments(arguments: &[Field]) -> Result<(), ActionError> {
    if arguments.len() > ACTION_ARGUMENT_LIMIT {
        return Err(ActionError::TooManyArguments {
            len: arguments.len(),
        });
    }
    for (index, argument) in arguments.iter().enumerate() {
        if arguments[..index]
            .iter()
            .any(|earlier| earlier.id() == argument.id())
        {
            return Err(ActionError::DuplicateArgument {
                id: argument.id().as_str().to_owned(),
            });
        }
    }
    Ok(())
}

fn validate_timeout(seconds: u32) -> Result<(), ActionError> {
    if (ACTION_TIMEOUT_SECONDS_MINIMUM..=ACTION_TIMEOUT_SECONDS_LIMIT).contains(&seconds) {
        Ok(())
    } else {
        Err(ActionError::TimeoutOutOfRange { seconds })
    }
}

fn validate_outcomes(outcomes: &[ActionOutcome]) -> Result<(), ActionError> {
    for (index, outcome) in outcomes.iter().enumerate() {
        if outcomes[..index].contains(outcome) {
            return Err(ActionError::DuplicateOutcome {
                outcome: outcome.as_wire().to_owned(),
            });
        }
    }
    Ok(())
}

/// Why an action declaration is invalid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionError {
    /// The label is empty or only whitespace.
    BlankLabel,
    /// The description is empty or only whitespace.
    BlankDescription,
    /// No input context was declared.
    NoContexts,
    /// More than [`ACTION_CONTEXT_LIMIT`] contexts.
    TooManyContexts { len: usize },
    /// The same context was declared twice.
    DuplicateContext { id: String },
    /// More than [`ACTION_ARGUMENT_LIMIT`] arguments.
    TooManyArguments { len: usize },
    /// Two arguments share an identifier.
    DuplicateArgument { id: String },
    /// The timeout is outside 1..=600 seconds.
    TimeoutOutOfRange { seconds: u32 },
    /// The same outcome was declared twice.
    DuplicateOutcome { outcome: String },
    /// A destructive action declared no confirmation.
    DestructiveWithoutConfirmation,
}

impl fmt::Display for ActionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BlankLabel => formatter.write_str("an action label may not be blank"),
            Self::BlankDescription => formatter.write_str("an action description may not be blank"),
            Self::NoContexts => {
                formatter.write_str("an action must declare at least one input context")
            }
            Self::TooManyContexts { len } => {
                write!(
                    formatter,
                    "{len} contexts exceeds the {ACTION_CONTEXT_LIMIT} limit"
                )
            }
            Self::DuplicateContext { id } => write!(formatter, "context {id:?} is declared twice"),
            Self::TooManyArguments { len } => {
                write!(
                    formatter,
                    "{len} arguments exceeds the {ACTION_ARGUMENT_LIMIT} limit"
                )
            }
            Self::DuplicateArgument { id } => {
                write!(formatter, "argument {id:?} is declared twice")
            }
            Self::TimeoutOutOfRange { seconds } => write!(
                formatter,
                "timeout {seconds}s is outside {ACTION_TIMEOUT_SECONDS_MINIMUM}..={ACTION_TIMEOUT_SECONDS_LIMIT}"
            ),
            Self::DuplicateOutcome { outcome } => {
                write!(formatter, "outcome {outcome:?} is declared twice")
            }
            Self::DestructiveWithoutConfirmation => {
                formatter.write_str("a destructive action must declare a confirmation")
            }
        }
    }
}

impl std::error::Error for ActionError {}

#[cfg(test)]
#[path = "action_tests.rs"]
mod tests;
