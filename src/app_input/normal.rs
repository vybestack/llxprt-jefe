//! Normal-mode keyboard event dispatch.

use iocraft::prelude::*;
use tracing::{debug, warn};

use jefe::runtime::RuntimeManager;
use jefe::state::{AppEvent, PaneFocus, ScreenMode};
use jefe::theme::ThemeManager;

use super::{
    AppStateHandle, MAC_ALT_DIGIT_SHORTCUTS, QuitHandle, SharedContext, jump_to_shortcut_agent,
    persist_state_snapshot,
};

fn mac_alt_digit_slot(c: char) -> Option<u8> {
    MAC_ALT_DIGIT_SHORTCUTS
        .iter()
        .find_map(|(symbol, slot)| (*symbol == c).then_some(*slot))
}

fn try_extract_shortcut_slot(key_event: &KeyEvent) -> Option<u8> {
    match key_event.code {
        KeyCode::Char(c) => {
            if key_event.modifiers.contains(KeyModifiers::ALT) {
                if let Some(digit) = c.to_digit(10)
                    && (1..=9).contains(&digit)
                {
                    return u8::try_from(digit).ok();
                }
            }

            // macOS default Option+digit emits these symbols when Option is not in Meta mode.
            if !key_event.modifiers.contains(KeyModifiers::CONTROL)
                && !key_event.modifiers.contains(KeyModifiers::SUPER)
                && !key_event.modifiers.contains(KeyModifiers::META)
                && let Some(slot) = mac_alt_digit_slot(c)
            {
                return Some(slot);
            }

            None
        }
        _ => None,
    }
}

fn relaunch_event_for_selected_agent(
    selected_agent_id: Option<jefe::domain::AgentId>,
    selected_agent_is_running: bool,
) -> Option<AppEvent> {
    if selected_agent_is_running {
        None
    } else {
        selected_agent_id.map(AppEvent::RelaunchAgent)
    }
}

pub fn handle_global_shortcut_key(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    key_event: &KeyEvent,
) -> bool {
    if let Some(slot) = try_extract_shortcut_slot(key_event) {
        let _ = jump_to_shortcut_agent(app_state, ctx, slot);
        return true;
    }

    false
}

