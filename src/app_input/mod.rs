use std::sync::Arc;

mod action_availability;
mod action_handlers;
pub use action_availability::refresh_action_availability;
#[cfg(test)]
#[path = "action_handlers_tests.rs"]
mod action_handlers_tests;
mod agent_chooser_entries;
mod dashboard_search;
mod filter_controls;
mod issues;
pub use settings::{handle_capture_key, handle_dirty_guard_key, handle_layout_key};
mod issues_comments_dispatch;
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

mod new_issue_submit;
mod normal;
mod persist_focus;
mod preflight;
mod pty_passthrough;
mod relaunch;
mod send_runtime;
mod settings;
mod settled_refresh;
/// Shell-overlay key dispatch (issue #222).
pub mod shell_overlay;

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
mod prs_diff_dispatch;
mod prs_dispatch;
mod prs_filter;
mod prs_lifecycle;
mod prs_list_dispatch;
mod prs_merge_dispatch;
mod prs_mutation;
mod prs_property_edit;
// @plan PLAN-20260624-PR-MODE.P11
mod prs_orchestration;
mod raw_key_mutations;

mod actions;
mod actions_orchestration;
// Terminal-manager key dispatch and runtime orchestration (issue #364 PR A).
pub mod terminal_manager;
// In-app device-code auth remediation dispatch (issue #244).
mod auth_remediation;
mod gh_async;
mod list_loader;
mod worker_panic;

mod agent_runtime;
mod availability;
pub use availability::observe_startup_agent_availability;
mod clone_identity;
mod fresh_prompt;
mod issue_git_prep;
mod issue_prep;
mod issue_self_assignment;
mod issues_send;
#[cfg(test)]
mod remote_probe;
mod target_resolution;
mod tracker_resolver;
pub mod transient_cleanup;
mod transient_issue_send;
mod transient_pr_send;
mod transient_queue_ops;
use agent_runtime::{
    bound_identities_for, clear_agent_runtime_attachment, mark_agent_runtime_attached,
    mark_runtime_session_dead_if_present, process_on_success, set_agent_runtime_binding,
};

pub use modal_handlers::handle_f12_toggle;

pub use list_navigation::dashboard_page_item_count;
pub use normal::observe_rapid_quit;
pub use raw_key_mutations::resolve as resolve_raw_key_mutation;

pub use action_handlers::{
    apply_execution as apply_action_execution, execution_for as action_execution_for,
    pre_mode_owned,
};

#[cfg(test)]
pub fn resolve_test_registry_event(
    state: &AppState,
    key_event: &KeyEvent,
    terminal_cols: u16,
    terminal_rows: u16,
) -> Option<AppEvent> {
    if let Some(event) = raw_key_mutations::resolve(state, key_event) {
        return Some(event);
    }
    let normalized;
    let key_event = if let iocraft::prelude::KeyCode::Char(character) = key_event.code
        && character.is_ascii_uppercase()
        && key_event.modifiers.is_empty()
    {
        normalized = {
            let mut event = key_event.clone();
            event.modifiers = iocraft::prelude::KeyModifiers::SHIFT;
            event
        };
        &normalized
    } else {
        key_event
    };
    let resolved = crate::app_shell_key_routing::resolve_compiled_registry_key(state, key_event);
    let jefe::domain::action_registry::Resolution::Dispatch { handler, .. } = resolved.resolution
    else {
        return None;
    };
    let page_items = dashboard_page_item_count(state, state.screen(), terminal_cols, terminal_rows);
    action_handlers::event_for_test(handler, resolved.chord, state, page_items)
}

// Re-export the background-refresh orchestration helper so `app_shell` can
// import it from `app_input` (issue #128).
pub use gh_async::{BackgroundGhDelivery, GhDeliveryHandle, install_gh_delivery_handler};

/// Apply a typed background GitHub result on the root component's lifecycle.
pub fn apply_background_gh_delivery(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    delivery: BackgroundGhDelivery,
) {
    match delivery {
        BackgroundGhDelivery::IssueList(delivery) => {
            issues_list_dispatch::apply_issue_list_delivery(app_state, ctx, *delivery);
        }
        BackgroundGhDelivery::Apply(apply) => apply(app_state, ctx),
        #[cfg(test)]
        BackgroundGhDelivery::Probe(_) => {}
    }
    refresh_action_availability(app_state);
}

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
#[cfg(test)]
use iocraft::prelude::KeyEvent;
use tracing::{debug, warn};

