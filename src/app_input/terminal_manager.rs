//! Terminal-manager key dispatch and runtime orchestration (issue #364 PR A).
//!
//! The manager is a read-only inventory browser. F7 enters it; Up/Down/Home/End
//! navigate; Ctrl-k closes the selected shell (works for non-Running owner);
//! Esc/F12 returns to the Dashboard; Enter resumes/focuses only Running owner
//! shells. Cross-agent focus reuses the existing attach scheduler by changing
//! the selected repo/agent and setting the desired attach target; the actual
//! overlay open happens only after the expected owner attaches (generation
//! guarded). Side effects always run BEFORE the deterministic reducer.

use std::time::Duration;

use iocraft::prelude::*;
use tracing::{debug, warn};

use jefe::domain::AgentId;
use jefe::runtime::{RuntimeManager, RuntimeSession, capture_shell_preview, close_shell_window};
use jefe::state::{
    AppEvent, AppState, ManagedShellRow, ScreenMode, ShellFocusOrigin, project_managed_shell_rows,
};

use super::{AppStateHandle, SharedContext, dispatch_app_event, persist_state, to_persisted_state};

/// Manager preview observer interval.
const MANAGER_PREVIEW_INTERVAL: Duration = Duration::from_secs(2);

/// Entry point: handle a key in DashboardTerminals screen mode.
///
/// Returns `Some(AppEvent)` for the reducer to apply, or `None` if the key is
/// unhandled. Runtime side effects (close, focus request) happen here BEFORE
/// the event is dispatched so the reducer stays deterministic.
pub fn handle_terminal_manager_mode_key(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    key_event: &KeyEvent,
) -> Option<AppEvent> {
    let snapshot = {
        let state = app_state.read();
        manager_key_snapshot(&state)
    };
    match key_event.code {
        // Esc / F12 return to Dashboard.
        KeyCode::Esc | KeyCode::F(12) => Some(AppEvent::ExitTerminalManagerMode),
        // Navigation.
        KeyCode::Up => Some(AppEvent::TerminalManagerNavigateUp),
        KeyCode::Down => Some(AppEvent::TerminalManagerNavigateDown),
        KeyCode::Home => Some(AppEvent::TerminalManagerNavigateHome),
        KeyCode::End => Some(AppEvent::TerminalManagerNavigateEnd),
        // Ctrl-k: close selected shell (works for non-Running owner).
        KeyCode::Char('k' | 'K') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
            if try_close_selected_shell(app_state, &snapshot) {
                snapshot
                    .selected_row
                    .as_ref()
                    .map(|row| AppEvent::ShellClosed(row.agent_id.clone()))
            } else {
                None
            }
        }
        // Enter: focus selected Running owner only.
        KeyCode::Enter => try_focus_selected_shell(app_state, ctx, &snapshot),
        _ => None,
    }
}

#[derive(Clone)]
struct ManagerKeySnapshot {
    selected_row: Option<ManagedShellRow>,
}

fn manager_key_snapshot(state: &AppState) -> ManagerKeySnapshot {
    let rows = project_managed_shell_rows(state);
    let selected_row = state
        .terminal_manager
        .selected_index
        .and_then(|idx| rows.get(idx).cloned());
    ManagerKeySnapshot { selected_row }
}

/// Close the selected shell window via the multiplexer (session-name free so
/// dead owners work). The runtime removes the inventory entry on success; the
/// reducer re-clamps selection.
fn try_close_selected_shell(app_state: &mut AppStateHandle, snapshot: &ManagerKeySnapshot) -> bool {
    let Some(row) = snapshot.selected_row.as_ref() else {
        return false;
    };
    let session_name = RuntimeSession::session_name_for(&row.agent_id);
    match close_shell_window(&session_name) {
        Ok(()) => {
            debug!(agent_id = %row.agent_id.0, "manager: closed selected shell");
            true
        }
        Err(error) => {
            warn!(agent_id = %row.agent_id.0, error = %error, "manager: close shell failed");
            app_state.write().warning_message = Some(format!("Failed to close shell: {error}"));
            false
        }
    }
}

