use std::sync::Arc;

mod agent_chooser_entries;
mod filter_controls;
mod issues;
mod issues_dispatch;
mod issues_filter;
mod issues_lifecycle;
mod issues_list_dispatch;
mod issues_mutation;
mod issues_navigation;
mod issues_property_edit;
mod issues_rewrite_dispatch;
mod issues_subfocus_dispatch;
mod list_navigation;
mod modal_handlers;
mod new_agent_submit;
mod normal;
mod persist_focus;
mod preflight;
mod pty_passthrough;
mod relaunch;
mod send_runtime;
mod settled_refresh;

use relaunch::dispatch_relaunch_agent;
use settled_refresh::SettledRefresh;

// Re-export so sibling modules importing `super::preflight_or_prompt` keep
// resolving after the helper moved into the `preflight` submodule.
pub use preflight::preflight_or_prompt;

// PR-mode key-routing + dispatch surface.
// @plan PLAN-20260624-PR-MODE.P11
// @requirement REQ-PR-001
// @requirement REQ-PR-002
mod prs;
mod prs_comments_dispatch;
mod prs_dispatch;
mod prs_filter;
mod prs_list_dispatch;
mod prs_merge_dispatch;
mod prs_mutation;
mod prs_property_edit;
// @plan PLAN-20260624-PR-MODE.P11
mod prs_orchestration;

mod actions;
mod actions_orchestration;
// Errors-mode key dispatch (issue #292).
mod errors;
// In-app device-code auth remediation dispatch (issue #244).
mod auth_remediation;
mod gh_async;
mod list_loader;

mod agent_runtime;
mod availability;
mod clone_identity;
mod fresh_prompt;
mod issue_git_prep;
mod issue_prep;
mod issue_self_assignment;
mod issues_send;
mod remote_probe;
mod target_resolution;
mod tracker_resolver;
pub mod transient_cleanup;
mod transient_issue_send;
mod transient_pr_send;
mod transient_queue_ops;
use agent_runtime::{
    clear_agent_runtime_attachment, clear_runtime_warning, mark_agent_runtime_attached,
    mark_runtime_session_dead_if_present, process_on_success, set_agent_runtime_binding,
    worker_process_for,
};

pub use modal_handlers::{
    handle_f12_toggle, handle_mode_auth_key, handle_mode_confirm_key, handle_mode_form_key,
    handle_mode_theme_picker_key,
};

pub use normal::{handle_global_shortcut_key, handle_normal_key_event};

// Re-export the background-refresh orchestration helper so `app_shell` can
// import it from `app_input` (issue #128).
pub use actions_orchestration::synchronize_actions_geometry;
pub use prs_orchestration::request_pr_background_refresh;

// Re-export the chooser metadata builder so key handlers in `issues`/`prs`
// can resolve git display metadata at the app_input boundary (issue #230).
pub use agent_chooser_entries::build_chooser_metadata;

// Re-export the PTY-forwarding helpers so `app_shell` can drive the agent
// terminal without owning the encoding/forwarding logic (issue #200, #286).
pub use pty_passthrough::{
    forward_key_to_pty, try_ctrl_c_interrupt_passthrough, try_suppress_synthetic_enter,
    update_paste_enter_suppression,
};

use iocraft::hooks::State as HookState;
use iocraft::prelude::*;
use tracing::{debug, warn};

use std::time::Duration;

use jefe::domain::{AgentId, LaunchSignature, Repository};

const MAC_ALT_DIGIT_SHORTCUTS: &[(char, u8)] = &[
    ('¡', 1),
    ('™', 2),
    ('£', 3),
    ('¢', 4),
    ('∞', 5),
    ('§', 6),
    ('¶', 7),
    ('•', 8),
    ('ª', 9),
];
use jefe::input::{SearchKeyRoute, route_search_key};
use jefe::messages::{AppMessage, IssuesMessage, RuntimeMessage, UiNavigationMessage};
use jefe::persistence::State as PersistedState;
const REMOTE_ATTACH_SETTLE_DELAY: Duration = Duration::from_millis(150);

use jefe::runtime::{RuntimeError, RuntimeManager, sandbox_ssh_agent_warning};

