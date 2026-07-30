//! Rapid-quit state handling and Dashboard focus boundary operations.

use std::time::Instant;

use iocraft::prelude::KeyEvent;
use tracing::warn;

use jefe::domain::{AgentId, RepositoryId};
use jefe::input::{QuitOutcome, observe_quit_sequence};
use jefe::runtime::RuntimeManager;
use jefe::state::{AppEvent, PaneFocus};

use super::{
    AppStateHandle, QuitHandle, SharedContext, durable_save_request, schedule_durable_save,
};

pub fn observe_rapid_quit(
    app_state: &mut AppStateHandle,
    should_quit: &mut QuitHandle,
    key_event: &KeyEvent,
) -> bool {
    if jefe::input::is_quit_key(key_event) {
        return false;
    }
    let outcome = {
        let mut state = app_state.write();
        observe_quit_sequence(&mut state.quit_sequence, key_event, Instant::now())
    };
    match outcome {
        QuitOutcome::Quit => {
            should_quit.set(true);
            true
        }
        QuitOutcome::Continue => true,
        QuitOutcome::Reset => false,
    }
}

pub(super) fn select_first_visible_repository(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
) -> Option<RepositoryId> {
    let state = app_state.read();
    let first_visible_idx = state.visible_repository_indices().first().copied();
    let first_id = first_visible_idx.and_then(|index| {
        state
            .repositories
            .get(index)
            .map(|repository| repository.id.clone())
    });
    drop(state);

    if let Some(index) = first_visible_idx {
        let mut state = app_state.write();
        state.selected_repository_index = Some(index);
        state.normalize_selection_indices();
        let persisted = durable_save_request(&mut state);
        drop(state);
        schedule_durable_save(ctx, persisted);
    }
    first_id
}

pub(super) fn set_pane_focus(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    pane_focus: PaneFocus,
) {
    let mut state = app_state.write();
    state.pane_focus = pane_focus;
    state.dashboard_grab = None;
    let persisted = durable_save_request(&mut state);
    drop(state);
    schedule_durable_save(ctx, persisted);
}

pub(super) fn focus_terminal_pane(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let selected_running_agent_id = prepare_terminal_focus_state(app_state);
    if let Some(agent_id) = selected_running_agent_id {
        attach_terminal_focus(app_state, ctx, &agent_id);
    } else {
        set_pane_focus(app_state, ctx, PaneFocus::Agents);
    }
}

fn prepare_terminal_focus_state(app_state: &mut AppStateHandle) -> Option<AgentId> {
    let mut state = app_state.write();
    let running_agent_id = state
        .selected_agent()
        .filter(|agent| agent.is_running())
        .map(|agent| agent.id.clone());
    if running_agent_id.is_some() {
        state.pane_focus = PaneFocus::Terminal;
        state.dashboard_grab = None;
        if !state.terminal_focused {
            jefe::state::transition::commit_pure_site(
                &mut state,
                (AppEvent::ToggleTerminalFocus).into(),
            );
        }
    } else {
        state.pane_focus = PaneFocus::Agents;
        state.dashboard_grab = None;
        state.terminal_focused = false;
    }
    running_agent_id
}

fn attach_terminal_focus(app_state: &mut AppStateHandle, ctx: &SharedContext, agent_id: &AgentId) {
    if let Some(ctx_arc) = ctx
        && let Ok(mut ctx_guard) = ctx_arc.lock()
        && let Err(error) = ctx_guard.runtime.attach(agent_id)
    {
        warn!(
            agent_id = %agent_id.0,
            error = %error,
            "could not attach session on 't' focus"
        );
        set_pane_focus(app_state, ctx, PaneFocus::Agents);
    }
}
