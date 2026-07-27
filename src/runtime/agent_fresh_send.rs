//! Definition-driven post-preflight fresh-send assembly (issue #382 S11).
//!
//! This boundary accepts only [`PreflightCleared`], so prompt argv cannot be
//! produced before execution authorization and preflight have succeeded. The
//! selected operation's declaration is the sole authority for prompt shape.

use std::ffi::OsString;

use crate::domain::RemoteRepositorySettings;
use crate::domain::agent_definition::{
    AgentDefinition, AgentLaunchPlan, Operation, PromptShape, Support, Target,
};

use super::agent_preflight::PreflightCleared;
use super::agent_remote_plan::{RemotePlanError, RemoteTranscript, transcript_from_plan};

/// A launch plan with exactly one fresh prompt emitted after preflight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedFreshSend {
    plan: AgentLaunchPlan,
    prompt_index: usize,
}

impl PreparedFreshSend {
    /// The immutable post-preflight launch plan.
    #[must_use]
    pub const fn plan(&self) -> &AgentLaunchPlan {
        &self.plan
    }

    /// Index of the one prompt argv element emitted by this boundary.
    #[must_use]
    pub const fn prompt_index(&self) -> usize {
        self.prompt_index
    }

    /// Convert a prepared remote plan into the audited SSH transcript.
    ///
    /// # Errors
    ///
    /// Returns the existing typed remote validation or serialization error.
    pub fn remote_transcript(
        &self,
        settings: &RemoteRepositorySettings,
    ) -> Result<RemoteTranscript, RemotePlanError> {
        transcript_from_plan(self.plan.clone(), settings)
    }
}

/// Typed zero-effect rejection from fresh-send assembly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FreshSendRejection {
    /// The plan is not a fresh Issue/PR operation.
    NotFreshOperation { operation: Operation },
    /// The definition does not match the authorized plan identity.
    DefinitionMismatch,
    /// The operation is unsupported with its exact declared reason.
    Unsupported { reason: String },
    /// A supported fresh operation omitted a prompt shape.
    PromptShapeMissing,
}

impl std::fmt::Display for FreshSendRejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFreshOperation { operation } => {
                write!(f, "operation {operation:?} is not a fresh send")
            }
            Self::DefinitionMismatch => {
                f.write_str("authorized plan does not match the selected agent definition")
            }
            Self::Unsupported { reason } => f.write_str(reason),
            Self::PromptShapeMissing => {
                f.write_str("supported fresh operation does not declare a prompt shape")
            }
        }
    }
}

impl std::error::Error for FreshSendRejection {}

/// Append one exact prompt to an authorized, preflight-cleared fresh plan.
///
/// `prompt` must already contain the final Issue/PR formatting. This function
/// does not truncate, rewrite, persist, or log it.
///
/// # Errors
///
/// Returns a typed rejection before cloning or appending prompt argv when the
/// plan is not fresh, its definition identity changed, or support/prompt shape
/// is invalid.
pub fn prepare_fresh_send(
    definition: &AgentDefinition,
    cleared: PreflightCleared<'_>,
    prompt: &str,
) -> Result<PreparedFreshSend, FreshSendRejection> {
    let authorized = cleared.plan();
    if !authorized.operation.is_fresh() {
        return Err(FreshSendRejection::NotFreshOperation {
            operation: authorized.operation,
        });
    }
    if authorized.type_id != definition.id || authorized.definition_sha256 != definition.sha256() {
        return Err(FreshSendRejection::DefinitionMismatch);
    }

    let operation = definition.operations.support_for(authorized.operation);
    if let Support::Unsupported { reason } = &operation.supported {
        return Err(FreshSendRejection::Unsupported {
            reason: reason.clone(),
        });
    }

    let mut plan = authorized.clone();
    let prompt_index = match operation.prompt {
        PromptShape::InitialPositional => {
            let index = plan.argv.len();
            plan.argv.push(OsString::from(prompt));
            index
        }
        PromptShape::InteractiveOption => {
            plan.argv.push(OsString::from("-i"));
            let index = plan.argv.len();
            plan.argv.push(OsString::from(prompt));
            index
        }
        PromptShape::None | PromptShape::NoneDefault => {
            return Err(FreshSendRejection::PromptShapeMissing);
        }
    };

    Ok(PreparedFreshSend { plan, prompt_index })
}

/// Return the declared zero-effect reason before planning or preflight.
pub fn fresh_send_support(
    definition: &AgentDefinition,
    operation: Operation,
    target: &Target,
) -> Result<(), FreshSendRejection> {
    if !operation.is_fresh() {
        return Err(FreshSendRejection::NotFreshOperation { operation });
    }
    let operation_support = &definition.operations.support_for(operation).supported;
    if let Support::Unsupported { reason } = operation_support {
        return Err(FreshSendRejection::Unsupported {
            reason: reason.clone(),
        });
    }
    let target_support = match target {
        Target::Local { .. } => &definition.targets.local.supported,
        Target::Remote(_) => &definition.targets.remote.supported,
    };
    match target_support {
        Support::Supported => Ok(()),
        Support::Unsupported { reason } => Err(FreshSendRejection::Unsupported {
            reason: reason.clone(),
        }),
    }
}

#[cfg(test)]
#[path = "agent_fresh_send_tests.rs"]
mod tests;