#[must_use]
fn jump_to_shortcut_agent(app_state: &mut AppStateHandle, ctx: &SharedContext, slot: u8) -> bool {
    let mut state = app_state.write();
    *state = std::mem::take(&mut *state).apply(AppEvent::JumpToAgentByShortcut(slot));

    let selected_running_agent_id = state
        .selected_agent()
        .filter(|agent| agent.is_running())
        .map(|agent| agent.id.clone());

    if let Some(agent_id) = selected_running_agent_id {
        state.pane_focus = PaneFocus::Terminal;
        if !state.terminal_focused {
            *state = std::mem::take(&mut *state).apply(AppEvent::ToggleTerminalFocus);
        }
        drop(state);

        let attached_ok = if let Some(ctx_arc) = ctx
            && let Ok(mut ctx_guard) = ctx_arc.lock()
        {
            ctx_guard.runtime.attach(&agent_id).is_ok()
        } else {
            false
        };

        let mut state = app_state.write();
        if !attached_ok {
            state.terminal_focused = false;
            state.pane_focus = PaneFocus::Agents;
            mark_agent_runtime_attached(&mut state, &agent_id, false);
            let persisted = to_persisted_state(&state);
            drop(state);
            persist_state(ctx, &persisted);
            return false;
        }

        clear_agent_runtime_attachment(&mut state);
        mark_agent_runtime_attached(&mut state, &agent_id, true);
        let persisted = to_persisted_state(&state);
        drop(state);
        persist_state(ctx, &persisted);
        true
    } else {
        state.terminal_focused = false;
        state.pane_focus = PaneFocus::Agents;
        let persisted = to_persisted_state(&state);
        drop(state);
        persist_state(ctx, &persisted);
        false
    }
}

use jefe::state::{AppEvent, AppState, PaneFocus, RepositoryFormFocus};

fn repository_focus_toggles_checkbox(focus: RepositoryFormFocus) -> bool {
    matches!(
        focus,
        RepositoryFormFocus::DefaultAgentKind
            | RepositoryFormFocus::RemoteEnabled
            | RepositoryFormFocus::SetupEnvDefault
    )
}

pub type SharedContext = Option<Arc<std::sync::Mutex<super::AppContext>>>;
pub type AppStateHandle = HookState<AppState>;
pub type QuitHandle = HookState<bool>;
pub type HelpScrollHandle = HookState<u32>;

fn github_client(ctx: &SharedContext) -> Option<jefe::github::GhClient> {
    let ctx_arc = ctx.as_ref()?;
    let ctx_guard = ctx_arc.lock().ok()?;
    Some(ctx_guard.gh_client)
}
pub fn to_persisted_state(state: &AppState) -> PersistedState {
    // Transient agents are runtime-only — never persisted to state.json
    // (issue #213). Filter them out and project selection metadata onto the
    // remaining persistent-agent collection so no transient index/ID lingers.
    let persistent_agents: Vec<_> = state
        .agents
        .iter()
        .filter(|a| !a.is_transient())
        .cloned()
        .collect();

    // Recompute selected_agent_index by tracking the originally-selected
    // agent's ID through the filter: if it was transient, clear; otherwise
    // find its new index in the persistent list (issue #213 OCR fix).
    let selected_agent_index = state.selected_agent_index.and_then(|idx| {
        let original = state.agents.get(idx)?;
        if original.is_transient() {
            return None;
        }
        persistent_agents.iter().position(|a| a.id == original.id)
    });

    // Filter last_selected_agent_by_repo: remove entries whose agent ID
    // belonged to a transient agent (no longer in persistent_agents).
    let persistent_agent_ids: std::collections::HashSet<_> =
        persistent_agents.iter().map(|a| a.id.clone()).collect();
    let last_selected_agent_by_repo: Vec<_> = state
        .last_selected_agent_by_repo
        .iter()
        .filter(|(_, agent_id)| persistent_agent_ids.contains(agent_id))
        .cloned()
        .collect();

    PersistedState {
        schema_version: jefe::persistence::STATE_SCHEMA_VERSION,
        repositories: state.repositories.clone(),
        agents: persistent_agents,
        selected_repository_index: state.selected_repository_index,
        selected_agent_index,
        hide_idle_repositories: state.hide_idle_repositories,
        last_selected_agent_by_repo,
        pane_focus: pane_focus_to_persisted(state.pane_focus),
        terminal_focused: state.terminal_focused,
        user_preferences: state.user_preferences.clone(),
    }
}

pub use persist_focus::{pane_focus_from_persisted, pane_focus_to_persisted, persist_state};

