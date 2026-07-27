//! Definition-driven immutable REMOTE `AgentLaunchPlan` generation
//! (issue #382 CW02-07 / S9).
//!
//! This is a **pure** planner: no I/O, no process spawn, no filesystem access,
//! no environment reads, and no side effects. It consumes a validated
//! [`AgentDefinition`], typed field values, a chosen [`Operation`], a remote
//! [`Target`], compatible current probe evidence/generations, and the
//! repository's authorized [`RemoteRepositorySettings`], and produces exactly
//! one [`RemoteTranscript`] — the fixture-golden structural command string and
//! the audited SSH arguments — or zero effects when the operation or target is
//! unsupported.
//!
//! # POSIX single-quote serializer
//!
//! [`posix_single_quote`] is the **one** audited serializer for every string
//! embedded in the remote command. It encloses each string in single quotes
//! and emits each embedded apostrophe as the POSIX `'"'"'` escape sequence
//! between quoted portions. Empty strings are preserved as `''`. NUL bytes
//! are rejected with a typed [`RemoteSerializeError::NulByte`].
//!
//! No shell template, token splitting, or raw-argument field is accepted from
//! definitions. Only typed emitter values from the definition's declared
//! emitters participate; every value is serialized through the one audited
//! function.
//!
//! # Architectural boundaries
//!
//! The planner resolves operation and target support **before** any effect.
//! An unsupported operation or target returns [`RemotePlanOutcome::Unsupported`]
//! with the exact declared reason; no remote command, SSH argument, or
//! signature is constructed. This is zero SSH / zero preparation.
//!
//! The agent argv/env is produced by reusing the local planner's pure
//! definition-driven emission logic (S7). The remote command string is then
//! assembled as `cd '<cwd>' && exec '<exe>' <quoted argv>` using only the
//! audited serializer. SSH arguments are produced through the existing
//! [`crate::ssh::SshPlan`] audited boundary; this module never builds a new
//! SSH transport or process subsystem. The SSH boundary may execute later;
//! this planner is deterministic and side-effect-free.
//!
//! Product knowledge lives only in the shipped definition data (emitters,
//! capability tokens, operation/target declarations). This module contains no
//! product tokens and performs no product matching.
//!
//! # Slice scope (S9)
//!
//! Only remote plan generation and serialization. It does not implement
//! execution (the SSH boundary executes later), stale recheck, preflight
//! process effects, fresh-send orchestration, persistence, migration, or
//! package-cache generalization.

use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

use crate::domain::RemoteRepositorySettings;
use crate::domain::agent_definition::{AgentLaunchPlan, Operation, RemoteTarget, Target};
use crate::runtime::agent_plan::{
    AgentPlanError, LaunchFieldValues, PlanOutcome, PlanRequest, plan_launch,
};
use crate::ssh::{SshError, SshMode, SshPlan};

// ---------------------------------------------------------------------------
// POSIX single-quote serializer
// ---------------------------------------------------------------------------

/// Typed serialization failure for [`posix_single_quote`].
///
/// Every remote command string is built exclusively through the audited
/// serializer; this error is the only failure mode for value serialization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteSerializeError {
    /// The value contains a NUL byte, which cannot be represented in a POSIX
    /// shell string argument.
    NulByte,
    /// The value contains a non-UTF-8 byte sequence.
    NonUtf8,
}

impl std::fmt::Display for RemoteSerializeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NulByte => f.write_str("remote value contains a NUL byte"),
            Self::NonUtf8 => f.write_str("remote value contains non-UTF-8 bytes"),
        }
    }
}

impl std::error::Error for RemoteSerializeError {}

