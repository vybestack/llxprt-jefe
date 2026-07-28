//! Core closed agent-definition contract types (issue #382 CW-02).
//!
//! These are the pure, I/O-free type definitions for the four-agent registry.
//! Product knowledge lives only in the shipped-definition data module
//! ([`crate::domain::agent_definition::shipped`]); this module contains no
//! product tokens. Every type here is a closed contract: there is no generic
//! JSON value, shell template, token-splitting, setup command, script, or
//! raw-argument field.

use std::ffi::OsString;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::agent_candidate_fingerprint::CandidateFingerprint;
use crate::agent_candidate_path::AgentWrapperKind;

use super::sha256::DefinitionSha256;
use super::signature::LaunchSignatureV1;
use super::type_id::AgentTypeId;

// ---------------------------------------------------------------------------
// Support / Availability / Operation / Target
// ---------------------------------------------------------------------------

/// Per-cell supported/unsupported declaration with an exact reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Support {
    /// Supported cell.
    Supported,
    /// Unsupported cell with the declared reason.
    Unsupported {
        /// Exact declared reason shown to the user.
        reason: String,
    },
}

impl Support {
    /// Construct the supported variant.
    #[must_use]
    pub fn supported() -> Self {
        Self::Supported
    }

    /// Construct the unsupported variant with a reason.
    #[must_use]
    pub fn unsupported(reason: impl Into<String>) -> Self {
        Self::Unsupported {
            reason: reason.into(),
        }
    }

    /// Whether this is the unsupported variant.
    #[must_use]
    pub fn is_unsupported(&self) -> bool {
        matches!(self, Self::Unsupported { .. })
    }

    /// The declared reason if unsupported.
    #[must_use]
    pub fn reason(&self) -> Option<&str> {
        match self {
            Self::Unsupported { reason } => Some(reason),
            Self::Supported => None,
        }
    }
}

/// How an operation emits its initial prompt, when supported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PromptShape {
    /// No initial prompt (operation has no positional/option prompt).
    None,
    /// Initial prompt is a positional argv element.
    InitialPositional,
    /// Initial prompt is an interactive option (e.g. `-i`).
    InteractiveOption,
    /// Sentinel used only by `Default`; never serialized by shipped data.
    #[default]
    NoneDefault,
}

/// Per-operation support and prompt shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationSupport {
    /// Supported/unsupported declaration.
    pub supported: Support,
    /// How the initial prompt is emitted, when supported.
    #[serde(default)]
    pub prompt: PromptShape,
}

impl Default for OperationSupport {
    fn default() -> Self {
        Self {
            supported: Support::unsupported("operation not declared"),
            prompt: PromptShape::None,
        }
    }
}

/// The four closed operations an agent may perform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    /// Normal interactive session.
    #[default]
    Normal,
    /// Resume an existing session.
    Resume,
    /// Fresh issue-send session.
    FreshIssue,
    /// Fresh PR-send session.
    FreshPullRequest,
}

impl Operation {
    /// Whether this is a fresh-prompt operation (FreshIssue or FreshPullRequest).
    #[must_use]
    pub fn is_fresh(&self) -> bool {
        matches!(self, Self::FreshIssue | Self::FreshPullRequest)
    }

    /// Whether this is the resume operation.
    #[must_use]
    pub const fn is_resume(self) -> bool {
        matches!(self, Self::Resume)
    }
}

/// Per-definition operation support matrix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct OperationMatrix {
    /// Normal operation support.
    pub normal: OperationSupport,
    /// Resume operation support.
    pub resume: OperationSupport,
    /// Fresh-issue operation support.
    pub fresh_issue: OperationSupport,
    /// Fresh-PR operation support.
    pub fresh_pull_request: OperationSupport,
}

impl OperationMatrix {
    /// Resolve support for a given operation.
    #[must_use]
    pub fn support_for(&self, operation: Operation) -> &OperationSupport {
        match operation {
            Operation::Normal => &self.normal,
            Operation::Resume => &self.resume,
            Operation::FreshIssue => &self.fresh_issue,
            Operation::FreshPullRequest => &self.fresh_pull_request,
        }
    }