fn launch_signature_for_agent(
    agent: &jefe::domain::Agent,
    repository: &Repository,
) -> LaunchSignature {
    LaunchSignature {
        work_dir: agent.work_dir.clone(),
        profile: agent.profile.clone(),
        code_puppy_model: if agent.code_puppy_model.trim().is_empty() {
            repository.default_code_puppy_model.trim().to_owned()
        } else {
            agent.code_puppy_model.trim().to_owned()
        },
        code_puppy_version: agent.code_puppy_version.trim().to_owned(),
        code_puppy_yolo: agent.code_puppy_yolo,
        code_puppy_quick_resume: agent.code_puppy_quick_resume,
        mode_flags: agent.mode_flags.clone(),
        llxprt_debug: agent.llxprt_debug.clone(),
        pass_continue: agent.pass_continue,
        sandbox_enabled: agent.sandbox_enabled,
        sandbox_engine: agent.sandbox_engine,
        sandbox_flags: agent.sandbox_flags.clone(),
        remote: repository.remote.clone(),
        agent_kind: agent.agent_kind,
        llxprt_version: agent.llxprt_version.clone(),
    }
}

pub fn launch_signature_for_transient(
    repository: &Repository,
    work_dir: &std::path::Path,
) -> LaunchSignature {
    LaunchSignature {
        work_dir: work_dir.to_path_buf(),
        profile: repository.default_profile.clone(),
        code_puppy_model: repository.default_code_puppy_model.trim().to_owned(),
        code_puppy_version: if repository.default_agent_kind == jefe::domain::AgentKind::CodePuppy {
            repository.default_code_puppy_version.trim().to_owned()
        } else {
            String::new()
        },
        code_puppy_yolo: repository.default_code_puppy_yolo,
        code_puppy_quick_resume: false,
        mode_flags: repository.default_llxprt_mode_flags.clone(),
        llxprt_debug: String::new(),
        pass_continue: false,
        sandbox_enabled: false,
        sandbox_engine: jefe::domain::SandboxEngine::default(),
        sandbox_flags: String::new(),
        remote: repository.remote.clone(),
        agent_kind: repository.default_agent_kind,
        llxprt_version: repository.default_llxprt_version.clone(),
    }
}

fn agent_and_signature(
    state: &AppState,
    agent_id: &AgentId,
) -> Option<(jefe::domain::Agent, LaunchSignature)> {
    let agent = state
        .agents
        .iter()
        .find(|agent| &agent.id == agent_id)?
        .clone();
    let repository = state.repository_by_id(&agent.repository_id)?;
    let signature = launch_signature_for_agent(&agent, repository);
    Some((agent, signature))
}

fn apply_and_persist(app_state: &mut AppStateHandle, ctx: &SharedContext, evt: AppEvent) {
    let settled_refresh = SettledRefresh::from_event(&evt);
    let mut state = app_state.write();
    *state = std::mem::take(&mut *state).apply(evt);
    let persisted = to_persisted_state(&state);
    drop(state);
    persist_state(ctx, &persisted);
    match settled_refresh {
        Some(SettledRefresh::Issues) => {
            issues_dispatch::resume_issue_post_mutation_refresh(app_state, ctx);
        }
        Some(SettledRefresh::PullRequests) => {
            prs_orchestration::resume_pr_post_mutation_refresh(app_state, ctx);
        }
        None => {}
    }
}

fn close_modal_and_persist(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    apply_and_persist(app_state, ctx, AppEvent::CloseModal);
}

/// Spawn + attach an agent session (shared by fresh-launch and post-preflight
/// resume paths). Returns `Ok` only on a successful launch so callers can gate
/// side effects (e.g. issue self-assignment) on the actual outcome.
fn execute_agent_launch(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: &AgentId,
    work_dir: &std::path::Path,
    signature: &LaunchSignature,
    is_relaunch: bool,
) -> Result<(), RuntimeError> {
    match spawn_and_attach(ctx, agent_id, work_dir, signature, is_relaunch) {
        Ok(()) => {
            mark_launch_attached(app_state, ctx, agent_id, signature);
            Ok(())
        }
        Err(error) => {
            warn!(error = %error, "could not spawn or attach session for agent");
            mark_launch_failed(app_state, ctx, agent_id, error.clone());
            Err(error)
        }
    }
}