/// Serialize one string into POSIX shell-safe single-quoted form.
///
/// Each string is enclosed in single quotes. Each embedded apostrophe (`'`) is
/// emitted as the POSIX escape sequence `'"'"'` (close-quote, double-quoted
/// apostrophe, open-quote) between quoted portions. Empty strings are
/// preserved as `''` (two single quotes). NUL bytes are rejected.
///
/// This is the **one** audited serializer for every remote command value. It
/// accepts no shell templates, no shell syntax, and no metacharacter
/// interpretation — only literal string bytes.
///
/// # Errors
///
/// Returns [`RemoteSerializeError::NulByte`] if the input contains a `\0`
/// byte.
///
/// # Examples
///
/// ```
/// # use jefe::runtime::agent_remote_plan::posix_single_quote;
/// assert_eq!(posix_single_quote("hello").unwrap(), "'hello'");
/// assert_eq!(posix_single_quote("").unwrap(), "''");
/// assert_eq!(posix_single_quote("it's").unwrap(), "'it'\"'\"'s'");
/// ```
pub fn posix_single_quote(input: &str) -> Result<String, RemoteSerializeError> {
    if input.contains('\0') {
        return Err(RemoteSerializeError::NulByte);
    }
    // Fast path: no apostrophes → wrap in single quotes.
    if !input.contains('\'') {
        return Ok(format!("'{input}'"));
    }
    // General path: split on apostrophes and join with the POSIX escape
    // sequence `'"'"'` between quoted portions.
    let parts: Vec<&str> = input.split('\'').collect();
    let mut output = String::with_capacity(input.len() + 2 + (parts.len() - 1) * 5);
    output.push('\'');
    output.push_str(parts[0]);
    for part in &parts[1..] {
        output.push_str("'\"'\"'");
        output.push_str(part);
    }
    output.push('\'');
    Ok(output)
}

// ---------------------------------------------------------------------------
// Remote transcript
// ---------------------------------------------------------------------------

/// One fixture-golden structural remote transcript.
///
/// Produced by [`plan_remote_launch`] when the operation and target are
/// supported. It carries the fully-quoted remote command string, the agent
/// argv elements (before serialization), the audited SSH arguments through the
/// existing [`SshPlan`] boundary, and the immutable [`AgentLaunchPlan`] that
/// the execution authorization guard (S8) authorizes before the SSH boundary
/// executes.
///
/// This value owns no process, no SSH connection, and no filesystem resource.
/// It is a deterministic structural transcript that the runtime may execute
/// later through the audited SSH boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteTranscript {
    /// The fully POSIX-quoted remote command string.
    remote_command: String,
    /// The agent argv elements (before serialization), for inspection.
    agent_argv: Vec<OsString>,
    /// The audited SSH arguments through the existing boundary.
    ssh_arguments: Vec<OsString>,
    /// The immutable launch plan stamped with signature and generations.
    plan: AgentLaunchPlan,
}

impl RemoteTranscript {
    /// The fully POSIX-quoted remote command string.
    #[must_use]
    pub fn remote_command(&self) -> &str {
        &self.remote_command
    }

    /// The agent argv elements (before serialization).
    #[must_use]
    pub fn agent_argv(&self) -> &[OsString] {
        &self.agent_argv
    }

    /// The audited SSH arguments through the existing boundary.
    #[must_use]
    pub fn ssh_arguments(&self) -> &[OsString] {
        &self.ssh_arguments
    }

    /// The immutable launch plan.
    #[must_use]
    pub fn plan(&self) -> &AgentLaunchPlan {
        &self.plan
    }
}

// ---------------------------------------------------------------------------
// Request / outcome / error
// ---------------------------------------------------------------------------

/// Immutable inputs to the remote launch planner.
///
/// Every field is borrowed; the planner performs no allocation beyond the
/// produced transcript's owned strings. The caller guarantees the
/// `definition` is validated, the `executable` is the resolved candidate
/// path, and the `ssh_settings` are the authorized repository settings.
#[derive(Debug, Clone)]
pub struct RemotePlanRequest<'a> {
    /// Validated agent definition.
    pub definition: &'a crate::domain::agent_definition::AgentDefinition,
    /// Chosen closed operation.
    pub operation: Operation,
    /// Chosen execution target (must be remote for this planner).
    pub target: Target,
    /// Resolved executable path from the candidate resolver.
    pub executable: PathBuf,
    /// Current probe availability evidence.
    pub probe: crate::domain::agent_definition::Availability,
    /// Probe generation stamp.
    pub probe_generation: u64,
    /// Target generation stamp.
    pub target_generation: u64,
    /// Activation generation stamp compared by the execution guard (S8).
    pub activation_generation: u64,
    /// Typed field values for argv/env emission.
    pub values: &'a LaunchFieldValues,
    /// Sandbox preflight contract.
    pub preflight: crate::domain::agent_definition::Preflight,
    /// Authorized SSH settings for the remote repository.
    pub ssh_settings: &'a RemoteRepositorySettings,
}

