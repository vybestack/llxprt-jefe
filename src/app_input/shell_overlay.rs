//! Shell-overlay key dispatch (issue #222) extended with hide/resume and
//! runtime-only shell inventory (issue #361 PR A).
//!
//! Handles F10 (open/resume/close embedded shell), F12 (hide visible shell),
//! and F8 (open external terminal). F10/F12 are checked early in the key
//! event flow while the shell is active so they work even while
//! `TerminalCapture` mode owns input.

use iocraft::prelude::{KeyCode, KeyEvent};
use tracing::warn;

use jefe::domain::AgentStatus;
use jefe::runtime::{
    DesktopPlatform, ExternalTerminalError, RuntimeError, RuntimeManager, RuntimeSession,
    build_external_terminal_plan, spawn_external_terminal,
};
use jefe::state::{AppEvent, ScreenMode};

use super::{AppStateHandle, SharedContext, dispatch_app_event};

/// Graceful shutdown: close every tracked `jefe-shell` window best-effort
/// without killing agent sessions (issue #361 PR A).
///
/// Closes all shells exactly once, including the currently visible shell, so
/// the caller must not close the visible shell separately (avoids duplicate
/// close). Snapshots the visible owner + hidden inventory under a read lock,
/// releases it, then drives the best-effort runtime close under the runtime
/// lock. Clears the shell inventory afterward so the cleanup is observable.
/// Failures are logged by the runtime boundary and also returned for any
/// caller that wants to surface them.
pub fn shutdown_all_shells(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
) -> Vec<jefe::runtime::RuntimeError> {
    // Snapshot every shell owner (visible + hidden) so the visible shell is
    // closed exactly once here and not separately by cleanup_active_shell.
    let owners = app_state.read().shell_window_owners();
    if owners.is_empty() {
        return Vec::new();
    }
    // Observe + close run without an AppContext guard (issue #374). These are
    // stateless multiplexer operations that need no manager in-memory state.
    let _ = ctx; // retained for API compatibility; no lock is taken.
    let failures = match jefe::runtime::observe_shell_window_sessions() {
        Ok(session_names) => jefe::runtime::close_all_shell_windows(&session_names),
        Err(error) => vec![error],
    };
    // Clear the runtime inventory so the cleanup is observable. The visible
    // overlay is also gone after this process exits, so clearing is safe.
    {
        let mut state = app_state.write();
        state.clear_shell_inventory();
    }
    failures
}

/// Observe natural shell-window exit and restore dashboard state.
pub async fn observe_shell_exit(mut app_state: AppStateHandle, ctx: SharedContext) {
    loop {
        smol::Timer::after(std::time::Duration::from_millis(250)).await;
        let observed = {
            let state = app_state.read();
            state
                .shell_overlay_agent_id()
                .cloned()
                .map(|agent_id| (agent_id, state.shell_overlay.generation))
        };
        let Some((agent_id, generation)) = observed else {
            continue;
        };
        // Snapshot the session name under a short lock, then release it so the
        // multiplexer subprocess runs without an AppContext guard (issue #374).
        let session_name = RuntimeSession::session_name_for(&agent_id);
        let result = smol::unblock(move || jefe::runtime::shell_window_exists(&session_name)).await;
        match result {
            Ok(false) => {
                let mut state = app_state.write();
                if state.shell_overlay_agent_id() != Some(&agent_id)
                    || state.shell_overlay.generation != generation
                {
                    continue;
                }
                *state = std::mem::take(&mut *state).apply(AppEvent::CloseShellOverlay);
                drop(state);
                resize_for_active_layout(&app_state, &ctx);
            }
            Ok(true) => {}
            Err(error) => set_warning(&mut app_state, &error.to_string()),
        }
    }
}