fn spawn_and_attach(
    ctx: &SharedContext,
    agent_id: &AgentId,
    work_dir: &std::path::Path,
    signature: &LaunchSignature,
    is_relaunch: bool,
) -> Result<(), RuntimeError> {
    let Some(ctx_arc) = ctx else {
        return Err(RuntimeError::SpawnFailed(
            "runtime context unavailable".to_owned(),
        ));
    };
    let Ok(mut ctx_guard) = ctx_arc.lock() else {
        return Err(RuntimeError::SpawnFailed(
            "runtime context lock unavailable".to_owned(),
        ));
    };

    let spawn_result = if is_relaunch {
        ctx_guard
            .runtime
            .spawn_session_fresh(agent_id, work_dir, signature)
    } else {
        ctx_guard
            .runtime
            .spawn_session(agent_id, work_dir, signature)
    };
    spawn_result.and_then(|()| {
        std::thread::sleep(REMOTE_ATTACH_SETTLE_DELAY);
        ctx_guard.runtime.attach(agent_id)
    })
}

fn mark_launch_failed(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: &AgentId,
    error: RuntimeError,
) {
    if let Some(ctx_arc) = ctx
        && let Ok(mut ctx_guard) = ctx_arc.lock()
    {
        let _ = ctx_guard.runtime.mark_session_dead(agent_id);
    }

    let mut state = app_state.write();
    state.terminal_focused = false;
    state.pane_focus = PaneFocus::Agents;
    state.error_message = Some(error.to_string());
    if let Some(agent) = state.agents.iter_mut().find(|agent| agent.id == *agent_id) {
        agent.runtime_binding = None;
    }
    mark_runtime_session_dead_if_present(&mut state, agent_id);
    let persisted = to_persisted_state(&state);
    drop(state);
    persist_state(ctx, &persisted);
}

fn mark_launch_attached(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: &AgentId,
    signature: &LaunchSignature,
) {
    // Query the runtime for the worker PID before taking the app-state write
    // lock, so the persisted binding carries the PID-liveness fallback.
    let (pid, process_identity) = worker_process_for(ctx, agent_id);

    let mut state = app_state.write();
    set_agent_runtime_binding(
        &mut state,
        agent_id,
        jefe::runtime::RuntimeSession::session_name_for(agent_id),
        signature.clone(),
        pid,
        process_identity,
    );
    clear_agent_runtime_attachment(&mut state);
    mark_agent_runtime_attached(&mut state, agent_id, true);
    // SSH agent warnings are only relevant for LLxprt sandbox sessions.
    // CodePuppy does not use the LLxprt sandbox, so stale sandbox_enabled
    // must not trigger the warning.
    if signature.agent_kind == jefe::domain::AgentKind::Llxprt {
        if let Some(warning) = sandbox_ssh_agent_warning() {
            state.warning_message = Some(warning);
        } else {
            clear_runtime_warning(&mut state);
        }
    }
    let persisted = to_persisted_state(&state);
    drop(state);
    persist_state(ctx, &persisted);
}

pub fn handle_mode_help_key(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    help_scroll: &mut HelpScrollHandle,
    key_event: &KeyEvent,
) {
    match key_event.code {
        KeyCode::Esc | KeyCode::Char('?') => {
            close_modal_and_persist(app_state, ctx);
        }
        KeyCode::Up => {
            let offset = help_scroll.get();
            if offset > 0 {
                help_scroll.set(offset - 1);
            }
        }
        KeyCode::Down => {
            // `saturating_add` is required: `End` sets the sentinel `u32::MAX`
            // (clamped by the renderer); plain `+ 1` would overflow-panic then.
            help_scroll.set(help_scroll.get().saturating_add(1));
        }
        KeyCode::PageUp => {
            let offset = help_scroll.get();
            help_scroll.set(offset.saturating_sub(8));
        }
        KeyCode::PageDown => {
            help_scroll.set(help_scroll.get().saturating_add(8));
        }
        KeyCode::Home => {
            help_scroll.set(0);
        }
        KeyCode::End => {
            help_scroll.set(u32::MAX);
        }
        _ => {}
    }
    // Mirror the help scroll offset to AppState so the selection content
    // projection can map screen coordinates to the correct help content
    // line (issue #178). The hook state may hold a sentinel (u32::MAX for
    // the End key) that the renderer clamps via ScrollableText; clamp here
    // using the same viewport math so the selection layer reads the actual
    // visible offset, not the raw sentinel.
    let (_, term_rows) = crossterm::terminal::size().unwrap_or((120, 40));
    let viewport_rows = jefe::ui::modals::help_viewport_rows(term_rows);
    let max_scroll = jefe::ui::modals::help_content_lines()
        .len()
        .saturating_sub(viewport_rows);
    let clamped = help_scroll
        .get()
        .min(u32::try_from(max_scroll).unwrap_or(u32::MAX));
    app_state.write().help_scroll_offset = usize::try_from(clamped).unwrap_or(0);
}