#[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
pub fn handle_normal_key_event(
    app_state: &mut AppStateHandle,
    should_quit: &mut QuitHandle,
    ctx: &SharedContext,
    key_event: &KeyEvent,
    screen_mode: ScreenMode,
) -> Option<AppEvent> {
    let state_ro = app_state.read();
    let pane_focus = state_ro.pane_focus;
    let selected_agent_is_running = state_ro
        .selected_agent()
        .is_some_and(jefe::domain::Agent::is_running);
    let selected_repo_id = state_ro
        .selected_repository()
        .map(|repository| repository.id.clone());
    let selected_agent_id = state_ro.selected_agent().map(|agent| agent.id.clone());
    drop(state_ro);

    // Issues mode routing — route to issues handler when in DashboardIssues.
    // Quit is handled here (not in the issues resolver) because it uses
    // the `should_quit` handle which is not an AppEvent.
    // @plan PLAN-20260329-ISSUES-MODE.P09
    // @requirement REQ-ISS-002
    if screen_mode == ScreenMode::DashboardIssues {
        if matches!(key_event.code, KeyCode::Char('q' | 'Q')) {
            should_quit.set(true);
            return None;
        }
        return super::issues::handle_issues_mode_key(&*app_state, ctx, key_event);
    }

    match key_event.code {
        // Quit
        KeyCode::Char('q' | 'Q') => {
            should_quit.set(true);
            None
        }

        // Navigation
        KeyCode::Up => Some(AppEvent::NavigateUp),
        KeyCode::Down => Some(AppEvent::NavigateDown),
        KeyCode::Left => Some(AppEvent::NavigateLeft),
        KeyCode::Right => Some(AppEvent::NavigateRight),
        KeyCode::Tab => Some(AppEvent::CyclePaneFocus),

        // New (n = new agent, N = new repository)
        KeyCode::Char('n') => {
            debug!(
                selected_repo_id = ?selected_repo_id,
                "n pressed: deriving new agent/repo action"
            );
            // If no repo is selected but repos exist, auto-select the first visible one.
            let repo_id = selected_repo_id.clone().or_else(|| {
                let state = app_state.read();
                let first_visible_idx = state.visible_repository_indices().first().copied();
                let first_id = first_visible_idx.and_then(|idx| {
                    state
                        .repositories
                        .get(idx)
                        .map(|repository| repository.id.clone())
                });
                drop(state);
                if let Some(first_visible_idx) = first_visible_idx {
                    let mut state_mut = app_state.write();
                    state_mut.selected_repository_index = Some(first_visible_idx);
                    state_mut.normalize_selection_indices();
                    persist_state_snapshot(ctx, &state_mut);
                }
                first_id
            });
            if repo_id.is_none() {
                debug!("n: no repos → OpenNewRepository");
                Some(AppEvent::OpenNewRepository)
            } else {
                debug!(repo_id = ?repo_id, "n: repo exists → OpenNewAgent");
                repo_id.map(AppEvent::OpenNewAgent)
            }
        }
        KeyCode::Char('N') => {
            debug!("N pressed: OpenNewRepository");
            Some(AppEvent::OpenNewRepository)
        }

        // Delete
        KeyCode::Char('d' | 'D') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
            if pane_focus == PaneFocus::Agents || pane_focus == PaneFocus::Terminal {
                selected_agent_id.clone().map(AppEvent::OpenDeleteAgent)
            } else if pane_focus == PaneFocus::Repositories {
                selected_repo_id.clone().map(AppEvent::OpenDeleteRepository)
            } else {
                None
            }
        }

        // Kill agent
        KeyCode::Char('k' | 'K') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
            selected_agent_id.clone().map(AppEvent::KillAgent)
        }

        // Relaunch agent (dead/non-running only)
        KeyCode::Char('l' | 'L') => {
            relaunch_event_for_selected_agent(selected_agent_id.clone(), selected_agent_is_running)
        }

        // Issues mode entry
        // @plan PLAN-20260329-ISSUES-MODE.P11
        // @requirement REQ-ISS-001
        // @pseudocode component-003 lines 01-02
        KeyCode::Char('i' | 'I') if screen_mode == ScreenMode::Dashboard => {
            Some(AppEvent::EnterIssuesMode)
        }

        // Split mode
        KeyCode::Char('s' | 'S') if screen_mode == ScreenMode::Dashboard => {
            Some(AppEvent::EnterSplitMode)
        }
        KeyCode::Esc if screen_mode == ScreenMode::Split => Some(AppEvent::ExitSplitMode),

        // Grab mode (in split screen)
        KeyCode::Char('g' | 'G') if screen_mode == ScreenMode::Split => {
            Some(AppEvent::EnterGrabMode)
        }

        // Help and search
        KeyCode::Char('?' | 'h' | 'H') | KeyCode::F(1) => Some(AppEvent::OpenHelp),
        KeyCode::Char('/') => Some(AppEvent::OpenSearch),

        // Theme picker
        KeyCode::Char('P') if screen_mode == ScreenMode::Dashboard => {
            if let Some(ctx_arc) = &ctx
                && let Ok(ctx_guard) = ctx_arc.lock()
            {
                let available = ctx_guard.theme_manager.themes_with_names();
                let active = ctx_guard.theme_manager.active_theme().slug.clone();
                drop(ctx_guard);
                Some(AppEvent::OpenThemePicker {
                    available_themes: available,
                    active_slug: active,
                })
            } else {
                None
            }
        }

        // Repository visibility filter
        KeyCode::Char('v' | 'V') if screen_mode == ScreenMode::Dashboard => {
            Some(AppEvent::ToggleHideIdleRepositories)
        }

        // Direct pane focus
        KeyCode::Char('r' | 'R') => {
            let mut state = app_state.write();
            state.pane_focus = PaneFocus::Repositories;
            persist_state_snapshot(ctx, &state);
            None
        }
        KeyCode::Char('a' | 'A') => {
            let mut state = app_state.write();
            state.pane_focus = PaneFocus::Agents;
            persist_state_snapshot(ctx, &state);
            None
        }
        KeyCode::Char('t' | 'T') => {
            let selected_running_agent_id = {
                let mut state = app_state.write();
                let running_agent_id = state
                    .selected_agent()
                    .filter(|agent| agent.is_running())
                    .map(|agent| agent.id.clone());

                if running_agent_id.is_some() {
                    state.pane_focus = PaneFocus::Terminal;
                    if !state.terminal_focused {
                        *state = std::mem::take(&mut *state).apply(AppEvent::ToggleTerminalFocus);
                    }
                } else {
                    state.pane_focus = PaneFocus::Agents;
                    state.terminal_focused = false;
                }

                running_agent_id
            };

            if let Some(agent_id) = selected_running_agent_id {
                if let Some(ctx_arc) = &ctx
                    && let Ok(mut ctx_guard) = ctx_arc.lock()
                    && let Err(e) = ctx_guard.runtime.attach(&agent_id)
                {
                    warn!(
                        agent_id = %agent_id.0,
                        error = %e,
                        "could not attach session on 't' focus"
                    );
                    let mut state = app_state.write();
                    state.terminal_focused = false;
                    state.pane_focus = PaneFocus::Agents;
                    persist_state_snapshot(ctx, &state);
                }
            } else {
                let mut state = app_state.write();
                state.terminal_focused = false;
                state.pane_focus = PaneFocus::Agents;
                persist_state_snapshot(ctx, &state);
            }

            None
        }

        // Enter selects current item (edit agent/repo)
        KeyCode::Enter => match pane_focus {
            PaneFocus::Agents => selected_agent_id.clone().map(AppEvent::OpenEditAgent),
            PaneFocus::Repositories => selected_repo_id.clone().map(AppEvent::OpenEditRepository),
            PaneFocus::Terminal => {
                // Toggle terminal focus on Enter when in terminal pane.
                Some(AppEvent::ToggleTerminalFocus)
            }
        },

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::relaunch_event_for_selected_agent;
    use jefe::domain::AgentId;
    use jefe::state::AppEvent;

    #[test]
    fn relaunch_event_is_none_for_running_agent() {
        let evt = relaunch_event_for_selected_agent(Some(AgentId(String::from("a1"))), true);
        assert!(evt.is_none());
    }

    #[test]
    fn relaunch_event_is_emitted_for_non_running_agent() {
        let evt = relaunch_event_for_selected_agent(Some(AgentId(String::from("a1"))), false);
        assert!(matches!(
            evt,
            Some(AppEvent::RelaunchAgent(AgentId(id))) if id == "a1"
        ));
    }
}