/// Batched inventory observer (issue #361 PR A).
///
/// Periodically reconciles the runtime-only shell inventory against the
/// multiplexer ground truth off the input/render path via `smol::unblock`.
/// When a hidden shell exits naturally (e.g. the user typed `exit`), its
/// inventory entry is removed without disrupting the current view. The visible
/// overlay is handled by [`observe_shell_exit`]; this observer covers hidden
/// shells only. The runtime probe uses one batched `list-windows -a` query in
/// the supported path (issue #361 invariant: one batch query).
///
/// Probe failures (`Err`) retain entries and retry — a transient error never
/// removes inventory or marks an agent Dead (issue #361 invariant).
pub async fn observe_shell_inventory(mut app_state: AppStateHandle) {
    loop {
        // Slow cadence (~2s) so the multiplexer is not hammered and the
        // executor stays free for input/render (issue #361 invariant).
        smol::Timer::after(std::time::Duration::from_secs(2)).await;
        let candidates = {
            let state = app_state.read();
            // Only reconcile hidden shells; the visible overlay is handled by
            // observe_shell_exit.
            let visible = state.shell_overlay_agent_id().cloned();
            state
                .shell_window_owners()
                .into_iter()
                .filter(|agent_id| visible.as_ref() != Some(agent_id))
                .collect::<Vec<_>>()
        };
        if candidates.is_empty() {
            continue;
        }
        // The batched multiplexer query runs without an AppContext guard
        // (issue #374): it is a stateless observe that needs no manager state.
        let observed = smol::unblock(|| -> Result<Vec<String>, RuntimeError> {
            jefe::runtime::observe_shell_window_sessions()
        })
        .await;
        match observed {
            Ok(session_names) => {
                let missing: Vec<_> = candidates
                    .iter()
                    .filter(|agent_id| {
                        let session_name = RuntimeSession::session_name_for(agent_id);
                        !session_names.iter().any(|owner| owner == &session_name)
                    })
                    .cloned()
                    .collect();
                if missing.is_empty() {
                    continue;
                }
                let mut state = app_state.write();
                for agent_id in &missing {
                    state.remove_shell_window(agent_id);
                }
            }
            Err(error) => {
                // Retain entries on probe failure; warn without removing.
                warn!(error = %error, "shell inventory probe failed; retaining entries");
            }
        }
    }
}

pub fn resize_terminal(ctx: &SharedContext, cols: u16, rows: u16, state: &jefe::state::AppState) {
    let Some(ctx_arc) = ctx else {
        return;
    };
    let Ok(mut guard) = ctx_arc.lock() else {
        return;
    };
    let layout =
        if state.shell_overlay_active() && state.screen_mode == ScreenMode::DashboardTerminals {
            jefe::layout::compute_terminal_manager_pty_layout(cols, rows)
        } else if state.shell_overlay_active() {
            jefe::layout::compute_shell_overlay_pty_layout(cols, rows)
        } else {
            jefe::layout::compute_pty_layout(cols, rows)
        };
    if let Err(error) = guard.runtime.resize(layout.pty_rows, layout.pty_cols) {
        warn!(error = %error, "failed to resize shell terminal");
    }
}

fn is_shell_overlay_close_shortcut(key_event: &KeyEvent) -> bool {
    key_event.code == KeyCode::F(10)
}

/// Returns `true` if the key event is the shell-overlay close shortcut (F10)
/// and the overlay is active. This is called early in the key flow — before
/// `TerminalCapture` forwarding — so F10 can close the shell even while the
/// terminal owns keyboard input.
pub fn try_close_shell_overlay(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    key_event: &KeyEvent,
) -> bool {
    if !is_shell_overlay_close_shortcut(key_event) {
        return false;
    }
    let agent_id = {
        let state = app_state.read();
        state.shell_overlay_agent_id().cloned()
    };
    let Some(agent_id) = agent_id else {
        return false;
    };
    close_overlay_and_restore(app_state, ctx, &agent_id);
    true
}

/// Intercept F12 to hide the visible shell overlay (issue #361 PR A).
///
/// Hiding selects agent window 0 (so the multiplexer current window is the
/// agent pane, not `jefe-shell`), leaves the `jefe-shell` process alive, and
/// restores the previous dashboard focus/layout. The inventory entry is kept
/// so F10 can resume the exact shell later.
///
/// Called from the overlay-first key route so F12 never reaches the PTY or
/// the Windows psmux prefix. On a select-window-0 failure the overlay stays
/// visible and a warning is shown.
pub fn try_hide_shell_overlay(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    key_event: &KeyEvent,
) -> bool {
    if key_event.code != KeyCode::F(12) {
        return false;
    }
    let agent_id = {
        let state = app_state.read();
        state.shell_overlay_agent_id().cloned()
    };
    let Some(agent_id) = agent_id else {
        return false;
    };
    hide_overlay_and_restore(app_state, ctx, &agent_id);
    true
}