pub fn handle_mode_search_key(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    key_event: &KeyEvent,
) -> bool {
    match route_search_key(key_event) {
        SearchKeyRoute::CloseAndConsume => {
            close_modal_and_persist(app_state, ctx);
            true
        }
        SearchKeyRoute::Backspace => {
            apply_and_persist(app_state, ctx, AppEvent::FormBackspace);
            true
        }
        SearchKeyRoute::EditQueryChar(c) => {
            apply_and_persist(app_state, ctx, AppEvent::FormChar(c));
            true
        }
        SearchKeyRoute::CloseAndReroute => {
            debug!(
                code = ?key_event.code,
                modifiers = ?key_event.modifiers,
                "closing search mode on non-search key"
            );
            close_modal_and_persist(app_state, ctx);
            false
        }
        SearchKeyRoute::Ignore => true,
    }
}

pub fn dispatch_app_event(app_state: &mut AppStateHandle, ctx: &SharedContext, evt: AppEvent) {
    dispatch_app_message(app_state, ctx, evt.into());
}

/// Dispatch a terminal scrollback event (issue #198).
///
/// Refreshes cached scroll geometry BEFORE applying the event so the reducer's
/// clamp bounds match rendered content. Uses apply-only (no persist) since
/// scrollback fields are runtime-only.
pub fn dispatch_terminal_scroll(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    evt: AppEvent,
) {
    refresh_terminal_scroll_geometry(app_state, ctx);
    let mut state = app_state.write();
    *state = std::mem::take(&mut *state).apply(evt);
}

/// Try to intercept a scrollback-control key while the terminal is focused
/// (issue #198). Returns `true` when the key was consumed as a terminal
/// scrollback viewport event (and must NOT be forwarded to the PTY).
///
/// PageUp/PageDown/Home intercept from both states; End/Up/Down only intercept
/// when scrolled back. Modifier chords are forwarded. The decision is made by
/// the pure [`jefe::input::should_intercept_for_scrollback`] helper so it stays
/// unit-testable.
pub fn try_intercept_terminal_scrollback(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    key_event: &KeyEvent,
) -> bool {
    let (offset_is_some, kennel_mode) = {
        let state = app_state.read();
        (
            state.terminal_history_offset.is_some(),
            state.is_kennel_mode(),
        )
    };
    let Some(scroll_evt) =
        jefe::input::should_intercept_for_scrollback(key_event, offset_is_some, kennel_mode)
    else {
        return false;
    };
    dispatch_terminal_scroll(app_state, ctx, scroll_evt);
    true
}