/// Request focus on the selected Running owner shell. Sets the selected
/// repo/agent so the attach scheduler picks up the target, records the
/// generation-guarded pending focus, and returns the reducer event. The
/// actual overlay open happens after the expected owner attaches (hooked in
/// `app_shell.rs`).
fn try_focus_selected_shell(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    snapshot: &ManagerKeySnapshot,
) -> Option<AppEvent> {
    let row = snapshot.selected_row.as_ref()?;
    if row.close_only {
        // Non-Running owner: Enter is a no-op (close-only).
        let mut state = app_state.write();
        state.warning_message =
            Some("Cannot focus a non-running agent's shell (close-only).".to_string());
        drop(state);
        return None;
    }

    // Verify the shell window still exists before requesting focus (never
    // create a shell during focus). Inventory membership is the typed mirror.
    {
        let state = app_state.read();
        if !state.has_shell_window(&row.agent_id) {
            drop(state);
            let mut state = app_state.write();
            state.warning_message = Some("Selected shell no longer exists.".to_string());
            drop(state);
            return None;
        }
    }

    // Change selected repo/agent so the attach scheduler picks up the target.
    select_agent_for_focus(app_state, ctx, &row.repository_id, &row.agent_id);

    // Dispatch the reducer event to set pending focus (generation-guarded).
    // The app-shell's pending-focus observer watches this state and drives the
    // existing attach scheduler (issue #364 PR A): setting the pending state
    // is the only side effect here so the reducer stays deterministic.
    Some(AppEvent::RequestShellFocus {
        agent_id: row.agent_id.clone(),
        origin: ShellFocusOrigin::ManagerEnter,
    })
}

/// Set the dashboard selection so the attach scheduler and render path see
/// the target agent as the selected running agent.
pub(super) fn select_agent_for_focus(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    repository_id: &jefe::domain::RepositoryId,
    agent_id: &AgentId,
) {
    let mut state = app_state.write();
    if let Some(repo_idx) = state
        .repositories
        .iter()
        .position(|r| &r.id == repository_id)
    {
        state.selected_repository_index = Some(repo_idx);
    }
    if let Some(agent_idx) = state.agents.iter().position(|a| &a.id == agent_id) {
        state.selected_agent_index = Some(agent_idx);
    }
    state.pane_focus = jefe::state::PaneFocus::Terminal;
    let persisted = to_persisted_state(&state);
    drop(state);
    persist_state(ctx, &persisted);
}

/// Background observer that captures a throttled, read-only preview for the
/// selected managed shell every ~2 seconds (issue #364 PR A).
///
/// Uses targeted multiplexer capture of `<session>:jefe-shell` via
/// session-name-free runtime helpers so dead owners still produce a (failed)
/// result. Results are correlated by owner and generation/selection so stale
/// captures are discarded. Never creates a second live viewer.
pub async fn observe_terminal_manager_preview(mut app_state: AppStateHandle, ctx: SharedContext) {
    loop {
        smol::Timer::after(MANAGER_PREVIEW_INTERVAL).await;

        let snapshot = {
            let state = app_state.read();
            if !state.terminal_manager.active {
                continue;
            }
            let rows = project_managed_shell_rows(&state);
            let selected_index = state.terminal_manager.selected_index;
            let generation = state.terminal_manager.generation;
            let selected_row = selected_index.and_then(|idx| rows.get(idx).cloned());
            drop(state);
            ManagerPreviewSnapshot {
                selected_row,
                generation,
            }
        };

        let Some(target) = snapshot.selected_row else {
            continue;
        };
        let agent_id = target.agent_id.clone();
        let generation = snapshot.generation;
        let preview_owner = agent_id.clone();

        if ctx.is_none() {
            continue;
        }
        let session_name = RuntimeSession::session_name_for(&preview_owner);
        let result = smol::unblock(move || capture_shell_preview(&session_name)).await;

        let (ok, lines) = match result {
            Ok(lines) => (true, lines),
            Err(error) => {
                warn!(agent_id = %agent_id.0, error = %error, "manager: preview capture failed");
                (false, Vec::new())
            }
        };
        let event = AppEvent::ShellPreviewResult {
            agent_id,
            generation,
            ok,
            lines,
        };
        let mut state = app_state.write();
        *state = std::mem::take(&mut *state).apply(event);
        drop(state);
    }
}

struct ManagerPreviewSnapshot {
    selected_row: Option<ManagedShellRow>,
    generation: u64,
}

fn expected_focus_screen(origin: ShellFocusOrigin) -> ScreenMode {
    match origin {
        ShellFocusOrigin::DashboardF10 => ScreenMode::Dashboard,
        ShellFocusOrigin::ManagerEnter => ScreenMode::DashboardTerminals,
    }
}