/// Handle the F10 / F8 shortcuts from the dashboard. Returns `true` if the key
/// was consumed.
pub fn handle_shell_shortcut_key(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    key_event: &KeyEvent,
) -> bool {
    match key_event.code {
        KeyCode::F(10) => {
            open_embedded_shell(app_state, ctx);
            true
        }
        KeyCode::F(8) => {
            open_external_terminal(app_state, ctx);
            true
        }
        _ => false,
    }
}

/// Open the embedded shell overlay for the selected local running agent.
///
/// Issue #361 PR A: if the agent already owns a hidden shell (tracked in the
/// runtime inventory), F10 resumes it instead of creating a duplicate. The
/// runtime `open_shell_window` is create-or-select so it never duplicates,
/// and the inventory records the owner after success.
fn open_embedded_shell(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let resumable = {
        let state = app_state.read();
        state
            .selected_repository()
            .and_then(|repository| jefe::state::resolve_repository_shell(&state, &repository.id))
    };
    if let Some(agent_id) = resumable {
        let repository_id = app_state
            .read()
            .repository_for_agent(&agent_id)
            .map(|repository| repository.id.clone());
        let Some(repository_id) = repository_id else {
            set_warning(app_state, "Shell owner repository is unavailable.");
            return;
        };
        super::terminal_manager::select_agent_for_focus(app_state, ctx, &repository_id, &agent_id);
        dispatch_app_event(
            app_state,
            ctx,
            AppEvent::RequestShellFocus {
                agent_id,
                origin: jefe::state::ShellFocusOrigin::DashboardF10,
            },
        );
        return;
    }

    let snapshot = read_dashboard_agent(app_state);
    let Some((agent_id, _work_dir)) = snapshot else {
        warn_no_selection(app_state, "open an embedded shell");
        return;
    };
    if app_state.read().shell_overlay_agent_id() == Some(&agent_id) {
        return;
    }

    // The open subprocess runs off-lock (issue #374 S2). Snapshot real
    // session_name/work_dir/remote from the manager under a short lock,
    // verify the owner is attached, release the lock, run the subprocess,
    // then reacquire AppContext and revalidate the attached owner before
    // dispatching the success transition.
    match open_runtime_shell_window(ctx, &agent_id) {
        Ok(inputs) => match owner_still_attached(ctx, &inputs) {
            Ok(true) => {
                dispatch_app_event(app_state, ctx, AppEvent::OpenShellOverlay);
                resize_for_active_layout(app_state, ctx);
            }
            Ok(false) => {
                warn!(agent_id = %agent_id.0, "open shell: attached owner changed while off-lock; discarding");
                compensate_open_shell(&inputs.session_name, "stale-result compensation failed");
            }
            Err(error) => {
                compensate_open_shell(&inputs.session_name, "revalidation compensation failed");
                warn!(error = %error, "open shell: unable to revalidate owner");
                set_warning(app_state, &error.to_string());
            }
        },
        Err(error) => {
            warn!(error = %error, "failed to open shell window");
            set_warning(app_state, &error.to_string());
        }
    }
}

/// Open an external terminal for the selected local agent.
fn open_external_terminal(app_state: &mut AppStateHandle, _ctx: &SharedContext) {
    let snapshot = read_local_agent(app_state, false);
    let Some((_, work_dir)) = snapshot else {
        warn_no_selection(app_state, "open an external terminal");
        return;
    };

    let platform = DesktopPlatform::current();
    match build_external_terminal_plan(&work_dir, platform) {
        Ok(plan) => {
            if let Err(error) = spawn_external_terminal(&plan) {
                warn!(error = %error, "failed to spawn external terminal");
                set_warning(app_state, &error.to_string());
            }
        }
        Err(ExternalTerminalError::InvalidWorkDir(dir)) => {
            let msg = format!("work directory not found: {dir}");
            set_warning(app_state, &msg);
        }
        Err(ExternalTerminalError::NoTerminalFound) => {
            let msg = "no terminal emulator found; set JEFE_TERMINAL to override".to_owned();
            set_warning(app_state, &msg);
        }
        Err(ExternalTerminalError::SpawnFailed(msg)) => {
            set_warning(app_state, &msg);
        }
    }
}

fn compensate_open_shell(session_name: &str, phase: &'static str) {
    if let Err(error) = jefe::runtime::hide_shell_window(session_name) {
        warn!(session_name, error = %error, phase, "open shell: compensation failed");
    }
}