use std::time::Duration;

use jefe::domain::{AgentId, AgentLaunchRequest, Repository};

use jefe::messages::{AppMessage, IssuesMessage, RuntimeMessage, UiNavigationMessage};
const REMOTE_ATTACH_SETTLE_DELAY: Duration = Duration::from_millis(150);

use jefe::runtime::{RuntimeError, RuntimeManager};

#[must_use]
fn jump_to_shortcut_agent(app_state: &mut AppStateHandle, ctx: &SharedContext, slot: u8) -> bool {
    let mut state = app_state.write();
    jefe::state::transition::commit_pure_site(
        &mut state,
        (AppEvent::JumpToAgentByShortcut(slot)).into(),
    );

    let selected_running_agent_id = state
        .selected_agent()
        .filter(|agent| agent.is_running())
        .map(|agent| agent.id.clone());

    if let Some(agent_id) = selected_running_agent_id {
        state.pane_focus = PaneFocus::Terminal;
        if !state.terminal_focused {
            jefe::state::transition::commit_pure_site(
                &mut state,
                (AppEvent::ToggleTerminalFocus).into(),
            );
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
            let persisted = durable_save_request(&mut state);
            drop(state);
            schedule_durable_save(ctx, persisted);
            return false;
        }

        clear_agent_runtime_attachment(&mut state);
        mark_agent_runtime_attached(&mut state, &agent_id, true);
        let persisted = durable_save_request(&mut state);
        drop(state);
        schedule_durable_save(ctx, persisted);
        true
    } else {
        state.terminal_focused = false;
        state.pane_focus = PaneFocus::Agents;
        let persisted = durable_save_request(&mut state);
        drop(state);
        schedule_durable_save(ctx, persisted);
        false
    }
}

use jefe::state::{AppEvent, AppState, PaneFocus, RepositoryFormFocus};

fn repository_focus_toggles_checkbox(focus: RepositoryFormFocus) -> bool {
    matches!(
        focus,
        RepositoryFormFocus::DefaultAgentType
            | RepositoryFormFocus::RemoteEnabled
            | RepositoryFormFocus::SetupEnvDefault
    )
}

pub type SharedContext = Option<Arc<std::sync::Mutex<super::AppContext>>>;
pub type AppStateHandle = HookState<AppState>;
pub type QuitHandle = HookState<bool>;

fn github_client(ctx: &SharedContext) -> Option<jefe::github::GhClient> {
    let ctx_arc = ctx.as_ref()?;
    let ctx_guard = ctx_arc.lock().ok()?;
    Some(ctx_guard.gh_client)
}
pub use persist_focus::{durable_save_request, schedule_durable_save};

fn launch_signature_for_agent(
    agent: &jefe::domain::Agent,
    repository: &Repository,
) -> AgentLaunchRequest {
    AgentLaunchRequest::for_agent(agent, repository)
}

fn launch_signature_for_new_agent(
    agent: &jefe::domain::Agent,
    repository: &Repository,
) -> AgentLaunchRequest {
    let mut request = AgentLaunchRequest::for_agent(agent, repository);
    request.operation = jefe::domain::agent_definition::Operation::Normal;
    request
}

pub fn launch_signature_for_transient(
    repository: &Repository,
    work_dir: &std::path::Path,
) -> AgentLaunchRequest {
    AgentLaunchRequest {
        type_id: repository.default_type_id.clone(),
        values: repository.default_values.clone(),
        work_dir: work_dir.to_path_buf(),
        remote: repository.remote.clone(),
        operation: jefe::domain::agent_definition::Operation::Normal,
    }
}