fn pending_focus_matches(state: &AppStateHandle, owner: &AgentId, generation: u64) -> bool {
    state.try_read().is_some_and(|state| {
        matches!(
            state.terminal_manager.pending_focus.as_ref(),
            Some(value) if value.agent_id == *owner && value.generation == generation
        )
    })
}

fn select_pending_runtime_shell(
    state: &AppStateHandle,
    ctx: &std::sync::Arc<std::sync::Mutex<crate::AppContext>>,
    owner: &AgentId,
    generation: u64,
) -> Result<bool, jefe::runtime::RuntimeError> {
    if !pending_focus_matches(state, owner, generation) {
        return Ok(false);
    }
    let mut guard = ctx.lock().map_err(|_| {
        jefe::runtime::RuntimeError::CapabilityProbeFailed(
            "runtime context lock unavailable".to_owned(),
        )
    })?;
    if guard.runtime.attached_agent() != Some(owner) || !guard.runtime.shell_window_exists(owner)? {
        return Err(jefe::runtime::RuntimeError::SessionNotFound(
            owner.0.clone(),
        ));
    }
    guard.runtime.open_shell_window(owner)?;
    drop(guard);
    Ok(true)
}

/// Complete a pending manager focus only after the expected owner is attached
/// and its existing shell window can be selected. The blocking multiplexer
/// checks run off the executor and state is revalidated before confirmation.
pub async fn complete_pending_shell_focus(
    mut app_state: AppStateHandle,
    ctx: SharedContext,
    attached_agent_id: AgentId,
) {
    let pending = app_state.read().terminal_manager.pending_focus.clone();
    let Some(pending) = pending else {
        return;
    };
    if app_state.read().screen_mode != expected_focus_screen(pending.origin) {
        return;
    }
    if pending.agent_id != attached_agent_id {
        return;
    }
    let Some(ctx_arc) = ctx.as_ref() else {
        return;
    };
    let ctx_clone = std::sync::Arc::clone(ctx_arc);
    let owner = attached_agent_id.clone();
    let state_for_guard = app_state;
    let pending_generation = pending.generation;
    let result = smol::unblock(move || {
        select_pending_runtime_shell(&state_for_guard, &ctx_clone, &owner, pending_generation)
    })
    .await;
    let selected = match result {
        Ok(selected) => selected,
        Err(error) => {
            warn!(agent_id = %attached_agent_id.0, error = %error, "manager: focus shell failed");
            on_shell_attach_failed(&mut app_state, &attached_agent_id);
            return;
        }
    };
    if !selected {
        return;
    }
    let current = app_state.read().terminal_manager.pending_focus.clone();
    if !matches!(
        current.as_ref(),
        Some(value)
            if value.agent_id == attached_agent_id
                && value.generation == pending.generation
    ) {
        return;
    }
    dispatch_app_event(
        &mut app_state,
        &ctx,
        AppEvent::ConfirmShellFocus(attached_agent_id),
    );
}

/// Poll pending focus requests so same-owner `Stable` scheduler outcomes also
/// complete after the expected viewer is attached.
pub async fn observe_pending_shell_focus(app_state: AppStateHandle, ctx: SharedContext) {
    loop {
        smol::Timer::after(Duration::from_millis(100)).await;
        let owner = app_state
            .read()
            .terminal_manager
            .pending_focus
            .as_ref()
            .map(|pending| pending.agent_id.clone());
        let Some(owner) = owner else {
            continue;
        };
        let attached = ctx
            .as_ref()
            .and_then(|ctx_arc| ctx_arc.try_lock().ok())
            .and_then(|guard| guard.runtime.attached_agent().cloned());
        if attached.as_ref() == Some(&owner) {
            complete_pending_shell_focus(app_state, ctx.clone(), owner).await;
        }
    }
}

/// Hook the Failed outcome from the attach scheduler: clear any pending
/// terminal-manager focus and warn.
///
/// Called from `app_shell.rs` after a failed attach.
pub fn on_shell_attach_failed(app_state: &mut AppStateHandle, failed_agent_id: &AgentId) {
    let pending_matches = app_state
        .read()
        .terminal_manager
        .pending_focus
        .as_ref()
        .is_some_and(|pending| pending.agent_id == *failed_agent_id);
    if !pending_matches {
        return;
    }
    let mut state = app_state.write();
    *state = std::mem::take(&mut *state).apply(AppEvent::FailShellFocus);
    state.warning_message = Some(format!(
        "Failed to focus shell for agent {}.",
        failed_agent_id.0
    ));
}