/// The outcome of remote plan generation.
#[derive(Debug)]
pub enum RemotePlanOutcome {
    /// Exactly one structural transcript was produced.
    Transcript(Box<RemoteTranscript>),
    /// The operation or target is unsupported; zero effects.
    Unsupported {
        /// The exact declared reason shown to the user.
        reason: String,
    },
    /// A typed validation or serialization failure occurred before any effect.
    Error(RemotePlanError),
}

/// Typed remote planner error. Never panics; never performs side effects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemotePlanError {
    /// The operation/target/probe validation failed (same typed errors as the
    /// local planner).
    Plan(AgentPlanError),
    /// A serialized value contained a NUL byte.
    Serialize(RemoteSerializeError),
    /// The SSH settings are invalid for the audited boundary.
    InvalidSshSettings(String),
    /// A non-remote target was passed to the remote planner.
    NotRemoteTarget,
}

impl std::fmt::Display for RemotePlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Plan(error) => write!(f, "remote plan validation failed: {error}"),
            Self::Serialize(error) => write!(f, "remote serialization failed: {error}"),
            Self::InvalidSshSettings(msg) => write!(f, "invalid SSH settings: {msg}"),
            Self::NotRemoteTarget => f.write_str("remote planner received a non-remote target"),
        }
    }
}

impl std::error::Error for RemotePlanError {}

impl From<RemoteSerializeError> for RemotePlanError {
    fn from(error: RemoteSerializeError) -> Self {
        Self::Serialize(error)
    }
}

impl From<AgentPlanError> for RemotePlanError {
    fn from(error: AgentPlanError) -> Self {
        Self::Plan(error)
    }
}

// ---------------------------------------------------------------------------
// Pure planner entry point
// ---------------------------------------------------------------------------

/// Produce one fixture-golden structural remote transcript, or zero effects.
///
/// Resolution order (deterministic, side-effect-free):
///   1. target must be remote;
///   2. operation support — unsupported returns the declared reason;
///   3. target support — unsupported returns the declared reason;
///   4. probe evidence must be `InstalledCompatible`;
///   5. probe generation must match;
///   6. typed field values validated against the definition;
///   7. argv/env emitted element-by-element in declaration order (reuses the
///      local planner's pure emission logic);
///   8. every argv element and cwd/executable serialized through the one
///      audited [`posix_single_quote`];
///   9. remote command assembled as `cd '<cwd>' && exec '<exe>' <argv>`;
///  10. SSH arguments produced through the existing audited [`SshPlan`]
///      boundary;
///  11. immutable plan stamped with signature and generations.
///
/// Steps 1-7 produce zero SSH / zero preparation on failure. Only steps 8-11
/// build the transcript, and they perform no I/O.
///
/// The agent argv/env is produced by delegating to the local planner (S7)
/// with a temporary local target wrapping the remote canonical cwd. This
/// reuses the audited definition-driven emission logic without duplicating it.
/// The remote planner then applies the one POSIX serializer to produce the
/// remote command string and builds SSH arguments through the existing
/// audited boundary.
///
/// # Errors
///
/// Returns [`RemotePlanOutcome::Error`] for any typed validation or
/// serialization failure, or [`RemotePlanOutcome::Unsupported`] when the
/// operation or target is declared unsupported. Both produce zero effects.
#[must_use]
pub fn plan_remote_launch(request: &RemotePlanRequest<'_>) -> RemotePlanOutcome {
    let remote = match &request.target {
        Target::Remote(remote) => remote,
        Target::Local { .. } => return RemotePlanOutcome::Error(RemotePlanError::NotRemoteTarget),
    };
    if let Err(error) = validate_target_settings(remote, request.ssh_settings) {
        return RemotePlanOutcome::Error(error);
    }
    let plan_request = PlanRequest {
        definition: request.definition,
        operation: request.operation,
        target: request.target.clone(),
        executable: request.executable.clone(),
        probe: request.probe.clone(),
        probe_generation: request.probe_generation,
        target_generation: request.target_generation,
        activation_generation: request.activation_generation,
        values: request.values,
        preflight: request.preflight.clone(),
    };
    let plan = match plan_launch(&plan_request) {
        PlanOutcome::Supported(plan) => *plan,
        PlanOutcome::Unsupported { reason } => {
            return RemotePlanOutcome::Unsupported { reason };
        }
        PlanOutcome::Error(error) => return RemotePlanOutcome::Error(RemotePlanError::Plan(error)),
    };
    match build_transcript(plan, request.ssh_settings) {
        Ok(transcript) => RemotePlanOutcome::Transcript(Box::new(transcript)),
        Err(error) => RemotePlanOutcome::Error(error),
    }
}