/// Refresh cached terminal scrollback geometry (issue #198). Computes
/// viewport rows from PTY layout and total lines from history + snapshot.
/// When ctx is None or the lock is contended, preserves existing geometry
/// instead of zeroing it (zeroing would clear the scroll offset).
///
/// Issue #301 Phase 2: reads from the `HistoryCache` via the public accessor
/// instead of calling `capture_history()` (which shells out to tmux
/// synchronously). The background capture worker fills the cache.
pub fn refresh_terminal_scroll_geometry(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let (term_cols, term_rows) = crossterm::terminal::size().unwrap_or((120, 40));
    let pty_layout = jefe::layout::compute_pty_layout(term_cols, term_rows);

    // Read retained history + live snapshot rows from the cache (issue #301
    // Phase 2: no synchronous tmux subprocess). try_lock keeps this
    // non-blocking when a background attach holds the mutex.
    let (history_count, live_rows) = match ctx.as_ref() {
        Some(ctx_arc) => match ctx_arc.try_lock() {
            Ok(guard) => {
                let history_count = match guard.runtime.attached_agent() {
                    Some(agent_id) => {
                        let generation = guard.runtime.output_generation();
                        // Use exact-generation cache; on miss, fall back to
                        // the any-generation cache so a cache miss (background
                        // capture still in flight) does not reset the
                        // scrollback count to zero mid-output. Use fallback
                        // only when the exact-generation lookup returns None
                        // (not .max()) to avoid overcounting when both caches
                        // contain data (issue #301 review).
                        guard
                            .runtime
                            .history_cache_get(agent_id, generation)
                            .map_or_else(
                                || {
                                    guard
                                        .runtime
                                        .history_cache_fallback(agent_id)
                                        .map_or(0, Vec::len)
                                },
                                Vec::len,
                            )
                    }
                    None => 0,
                };
                let live_rows = guard.runtime.snapshot().map_or(0, |s| s.rows);
                (history_count, live_rows)
            }
            Err(_) => {
                // Lock contention: preserve existing geometry instead of
                // zeroing it. Zeroing would clear the scroll offset and jump
                // to follow-tail during attach.
                return;
            }
        },
        None => {
            // No context: preserve existing geometry instead of zeroing it.
            // Zeroing would clear the scroll offset.
            return;
        }
    };

    let mut state = app_state.write();
    let old_total = state.terminal_total_lines;
    let viewport_rows = usize::from(pty_layout.pty_rows);

    let (new_offset, new_total) = jefe::state::scrollback_ops::compute_terminal_scroll_geometry(
        state.terminal_history_offset,
        old_total,
        history_count,
        live_rows,
        viewport_rows,
    );
    state.terminal_history_offset = new_offset;
    state.terminal_viewport_rows = viewport_rows;
    state.terminal_total_lines = new_total;
}

pub fn dispatch_app_message(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    message: AppMessage,
) {
    log_dispatch(&message);

    match message {
        AppMessage::UiNavigation(UiNavigationMessage::ToggleTerminalFocus) => {
            apply_and_persist(app_state, ctx, AppEvent::ToggleTerminalFocus);
        }
        AppMessage::Runtime(RuntimeMessage::KillAgent(agent_id)) => {
            dispatch_kill_agent(app_state, ctx, agent_id);
        }
        AppMessage::Runtime(RuntimeMessage::RelaunchAgent(agent_id)) => {
            dispatch_relaunch_agent(app_state, ctx, agent_id);
        }
        AppMessage::Runtime(RuntimeMessage::RestartAgent(agent_id)) => {
            dispatch_restart_agent(app_state, ctx, agent_id);
        }
        AppMessage::Runtime(RuntimeMessage::AgentStatusChanged(agent_id, status)) => {
            apply_and_persist(
                app_state,
                ctx,
                AppEvent::AgentStatusChanged(agent_id, status),
            );
            transient_queue_ops::drain_transient_queue(app_state, ctx);
        }
        AppMessage::Issues(message) => {
            issues_dispatch::dispatch_issues_message(app_state, ctx, message);
        }
        // ── PR-mode dispatch arms ───────────────────────────────────────────
        // @plan PLAN-20260624-PR-MODE.P11
        // @requirement REQ-PR-001
        // @requirement REQ-PR-003
        // @requirement REQ-PR-010
        // @requirement REQ-PR-011
        // @requirement REQ-PR-012
        // @pseudocode component-004 lines 97-118
        AppMessage::PullRequests(message) => {
            prs_orchestration::dispatch_prs_message(app_state, ctx, message);
        }
        AppMessage::Actions(message) => {
            actions_orchestration::dispatch_actions_message(app_state, ctx, message);
        }
        message => apply_and_persist(app_state, ctx, AppEvent::from(message)),
    }
}