    /// Whether any cell is unsupported (default matrix exposes gaps).
    #[must_use]
    pub fn has_any_unsupported(&self) -> bool {
        self.normal.supported.is_unsupported()
            || self.resume.supported.is_unsupported()
            || self.fresh_issue.supported.is_unsupported()
            || self.fresh_pull_request.supported.is_unsupported()
    }
}

/// Per-target support declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetSupport {
    /// Supported/unsupported declaration.
    pub supported: Support,
}

impl Default for TargetSupport {
    fn default() -> Self {
        Self {
            supported: Support::unsupported("target not declared"),
        }
    }
}

/// Per-definition target support matrix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TargetMatrix {
    /// Local target support.
    pub local: TargetSupport,
    /// Remote target support.
    pub remote: TargetSupport,
}

/// Remote execution target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RemoteTarget {
    /// Remote user (may be empty for default).
    #[serde(default)]
    pub user: String,
    /// Remote host.
    #[serde(default)]
    pub host: String,
    /// Optional SSH port.
    #[serde(default)]
    pub port: Option<u16>,
    /// Optional remote user selected after SSH login.
    #[serde(default)]
    pub run_as_user: String,
    /// Canonical remote working directory.
    #[serde(default)]
    pub canonical_cwd: PathBuf,
}

/// One execution target: local or remote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Target {
    /// Local execution with a canonical working directory.
    Local {
        /// Canonical local working directory.
        canonical_cwd: PathBuf,
    },
    /// Remote execution target.
    Remote(RemoteTarget),
}

impl Target {
    /// Whether this is the local target.
    #[must_use]
    pub fn is_local(&self) -> bool {
        matches!(self, Self::Local { .. })
    }

    /// Canonical working directory (local or remote).
    #[must_use]
    pub fn canonical_cwd(&self) -> &std::path::Path {
        match self {
            Self::Local { canonical_cwd } => canonical_cwd,
            Self::Remote(remote) => &remote.canonical_cwd,
        }
    }
}

// ---------------------------------------------------------------------------
// Probe error code
// ---------------------------------------------------------------------------

/// Closed probe/runtime diagnostic codes (issue #382 failure table).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProbeErrorCode {
    /// `AGT-E201`: definition invalid.
    Agte201,
    /// `AGT-E202`: probe malformed/timed out/failed framing.
    Agte202,
    /// `AGT-E203`: executable/target/probe/activation generation mismatch.
    Agte203,
}

impl ProbeErrorCode {
    /// Stable diagnostic string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Agte201 => "AGT-E201",
            Self::Agte202 => "AGT-E202",
            Self::Agte203 => "AGT-E203",
        }
    }

    /// Whether this is a probe-error code (AGT-E202).
    #[must_use]
    pub const fn is_probe_error(self) -> bool {
        matches!(self, Self::Agte202)
    }

    /// Whether this is the stale-generation code (AGT-E203).
    #[must_use]
    pub const fn is_generation_mismatch(self) -> bool {
        matches!(self, Self::Agte203)
    }
}

/// Runtime availability of a definition's executable on a given installation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Availability {
    /// No candidate resolved to a physical executable.
    NotFound,
    /// Executable present with identity and all required capabilities.
    InstalledCompatible {
        /// Recognized identity token.
        identity: String,
        /// Sorted, deduplicated capabilities.
        capabilities: Vec<String>,
        /// Probe generation stamp.
        generation: u64,
    },
    /// Executable present but a required capability is absent.
    InstalledIncompatible {
        /// Exact reason (including the missing capability).
        reason: String,
        /// Probe generation stamp.
        generation: u64,
    },
    /// Probe failed (framing, UTF-8, bounds, exit, timeout).
    ProbeError {
        /// Closed probe error code.
        code: ProbeErrorCode,
        /// Exact reason.
        reason: String,
        /// Probe generation stamp.
        generation: u64,
    },
}

impl Availability {
    /// Whether this is the NotFound variant.
    #[must_use]
    pub fn is_not_found(&self) -> bool {
        matches!(self, Self::NotFound)
    }

    /// Whether the executable is installed (compatible or incompatible).
    #[must_use]
    pub fn is_installed(&self) -> bool {
        matches!(
            self,
            Self::InstalledCompatible { .. } | Self::InstalledIncompatible { .. }
        )
    }