fn validate_target_settings(
    target: &RemoteTarget,
    settings: &RemoteRepositorySettings,
) -> Result<(), RemotePlanError> {
    let target_identity = (
        target.user.as_str(),
        target.host.as_str(),
        target.port.unwrap_or(22),
        target.run_as_user.as_str(),
    );
    let settings_identity = (
        settings.login_user.as_str(),
        settings.host.as_str(),
        settings.port.unwrap_or(22),
        settings.run_as_user.as_str(),
    );
    if target_identity != settings_identity {
        return Err(RemotePlanError::InvalidSshSettings(
            "remote target identity does not match authorized SSH settings".to_string(),
        ));
    }
    Ok(())
}

fn build_transcript(
    plan: AgentLaunchPlan,
    settings: &RemoteRepositorySettings,
) -> Result<RemoteTranscript, RemotePlanError> {
    let remote_command = serialize_remote_command(&plan)?;
    let ssh_arguments = SshPlan::arguments(settings, &remote_command, SshMode::NonInteractive)
        .map_err(map_ssh_error)?;
    Ok(RemoteTranscript {
        remote_command,
        agent_argv: plan.argv.clone(),
        ssh_arguments,
        plan,
    })
}

fn serialize_remote_command(plan: &AgentLaunchPlan) -> Result<String, RemotePlanError> {
    let quoted_cwd = quote_os(plan.cwd.as_os_str())?;
    let quoted_executable = quote_os(plan.executable.as_os_str())?;
    let mut command = format!("cd {quoted_cwd} && exec");
    if !plan.env.is_empty() {
        command.push_str(" env");
        for (name, value) in &plan.env {
            let name = os_str(name)?;
            let value = os_str(value)?;
            command.push(' ');
            command.push_str(&posix_single_quote(&format!("{name}={value}"))?);
        }
    }
    command.push(' ');
    command.push_str(&quoted_executable);
    for argument in &plan.argv {
        command.push(' ');
        command.push_str(&quote_os(argument)?);
    }
    Ok(command)
}

fn quote_os(value: &OsStr) -> Result<String, RemotePlanError> {
    posix_single_quote(os_str(value)?).map_err(RemotePlanError::from)
}

fn os_str(value: &OsStr) -> Result<&str, RemotePlanError> {
    value
        .to_str()
        .ok_or(RemotePlanError::Serialize(RemoteSerializeError::NonUtf8))
}

fn map_ssh_error(error: SshError) -> RemotePlanError {
    match error {
        SshError::InvalidSettings(message) => RemotePlanError::InvalidSshSettings(message),
        other => RemotePlanError::InvalidSshSettings(other.to_string()),
    }
}

#[cfg(test)]
#[path = "agent_remote_plan_tests.rs"]
mod tests;