/// Dispatch issues close/delete lifecycle messages (issue #182).
///
/// Applies the reducer event first, then — for the action events that start an
/// off-thread gh mutation — hands off to the lifecycle dispatch helper.
fn dispatch_issues_lifecycle(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    message: IssuesMessage,
) {
    match message {
        IssuesMessage::CloseIssue => {
            apply_and_persist(app_state, ctx, AppEvent::CloseIssue);
            issues_lifecycle::handle_issue_close(app_state, ctx);
        }
        IssuesMessage::CloseReasonConfirm => {
            apply_and_persist(app_state, ctx, AppEvent::CloseReasonConfirm);
            issues_lifecycle::handle_issue_close_with_reason(app_state, ctx);
        }
        message @ (IssuesMessage::OpenCloseReasonChooser
        | IssuesMessage::CloseReasonNavigateUp
        | IssuesMessage::CloseReasonNavigateDown
        | IssuesMessage::CloseReasonSelect
        | IssuesMessage::CloseReasonDuplicateSearchChar(_)
        | IssuesMessage::CloseReasonDuplicateSearchBackspace
        | IssuesMessage::CloseReasonDuplicateSearchNavigateUp
        | IssuesMessage::CloseReasonDuplicateSearchNavigateDown
        | IssuesMessage::CloseReasonCancel) => {
            apply_and_persist(app_state, ctx, AppEvent::from(message));
        }
        IssuesMessage::OpenDeleteIssueConfirm => {
            apply_and_persist(app_state, ctx, AppEvent::OpenDeleteIssueConfirm);
        }
        IssuesMessage::IssueDeleteConfirm => {
            apply_and_persist(app_state, ctx, AppEvent::IssueDeleteConfirm);
            issues_lifecycle::handle_issue_delete_confirm(app_state, ctx);
        }
        IssuesMessage::IssueDeleteCancel => {
            apply_and_persist(app_state, ctx, AppEvent::IssueDeleteCancel);
        }
        // Defensive fallback: the sole caller (dispatch_app_message) pre-filters
        // to the lifecycle variants above, so other IssuesMessage variants
        // never reach here. Kept as a no-op safety net rather than forcing this
        // match to enumerate every IssuesMessage variant.
        _ => apply_and_persist(app_state, ctx, AppEvent::from(message)),
    }
}

fn update_detail_viewport_rows(app_state: &mut AppStateHandle) {
    let (term_cols, term_rows) = crossterm::terminal::size().unwrap_or((120, 40));
    let (_, render_rows) = jefe::layout::effective_render_size(term_cols, term_rows);
    let mut state = app_state.write();
    // Issue #265: use the shared banner projection so a notice-only banner
    // reserves the same viewport row as an error banner.
    let issues_banner_visible = jefe::layout::issues_banner_visible(
        state.issues_state.error.as_deref(),
        state.issues_state.draft_notice.as_deref(),
    );
    state.issues_state.detail_viewport_rows = jefe::layout::issues_detail_viewport_rows(
        usize::from(render_rows),
        issues_banner_visible,
        state.issues_state.filter_ui.controls_open,
    );
}

fn log_dispatch(message: &AppMessage) {
    let route = message.route();
    debug!(
        message_domain = ?route.domain,
        message = route.name,
        "dispatching app message"
    );
}

mod agent_lifecycle_ops;
use agent_lifecycle_ops::{dispatch_kill_agent, dispatch_restart_agent};

#[cfg(test)]
#[path = "app_input_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "issue_send_modal_tests.rs"]
mod issue_send_modal_tests;
#[cfg(test)]
#[path = "modal_handlers_tests.rs"]
mod modal_handlers_tests;
#[cfg(test)]
#[path = "new_agent_submit_tests.rs"]
mod new_agent_submit_tests;
#[cfg(test)]
#[path = "preflight_gating_tests.rs"]
mod preflight_gating_tests;
#[cfg(test)]
#[path = "relaunch_tests.rs"]
mod relaunch_tests;

// @plan PLAN-20260624-PR-MODE.P15
// @requirement REQ-PR-001
#[cfg(test)]
#[path = "prs_integration_test_fixtures.rs"]
mod prs_integration_test_fixtures;
#[cfg(test)]
#[path = "prs_integration_tests.rs"]
mod prs_integration_tests;

// @plan PLAN-20260624-PR-MODE.P15
// @requirement REQ-PR-004
// @requirement REQ-PR-NFR-003
#[cfg(test)]
#[path = "prs_integration_tests_lifecycle.rs"]
mod prs_integration_tests_lifecycle;
// Extracted from `prs_dispatch.rs` to keep that handler module under the
// per-file line limit.
#[cfg(test)]
#[path = "prs_dispatch_tests.rs"]
mod prs_dispatch_tests;

// Issue #266: configurable Issues / PRs Repo override (tracker wiring,
// payload identity, self-assignment decoupling, Actions regression).
#[cfg(test)]
#[path = "issue266_tracker_tests.rs"]
mod issue266_tracker_tests;

// Transient agent persistence tests (issue #213).
#[cfg(test)]
#[path = "transient_persistence_tests.rs"]
mod transient_persistence_tests;

#[cfg(test)]
#[path = "transient_launch_options_tests.rs"]
mod transient_launch_options_tests;