    /// Generation stamp if a probe was attempted.
    #[must_use]
    pub fn generation(&self) -> Option<u64> {
        match self {
            Self::NotFound => None,
            Self::InstalledCompatible { generation, .. }
            | Self::InstalledIncompatible { generation, .. }
            | Self::ProbeError { generation, .. } => Some(*generation),
        }
    }
}

// ---------------------------------------------------------------------------
// Preflight
// ---------------------------------------------------------------------------

/// Sandbox preflight contract gating every preparation effect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preflight {
    /// Sandbox engine name (e.g. docker), if configured.
    #[serde(default)]
    pub engine: Option<String>,
    /// Sandbox image reference, if configured.
    #[serde(default)]
    pub image: Option<String>,
    /// Required environment variable names.
    #[serde(default)]
    pub required_env: Vec<String>,
    /// Whether preflight is required for this target.
    #[serde(default = "default_required")]
    pub required: bool,
}

fn default_required() -> bool {
    true
}

impl Default for Preflight {
    fn default() -> Self {
        Self {
            engine: None,
            image: None,
            required_env: Vec::new(),
            required: true,
        }
    }
}

impl Preflight {
    /// Whether this preflight is unavailable (required but unconfigured).
    #[must_use]
    pub fn is_unavailable(&self) -> bool {
        self.required && (self.engine.is_none() || self.image.is_none())
    }

    /// Whether preflight is required for any preparation effect.
    #[must_use]
    pub const fn is_required(&self) -> bool {
        self.required
    }
}

// ---------------------------------------------------------------------------
// AgentLaunchPlan
// ---------------------------------------------------------------------------

/// One immutable, validated launch plan executed by the runtime.
///
/// Built once by the planner from a definition + typed values + operation +
/// target + probe generations. The runtime executes only this; it performs no
/// product matching. The signature excludes secrets and display-only fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentLaunchPlan {
    /// Stable agent-type id.
    pub type_id: AgentTypeId,
    /// Closed operation.
    pub operation: Operation,
    /// Content digest of the agent definition.
    pub definition_sha256: DefinitionSha256,
    /// Resolved executable path.
    pub executable: PathBuf,
    /// Full physical identity of the executable selected before planning.
    pub executable_fingerprint: CandidateFingerprint,
    /// Platform launch strategy selected before planning.
    pub executable_wrapper: AgentWrapperKind,
    /// Ordered argv elements (preserved byte-wise).
    pub argv: Vec<OsString>,
    /// Ordered environment pairs (allowlisted names only).
    pub env: Vec<(OsString, OsString)>,
    /// Canonical working directory.
    pub cwd: PathBuf,
    /// Execution target.
    pub target: Target,
    /// Probe generation stamp.
    pub probe_generation: u64,
    /// Target generation stamp.
    pub target_generation: u64,
    /// Activation generation stamp compared by the execution authorization
    /// guard (issue #382 CW02-12 / S8). Represents the generation at which
    /// the plan's activation binding is valid; a mismatch yields `AGT-E203`.
    pub activation_generation: u64,
    /// Sandbox preflight contract.
    pub preflight: Preflight,
    /// Versioned launch signature.
    pub signature: LaunchSignatureV1,
}

impl AgentLaunchPlan {
    /// Signature v1 excludes secrets and display-only values (contract).
    #[must_use]
    pub const fn signature_excludes_secrets(&self) -> bool {
        true
    }
}

impl Default for AgentLaunchPlan {
    fn default() -> Self {
        Self {
            type_id: AgentTypeId::parse("core.normal").unwrap_or_else(|_| {
                // Default plan is only used by tests that assert the contract
                // shape; a valid placeholder id keeps the default well-typed.
                AgentTypeId::from_validated("core.normal")
            }),
            operation: Operation::Normal,
            definition_sha256: DefinitionSha256::default(),
            executable: PathBuf::new(),
            executable_fingerprint: CandidateFingerprint::new(PathBuf::new(), None, None, 0, 0),
            executable_wrapper: AgentWrapperKind::Direct,
            argv: Vec::new(),
            env: Vec::new(),
            cwd: PathBuf::new(),
            target: Target::Local {
                canonical_cwd: PathBuf::new(),
            },
            probe_generation: 0,
            target_generation: 0,
            activation_generation: 0,
            preflight: Preflight::default(),
            signature: LaunchSignatureV1::default(),
        }
    }
}

#[cfg(test)]
#[path = "types_tests.rs"]
mod tests;
