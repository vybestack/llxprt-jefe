//! Shell-overlay key dispatch (issue #222).
//!
//! Handles F10 (open embedded shell), F8 (open external terminal), and F11
//! (close embedded shell). F11 is checked early in the key event flow so it
//! works even while `TerminalCapture` mode owns input.

use iocraft::prelude::{KeyCode, KeyEvent};
use tracing::warn;

use jefe::domain::AgentStatus;
use jefe::runtime::{
    DesktopPlatform, ExternalTerminalError, RuntimeError, RuntimeManager,
    build_external_terminal_plan, spawn_external_terminal,
};
use jefe::state::{AppEvent, ScreenMode};

use super::{AppStateHandle, SharedContext, dispatch_app_event};
pub fn cleanup_active_shell(state: &jefe::state::AppState, ctx: &SharedContext) {
    let Some(agent_id) = state.shell_overlay_agent_id() else {
        return;
    };
    if let Some(ctx_arc) = ctx
        && let Ok(mut guard) = ctx_arc.lock()
    {
        let _ = guard.runtime.close_shell_window(agent_id);
    }
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
        let Some(ctx_arc) = ctx.clone() else {
            continue;
        };
        let queried_agent = agent_id.clone();
        let result = smol::unblock(move || {
            let guard = ctx_arc.lock().map_err(|_| {
                RuntimeError::CapabilityProbeFailed("runtime context lock unavailable".to_owned())
            })?;
            guard.runtime.shell_window_exists(&queried_agent)
        })
        .await;
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
                resize_for_active_layout(&ctx, false);
            }
            Ok(true) => {}
            Err(error) => set_warning(&mut app_state, &error.to_string()),
        }
    }
}

pub fn resize_terminal(ctx: &SharedContext, cols: u16, rows: u16, overlay_active: bool) {
    let Some(ctx_arc) = ctx else {
        return;
    };
    let Ok(mut guard) = ctx_arc.lock() else {
        return;
    };
    let layout = if overlay_active {
        jefe::layout::compute_shell_overlay_pty_layout(cols, rows)
    } else {
        jefe::layout::compute_pty_layout(cols, rows)
    };
    let _ = guard.runtime.resize(layout.pty_rows, layout.pty_cols);
}

/// Returns `true` if the key event is the shell-overlay close shortcut (F11)
/// and the overlay is active. This is called early in the key flow — before
/// `TerminalCapture` forwarding — so F11 can close the shell even while the
/// terminal owns keyboard input.
pub fn try_close_shell_overlay(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    key_event: &KeyEvent,
) -> bool {
    if key_event.code != KeyCode::F(11) {
        return false;
    }
    let overlay_active = app_state.read().shell_overlay_active();
    if !overlay_active {
        return false;
    }

    let agent_id = app_state.read().shell_overlay_agent_id().cloned();
    if let Some(agent_id) = agent_id {
        close_overlay_and_restore(app_state, ctx, &agent_id);
    }
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
fn open_embedded_shell(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let snapshot = read_dashboard_agent(app_state);
    let Some((agent_id, _work_dir)) = snapshot else {
        warn_no_selection(app_state, "open an embedded shell");
        return;
    };

    // Idempotency: if the overlay is already active for this agent, no-op.
    let already_active = {
        let state = app_state.read();
        state.shell_overlay_agent_id() == Some(&agent_id)
    };
    if already_active {
        return;
    }

    // Call the runtime to open the shell window before transitioning state.
    let result = open_runtime_shell_window(ctx, &agent_id);
    match result {
        Ok(()) => {
            dispatch_app_event(app_state, ctx, AppEvent::OpenShellOverlay);
            resize_for_active_layout(ctx, true);
        }
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

/// Close the shell overlay, kill the temporary window, and restore the dashboard.
fn close_overlay_and_restore(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: &jefe::domain::AgentId,
) {
    let result = close_runtime_shell_window(ctx, agent_id);
    match result {
        Ok(()) => {
            dispatch_app_event(app_state, ctx, AppEvent::CloseShellOverlay);
            resize_for_active_layout(ctx, false);
        }
        Err(error) => {
            warn!(error = %error, "failed to close shell window");
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

fn open_runtime_shell_window(
    ctx: &SharedContext,
    agent_id: &jefe::domain::AgentId,
) -> Result<(), RuntimeError> {
    let Some(ctx_arc) = ctx.as_ref() else {
        return Err(RuntimeError::SpawnFailed(
            "runtime context unavailable".into(),
        ));
    };
    let Ok(mut guard) = ctx_arc.lock() else {
        return Err(RuntimeError::SpawnFailed(
            "runtime context lock unavailable".into(),
        ));
    };
    if guard.runtime.attached_agent() != Some(agent_id) {
        return Err(RuntimeError::SpawnFailed(
            "wait for the selected agent terminal to attach before opening its shell".into(),
        ));
    }
    guard.runtime.open_shell_window(agent_id)
}

fn close_runtime_shell_window(
    ctx: &SharedContext,
    agent_id: &jefe::domain::AgentId,
) -> Result<(), RuntimeError> {
    let Some(ctx_arc) = ctx.as_ref() else {
        return Err(RuntimeError::SpawnFailed(
            "runtime context unavailable".into(),
        ));
    };
    let Ok(mut guard) = ctx_arc.lock() else {
        return Err(RuntimeError::SpawnFailed(
            "runtime context lock unavailable".into(),
        ));
    };
    guard.runtime.close_shell_window(agent_id)
}

fn resize_for_active_layout(ctx: &SharedContext, overlay_active: bool) {
    let (cols, rows) = crossterm::terminal::size().unwrap_or((120, 40));
    resize_terminal(ctx, cols, rows, overlay_active);
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
