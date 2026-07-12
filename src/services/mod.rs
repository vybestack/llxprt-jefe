//! Application service layer — the app/domain boundary.
//!
//! This module owns app-side use-cases that coordinate domain entities with
//! application policy. The first such use-case is canonical agent creation,
//! which centralizes validation, normalization, and the initial-status policy
//! that was previously spread across the UI/state layer and domain defaults.

mod normalize;

use std::path::PathBuf;

use crate::domain::{
    Agent, AgentId, AgentKind, AgentStatus, PlatformCapabilities, Repository, SandboxEngine,
};

pub(crate) use normalize::{
    expand_tilde, normalize_llxprt_debug, normalize_profile, normalize_sandbox_flags,
};

/// Generate a stable, time-based identifier with the given prefix.
pub(crate) fn generate_id(prefix: &str) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!("{prefix}-{timestamp:x}")
}

/// Inputs for the canonical agent-creation use-case.
///
/// This captures everything the service needs to build an [`Agent`] from
/// user-facing form input, decoupling the form/state layer from the domain
/// construction and lifecycle policy.
pub struct CreateAgentParams<'a> {
    /// Repository the agent belongs to. Used for work_dir handling (tilde
    /// expansion for local repos, verbatim for remote) and the agent's
    /// repository binding.
    pub repository: &'a Repository,
    /// Raw agent name (validated/trimmed by the service).
    pub name: &'a str,
    /// Free-form description.
    pub description: &'a str,
    /// Raw work directory (validated/normalized by the service).
    pub work_dir: &'a str,
    /// Raw profile value (normalized by the service).
    pub profile: &'a str,
    /// Optional Code Puppy model override.
    pub code_puppy_model: &'a str,
    /// Explicit Code Puppy YOLO choice.
    pub code_puppy_yolo: bool,
    /// Agent runtime selected in the form.
    pub agent_kind: &'a str,
    /// Raw mode string, whitespace-split into flags by the service.
    pub mode: &'a str,
    /// Raw llxprt debug value (trimmed by the service).
    pub llxprt_debug: &'a str,
    /// Whether `--continue` should be passed on subsequent launches.
    pub pass_continue: bool,
    /// Whether sandboxing is enabled.
    pub sandbox_enabled: bool,
    /// Raw sandbox engine value (parsed/normalized via platform capabilities).
    pub sandbox_engine: &'a str,
    /// Raw sandbox flags (normalized by the service).
    pub sandbox_flags: &'a str,
    /// Optional keyboard shortcut slot.
    pub shortcut_slot: Option<u8>,
    /// 1-based index used to build the agent's display id.
    pub next_display_index: usize,
}

/// Resolve the agent work directory from raw form input.
///
/// Returns `None` for blank input. Local repositories get tilde expansion;
/// remote repositories keep the path verbatim (it refers to a remote host).
pub(crate) fn resolve_agent_work_dir(repository: &Repository, value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    if repository.remote.enabled {
        Some(trimmed.to_owned())
    } else {
        Some(expand_tilde(trimmed))
    }
}

/// Canonical app-side agent creation path.
///
/// This is the single source of truth for constructing an [`Agent`] from
/// user-facing input. It validates required fields, applies all normalization
/// (profile, mode, sandbox engine/flags, work_dir), and sets the initial
/// status policy.
///
/// Returns agents with `status: Running` because app-side creation immediately
/// triggers launch — there is no observable `Queued` intermediate state for a
/// user-initiated create. Use [`Agent::new`] for simple domain/test
/// construction, which defaults to `Queued`.
///
/// Returns `None` when the name or work directory is empty/whitespace-only.
///
/// This function is pure: it performs no filesystem side effects. Callers that
/// need to materialize a local work directory (e.g. the state layer) should do
/// so separately.
#[must_use]
pub fn create_agent(params: CreateAgentParams<'_>) -> Option<Agent> {
    let trimmed_name = params.name.trim();
    if trimmed_name.is_empty() {
        return None;
    }

    let work_dir = resolve_agent_work_dir(params.repository, params.work_dir)?;

    // The mode field is the single source of truth for whether --yolo (or any
    // flag) is passed. An empty mode yields no flags so an agent can run
    // non-yolo; the new-agent form pre-fills --yolo as the default instead.
    let mode_flags: Vec<String> = params.mode.split_whitespace().map(String::from).collect();

    let caps = PlatformCapabilities::current();
    let sandbox_engine = SandboxEngine::from_form_value(params.sandbox_engine)
        .and_then(|engine| caps.normalize_engine(engine))
        .unwrap_or_default();

    Some(Agent {
        id: AgentId(generate_id("agent")),
        display_id: format!("#{}", params.next_display_index),
        repository_id: params.repository.id.clone(),
        shortcut_slot: params.shortcut_slot,
        name: trimmed_name.to_owned(),
        description: params.description.to_owned(),
        work_dir: PathBuf::from(&work_dir),
        profile: normalize_profile(params.profile),
        code_puppy_model: params.code_puppy_model.trim().to_owned(),
        code_puppy_yolo: Some(params.code_puppy_yolo),
        mode_flags,
        llxprt_debug: normalize_llxprt_debug(params.llxprt_debug),
        pass_continue: params.pass_continue,
        sandbox_enabled: params.sandbox_enabled,
        sandbox_engine,
        sandbox_flags: normalize_sandbox_flags(params.sandbox_flags),
        agent_kind: AgentKind::from_form_value(params.agent_kind)
            .unwrap_or(params.repository.default_agent_kind),
        // App-created agents start Running because creation triggers immediate launch.
        status: AgentStatus::Running,
        runtime_binding: None,
    })
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