/// Close the shell overlay, kill the temporary window, and restore the dashboard.
///
/// The close subprocess runs off-lock (issue #374 S2). The decision seam
/// revalidates the visible owner/generation after the subprocess: a stale
/// result is discarded (the overlay was already closed/changed) and a
/// matching result applies the transition. Failure ordering is preserved.
fn close_overlay_and_restore(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: &jefe::domain::AgentId,
) {
    let overlay_generation = app_state.read().shell_overlay.generation;
    match close_runtime_shell_window(ctx, agent_id) {
        Ok(inputs) => match owner_still_attached(ctx, &inputs) {
            Ok(true) if overlay_still_matches(app_state, agent_id, overlay_generation) => {
                dispatch_app_event(app_state, ctx, AppEvent::CloseShellOverlay);
                resize_for_active_layout(app_state, ctx);
            }
            Ok(_) => warn!(agent_id = %agent_id.0, "close shell: stale completion discarded"),
            Err(error) => {
                warn!(error = %error, "close shell: unable to revalidate owner");
                set_warning(app_state, &error.to_string());
            }
        },
        Err(error) => {
            warn!(error = %error, "failed to close shell window");
            set_warning(app_state, &error.to_string());
        }
    }
}

/// Hide the shell overlay by selecting window 0, leaving the shell alive
/// (issue #361 PR A). Runtime side effect (select-window 0) runs off-lock
/// before the state transition (issue #374 S2); on failure the overlay stays
/// visible and warns.
fn hide_overlay_and_restore(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: &jefe::domain::AgentId,
) {
    let overlay_generation = app_state.read().shell_overlay.generation;
    match hide_runtime_shell_window(ctx, agent_id) {
        Ok(inputs) => match owner_still_attached(ctx, &inputs) {
            Ok(true) if overlay_still_matches(app_state, agent_id, overlay_generation) => {
                dispatch_app_event(app_state, ctx, AppEvent::HideShellOverlay);
                resize_for_active_layout(app_state, ctx);
            }
            Ok(_) => warn!(agent_id = %agent_id.0, "hide shell: stale completion discarded"),
            Err(error) => {
                warn!(error = %error, "hide shell: unable to revalidate owner");
                set_warning(app_state, &error.to_string());
            }
        },
        Err(error) => {
            warn!(error = %error, "failed to hide shell window");
            set_warning(app_state, &error.to_string());
        }
    }
}

/// Read the selected agent + work_dir from the dashboard state. Returns `None`
/// if no agent is selected, the agent is not running, or the repository is remote.
fn read_dashboard_agent(
    app_state: &AppStateHandle,
) -> Option<(jefe::domain::AgentId, std::path::PathBuf)> {
    read_local_agent(app_state, true)
}

fn overlay_still_matches(
    app_state: &AppStateHandle,
    agent_id: &jefe::domain::AgentId,
    generation: u64,
) -> bool {
    let state = app_state.read();
    state.shell_overlay.agent_id.as_ref() == Some(agent_id)
        && state.shell_overlay.generation == generation
}

fn read_local_agent(
    app_state: &AppStateHandle,
    require_running: bool,
) -> Option<(jefe::domain::AgentId, std::path::PathBuf)> {
    let state = app_state.read();
    if state.screen_mode != ScreenMode::Dashboard {
        return None;
    }
    let agent = state.selected_agent()?;
    if require_running && agent.status != AgentStatus::Running {
        return None;
    }
    let repository = state.repository_by_id(&agent.repository_id)?;
    if repository.remote.enabled {
        return None;
    }
    let selected = (agent.id.clone(), agent.work_dir.clone());
    drop(state);
    Some(selected)
}

/// Snapshot the owner's shell-window inputs under a short lock, verify the
/// owner is attached, release the lock, then run the open subprocess without
/// an AppContext guard (issue #374 S2). Returns the snapshot inputs alongside
/// the subprocess result so the caller can revalidate ownership afterwards.
fn open_runtime_shell_window(
    ctx: &SharedContext,
    agent_id: &jefe::domain::AgentId,
) -> Result<jefe::runtime::ShellWindowInputs, RuntimeError> {
    let Some(ctx_arc) = ctx.as_ref() else {
        return Err(RuntimeError::SpawnFailed(
            "runtime context unavailable".into(),
        ));
    };
    let inputs = {
        let guard = ctx_arc
            .lock()
            .map_err(|_| RuntimeError::SpawnFailed("runtime context lock unavailable".into()))?;
        if guard.runtime.attached_agent() != Some(agent_id) {
            return Err(RuntimeError::SpawnFailed(
                "wait for the selected agent terminal to attach before opening its shell".into(),
            ));
        }
        guard
            .runtime
            .shell_window_inputs(agent_id)
            .ok_or_else(|| RuntimeError::SessionNotFound(agent_id.0.clone()))?
    };
    inputs.execute_open()?;
    Ok(inputs)
}