fn agent_and_signature(
    state: &AppState,
    agent_id: &AgentId,
) -> Option<(jefe::domain::Agent, AgentLaunchRequest)> {
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
    jefe::state::transition::commit_pure_site(&mut state, (evt).into());
    let persisted = durable_save_request(&mut state);
    drop(state);
    schedule_durable_save(ctx, persisted);
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
    signature: &AgentLaunchRequest,
    is_relaunch: bool,
) -> Result<(), RuntimeError> {
    match spawn_and_attach(app_state, ctx, agent_id, work_dir, signature, is_relaunch) {
        Ok(()) => {
            mark_launch_attached(app_state, ctx, agent_id, signature)?;
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
    app_state: &AppStateHandle,
    ctx: &SharedContext,
    agent_id: &AgentId,
    _work_dir: &std::path::Path,
    signature: &AgentLaunchRequest,
    is_relaunch: bool,
) -> Result<(), RuntimeError> {
    let evidence = availability::launch_state_evidence(app_state, signature)?;
    let prepared = jefe::runtime::launch_compose::prepare_launch(signature, &evidence)?;
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
            .spawn_session_fresh(agent_id, prepared.authorized(), prepared.remote())
    } else {
        ctx_guard
            .runtime
            .spawn_session(agent_id, prepared.authorized(), prepared.remote())
    };
    spawn_result.and_then(|()| {
        std::thread::sleep(REMOTE_ATTACH_SETTLE_DELAY);
        ctx_guard.runtime.attach(agent_id)
    })
}

fn launch_signature_for_request(
    request: &AgentLaunchRequest,
) -> Result<jefe::domain::LaunchSignatureV1, RuntimeError> {
    jefe::runtime::launch_compose::launch_signature_from_request(request)
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
    // Capture the launch error into the Errors ring buffer so it is visible
    // on the dashboard status bar and persists until superseded (issue #403
    // Bug 3). Without this, the direct write to error_message bypasses the
    // reducer's finalize_message capture path and the failure dead-ends
    // silently with the agent transitioning to Dead.
    jefe::state::capture_runtime_errors(&mut state);
    if let Some(agent) = state.agents.iter_mut().find(|agent| agent.id == *agent_id) {
        agent.runtime_binding = None;
    }
    mark_runtime_session_dead_if_present(&mut state, agent_id);
    let persisted = durable_save_request(&mut state);
    drop(state);
    schedule_durable_save(ctx, persisted);
}

fn mark_launch_attached(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: &AgentId,
    signature: &AgentLaunchRequest,
) -> Result<(), RuntimeError> {
    // Query the runtime for the process anchors before taking the app-state
    // write lock, so the persisted binding carries the PID-liveness fallback.
    let identities = bound_identities_for(ctx, agent_id);

    let mut state = app_state.write();
    set_agent_runtime_binding(
        &mut state,
        agent_id,
        jefe::runtime::RuntimeSession::session_name_for(agent_id),
        launch_signature_for_request(signature)?,
        identities,
    );
    clear_agent_runtime_attachment(&mut state);
    mark_agent_runtime_attached(&mut state, agent_id, true);
    let persisted = durable_save_request(&mut state);
    drop(state);
    schedule_durable_save(ctx, persisted);
    Ok(())
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
    jefe::state::transition::commit_pure_site(&mut state, (evt).into());
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
    let overlay_active = app_state.read().shell_overlay_active();
    let pty_layout = if overlay_active {
        jefe::layout::compute_shell_overlay_pty_layout(term_cols, term_rows)
    } else {
        jefe::layout::compute_pty_layout(term_cols, term_rows)
    };

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
        AppMessage::TerminalManager(message) => {
            let mut state = app_state.write();
            jefe::state::transition::commit_pure_site(&mut state, (AppEvent::from(message)).into());
        }
        message => apply_and_persist(app_state, ctx, AppEvent::from(message)),
    }
    refresh_action_availability(app_state);
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
    let (render_cols, render_rows) = jefe::layout::effective_render_size(term_cols, term_rows);
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
    state.issues_state.detail_content_width =
        usize::from(jefe::layout::issues_detail_content_width(render_cols));
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
#[path = "persist_projection_tests.rs"]
mod persist_projection_tests;

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
#[path = "list_send_completion_tests.rs"]
mod list_send_completion_tests;
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

// Issue #409: prompt compaction (large issue/PR bodies → gh fetch reference).
#[cfg(test)]
#[path = "prompt_compaction_tests.rs"]
mod prompt_compaction_tests;

#[cfg(test)]
#[path = "pty_passthrough_tests.rs"]
mod pty_passthrough_tests;

#[cfg(test)]
#[path = "split_mode_key_tests.rs"]
mod split_mode_key_tests;