/// Reacquire AppContext and validate that the attached owner still matches
/// the snapshot inputs (issue #374 S2 stale-owner guard for open/select).
/// Returns `false` on lock failure or owner mismatch.
fn owner_still_attached(
    ctx: &SharedContext,
    inputs: &jefe::runtime::ShellWindowInputs,
) -> Result<bool, RuntimeError> {
    let ctx_arc = ctx
        .as_ref()
        .ok_or_else(|| RuntimeError::SpawnFailed("runtime context unavailable".into()))?;
    let guard = ctx_arc
        .lock()
        .map_err(|_| RuntimeError::SpawnFailed("runtime context lock unavailable".into()))?;
    Ok(guard.runtime.attached_agent() == Some(&inputs.owner)
        && guard.runtime.shell_window_owner_matches(inputs))
}

/// Snapshot the owner's session name under a short lock, release, then run
/// the close subprocess without an AppContext guard (issue #374 S2).
fn close_runtime_shell_window(
    ctx: &SharedContext,
    agent_id: &jefe::domain::AgentId,
) -> Result<jefe::runtime::ShellWindowInputs, RuntimeError> {
    let ctx_arc = ctx
        .as_ref()
        .ok_or_else(|| RuntimeError::SpawnFailed("runtime context unavailable".into()))?;
    let inputs = ctx_arc
        .lock()
        .map_err(|_| RuntimeError::SpawnFailed("runtime context lock unavailable".into()))?
        .runtime
        .shell_window_inputs(agent_id)
        .ok_or_else(|| RuntimeError::SessionNotFound(agent_id.0.clone()))?;
    inputs.execute_close()?;
    Ok(inputs)
}

/// Snapshot the owner's session name, release, then run the hide subprocess
/// (select window 0) without an AppContext guard (issue #374 S2).
fn hide_runtime_shell_window(
    ctx: &SharedContext,
    agent_id: &jefe::domain::AgentId,
) -> Result<jefe::runtime::ShellWindowInputs, RuntimeError> {
    let ctx_arc = ctx
        .as_ref()
        .ok_or_else(|| RuntimeError::SpawnFailed("runtime context unavailable".into()))?;
    let inputs = ctx_arc
        .lock()
        .map_err(|_| RuntimeError::SpawnFailed("runtime context lock unavailable".into()))?
        .runtime
        .shell_window_inputs(agent_id)
        .ok_or_else(|| RuntimeError::SessionNotFound(agent_id.0.clone()))?;
    inputs.execute_hide()?;
    Ok(inputs)
}

pub(super) fn resize_for_active_layout(app_state: &AppStateHandle, ctx: &SharedContext) {
    let (cols, rows) = crossterm::terminal::size().unwrap_or((120, 40));
    let state = app_state.read();
    resize_terminal(ctx, cols, rows, &state);
}

fn warn_no_selection(app_state: &mut AppStateHandle, action: &str) {
    set_warning(
        app_state,
        &format!("select a running local agent to {action}"),
    );
}

fn set_warning(app_state: &mut AppStateHandle, message: &str) {
    app_state.write().warning_message = Some(message.to_owned());
}

#[cfg(test)]
mod tests {
    use iocraft::prelude::{KeyCode, KeyEvent, KeyEventKind};

    use super::is_shell_overlay_close_shortcut;

    #[test]
    fn f10_is_the_only_shell_overlay_close_shortcut() {
        let f10 = KeyEvent::new(KeyEventKind::Press, KeyCode::F(10));
        let f11 = KeyEvent::new(KeyEventKind::Press, KeyCode::F(11));
        let character = KeyEvent::new(KeyEventKind::Press, KeyCode::Char('x'));

        assert!(is_shell_overlay_close_shortcut(&f10));
        assert!(!is_shell_overlay_close_shortcut(&f11));
        assert!(!is_shell_overlay_close_shortcut(&character));
    }
}
