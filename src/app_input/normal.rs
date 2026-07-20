//! Normal-mode keyboard event dispatch.

use std::time::Instant;

use iocraft::prelude::*;
use tracing::{debug, warn};

use jefe::domain::{AgentId, RepositoryId};
use jefe::input::{InputMode, QuitOutcome, input_mode_for_state, observe_quit_sequence};
use jefe::list_viewport::PageItemCount;
use jefe::runtime::RuntimeManager;
use jefe::state::{AppEvent, AppState, PaneFocus, ScreenMode};
use jefe::theme::ThemeManager;

use super::{
    AppStateHandle, MAC_ALT_DIGIT_SHORTCUTS, QuitHandle, SharedContext, jump_to_shortcut_agent,
    persist_state, to_persisted_state,
};

#[derive(Clone)]
struct NormalKeySnapshot {
    pane_focus: PaneFocus,
    selected_agent_is_running: bool,
    selected_repo_id: Option<RepositoryId>,
    selected_agent_id: Option<AgentId>,
}

#[derive(Debug)]
pub(super) enum KeyHandling {
    Unhandled,
    Handled(Option<AppEvent>),
}

fn mac_alt_digit_slot(c: char) -> Option<u8> {
    MAC_ALT_DIGIT_SHORTCUTS
        .iter()
        .find_map(|(symbol, slot)| (*symbol == c).then_some(*slot))
}

fn try_extract_shortcut_slot(key_event: &KeyEvent) -> Option<u8> {
    match key_event.code {
        KeyCode::Char(c) => {
            if key_event.modifiers.contains(KeyModifiers::ALT)
                && let Some(digit) = c.to_digit(10)
                && (1..=9).contains(&digit)
            {
                return u8::try_from(digit).ok();
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
    selected_agent_id: Option<AgentId>,
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

pub fn handle_normal_key_event(
    app_state: &mut AppStateHandle,
    should_quit: &mut QuitHandle,
    ctx: &SharedContext,
    key_event: &KeyEvent,
    screen_mode: ScreenMode,
) -> Option<AppEvent> {
    if let KeyHandling::Handled(event) =
        resolve_quit(app_state, should_quit, key_event, screen_mode)
    {
        return event;
    }
    for handling in [
        handle_dashboard_issues_key(app_state, ctx, key_event, screen_mode),
        handle_dashboard_prs_key(app_state, ctx, key_event, screen_mode),
        handle_dashboard_actions_key(app_state, ctx, key_event, screen_mode),
    ] {
        if let KeyHandling::Handled(event) = handling {
            return event;
        }
    }
    if screen_mode == ScreenMode::DashboardTerminals {
        return super::terminal_manager::handle_terminal_manager_mode_key(
            app_state, ctx, key_event,
        );
    }
    if screen_mode == ScreenMode::DashboardErrors {
        return super::errors::handle_errors_mode_key(app_state, ctx, key_event);
    }
    let snapshot = normal_key_snapshot(app_state);
    if let KeyHandling::Handled(event) = resolve_dashboard_grab_key(app_state, key_event) {
        return event;
    }
    let page_items = dashboard_page_items(app_state, screen_mode);
    if let KeyHandling::Handled(event) = resolve_navigation_key(key_event, page_items) {
        return event;
    }
    if let KeyHandling::Handled(event) = resolve_new_key(app_state, ctx, key_event, &snapshot) {
        return event;
    }
    if let KeyHandling::Handled(event) = resolve_agent_lifecycle_key(key_event, &snapshot) {
        return event;
    }
    if let KeyHandling::Handled(event) = resolve_mode_key(key_event, screen_mode) {
        return event;
    }
    if let KeyHandling::Handled(event) = resolve_help_search_key(key_event) {
        return event;
    }
    if let KeyHandling::Handled(event) = resolve_visibility_key(key_event, screen_mode) {
        return event;
    }
    if let KeyHandling::Handled(event) = handle_direct_pane_focus_key(app_state, ctx, key_event) {
        return event;
    }
    if let KeyHandling::Handled(event) = resolve_enter_key(key_event, &snapshot) {
        return event;
    }
    if let KeyHandling::Handled(event) = handle_theme_key(app_state, ctx, key_event, screen_mode) {
        return event;
    }
    None
}

fn normal_key_snapshot(app_state: &AppStateHandle) -> NormalKeySnapshot {
    let state = app_state.read();
    NormalKeySnapshot {
        pane_focus: state.pane_focus,
        selected_agent_is_running: state
            .selected_agent()
            .is_some_and(jefe::domain::Agent::is_running),

        selected_repo_id: state
            .selected_repository()
            .map(|repository| repository.id.clone()),
        selected_agent_id: state.selected_agent().map(|agent| agent.id.clone()),
    }
}

/// Whether the global quit shortcut (`Ctrl-Q` / rapid `qqq`) should be eligible
/// to act for the current screen and input mode.
///
/// Quit is eligible in the plain navigation sub-modes — `Dashboard` normal,
/// `Split`, `IssuesNormal`, and `PrsNormal` — and explicitly *not* in any
/// text-capturing or overlay sub-mode, so a `q` typed in a composer/search/
/// filter is never swallowed by quit. `Split` has no text-capturing sub-modes
/// and does not bind `q` for anything else, so quit stays eligible there (a
/// bare `q` harmlessly advances the `qqq` sequence).
fn quit_shortcut_active(state: &AppState, screen_mode: ScreenMode) -> bool {
    match screen_mode {
        ScreenMode::Dashboard
        | ScreenMode::Split
        | ScreenMode::DashboardErrors
        | ScreenMode::DashboardTerminals => true,
        ScreenMode::DashboardIssues => issues_quit_shortcut_active(state),
        ScreenMode::DashboardPullRequests => prs_quit_shortcut_active(state),
        ScreenMode::DashboardActions => actions_quit_shortcut_active(state),
    }
}

/// Unified quit resolver: the instant `Ctrl-Q` chord or the rapid `qqq`
/// sequence. Checked first in the normal-mode dispatch so the quit trigger is
/// honored in every eligible sub-mode and any pending `q` is swallowed before
/// lower handlers run.
fn resolve_quit(
    app_state: &mut AppStateHandle,
    should_quit: &mut QuitHandle,
    key_event: &KeyEvent,
    screen_mode: ScreenMode,
) -> KeyHandling {
    let eligible = {
        let state = app_state.read();
        quit_shortcut_active(&state, screen_mode)
    };
    if !eligible {
        return KeyHandling::Unhandled;
    }
    let outcome = {
        let mut state = app_state.write();
        observe_quit_sequence(&mut state.quit_sequence, key_event, Instant::now())
    };
    match outcome {
        QuitOutcome::Quit => {
            should_quit.set(true);
            KeyHandling::Handled(None)
        }
        // A `q` is accumulating toward `qqq`: consume it so it neither quits
        // nor reaches a lower handler.
        QuitOutcome::Continue => KeyHandling::Handled(None),
        // Unrelated key: let the rest of the dispatch handle it.
        QuitOutcome::Reset => KeyHandling::Unhandled,
    }
}

/// Returns true when the global quit shortcut should act while in
/// Issues Mode. Quit only applies in the plain `IssuesNormal` sub-mode; any
/// text-capturing or overlay sub-mode must receive the key so it is
/// not swallowed by quit.
fn issues_quit_shortcut_active(state: &AppState) -> bool {
    matches!(input_mode_for_state(state), InputMode::IssuesNormal)
}

fn handle_dashboard_issues_key(
    app_state: &AppStateHandle,
    ctx: &SharedContext,
    key_event: &KeyEvent,
    screen_mode: ScreenMode,
) -> KeyHandling {
    if screen_mode != ScreenMode::DashboardIssues {
        return KeyHandling::Unhandled;
    }

    // Quit is resolved centrally by `resolve_quit` before this handler runs;
    // every remaining key is delegated to Issues mode (and consumed).
    KeyHandling::Handled(super::issues::handle_issues_mode_key(
        app_state, ctx, key_event,
    ))
}

/// Returns true when the global quit shortcut should act while in
/// PR Mode. Quit only applies in the plain `PrsNormal` sub-mode; any
/// text-capturing or overlay sub-mode must receive the key.
///
/// @plan PLAN-20260624-PR-MODE.P09
/// @requirement REQ-PR-002
/// @pseudocode component-003 lines 05-09
fn prs_quit_shortcut_active(state: &AppState) -> bool {
    matches!(input_mode_for_state(state), InputMode::PrsNormal)
}

/// Route key events when `screen_mode == DashboardPullRequests`.
///
/// Mirrors `handle_dashboard_issues_key`: if the quit shortcut is active and
/// the key is the quit shortcut, quit; otherwise delegate to `prs::handle_prs_mode_key`.
/// The entire result is wrapped in `KeyHandling::Handled(...)` so every key is
/// consumed while in PR Mode (never leaks to dashboard/destructive handlers).
///
/// @plan PLAN-20260624-PR-MODE.P09
/// @requirement REQ-PR-001
/// @requirement REQ-PR-002
/// @pseudocode component-003 lines 05-14
fn handle_dashboard_prs_key(
    app_state: &AppStateHandle,
    ctx: &SharedContext,
    key_event: &KeyEvent,
    screen_mode: ScreenMode,
) -> KeyHandling {
    if screen_mode != ScreenMode::DashboardPullRequests {
        return KeyHandling::Unhandled;
    }

    // Quit is resolved centrally by `resolve_quit` before this handler runs;
    // every remaining key is delegated to PR mode (and consumed).
    KeyHandling::Handled(super::prs::handle_prs_mode_key(app_state, ctx, key_event))
}

fn actions_quit_shortcut_active(state: &AppState) -> bool {
    matches!(input_mode_for_state(state), InputMode::ActionsNormal)
}

fn handle_dashboard_actions_key(
    app_state: &AppStateHandle,
    ctx: &SharedContext,
    key_event: &KeyEvent,
    screen_mode: ScreenMode,
) -> KeyHandling {
    if screen_mode != ScreenMode::DashboardActions {
        return KeyHandling::Unhandled;
    }

    // Quit is resolved centrally by `resolve_quit` before this handler runs;
    // every remaining key is delegated to Actions mode (and consumed).
    KeyHandling::Handled(super::actions::handle_actions_mode_key(
        app_state, ctx, key_event,
    ))
}

/// Dashboard reorder grab interaction: Space grabs, arrows move, Space/Enter drops.
///
/// Only active on the Dashboard screen. When a grab is in progress, Space/Enter
/// drops and Up/Down move; other keys fall through. When no grab is active,
/// Space grabs the highlighted item in the Repositories or Agents pane (the
/// terminal pane is a no-op so Space passes through to the PTY).
fn resolve_dashboard_grab_key(app_state: &AppStateHandle, key_event: &KeyEvent) -> KeyHandling {
    let state = app_state.read();
    if state.screen_mode != ScreenMode::Dashboard || state.terminal_focused {
        return KeyHandling::Unhandled;
    }
    match state.dashboard_grab {
        Some(_) => match key_event.code {
            KeyCode::Char(' ') | KeyCode::Enter => {
                KeyHandling::Handled(Some(AppEvent::ExitDashboardGrab))
            }
            KeyCode::Up => KeyHandling::Handled(Some(AppEvent::DashboardGrabMoveUp)),
            KeyCode::Down => KeyHandling::Handled(Some(AppEvent::DashboardGrabMoveDown)),
            _ => KeyHandling::Unhandled,
        },
        None => {
            if key_event.code == KeyCode::Char(' ') {
                match state.pane_focus {
                    PaneFocus::Repositories | PaneFocus::Agents => {
                        KeyHandling::Handled(Some(AppEvent::EnterDashboardGrab))
                    }
                    PaneFocus::Terminal => KeyHandling::Unhandled,
                }
            } else {
                KeyHandling::Unhandled
            }
        }
    }
}

fn dashboard_page_items(app_state: &AppStateHandle, screen_mode: ScreenMode) -> PageItemCount {
    let (terminal_cols, terminal_rows) = crossterm::terminal::size().unwrap_or((120, 40));
    super::list_navigation::dashboard_page_item_count(
        &app_state.read(),
        screen_mode,
        terminal_cols,
        terminal_rows,
    )
}

fn resolve_navigation_key(key_event: &KeyEvent, page_items: PageItemCount) -> KeyHandling {
    match key_event.code {
        KeyCode::Up => KeyHandling::Handled(Some(AppEvent::NavigateUp)),
        KeyCode::Down => KeyHandling::Handled(Some(AppEvent::NavigateDown)),
        KeyCode::PageUp => KeyHandling::Handled(Some(AppEvent::NavigatePageUp(page_items))),
        KeyCode::PageDown => KeyHandling::Handled(Some(AppEvent::NavigatePageDown(page_items))),
        KeyCode::Home => KeyHandling::Handled(Some(AppEvent::NavigateHome)),
        KeyCode::End => KeyHandling::Handled(Some(AppEvent::NavigateEnd)),
        KeyCode::Left => KeyHandling::Handled(Some(AppEvent::NavigateLeft)),
        KeyCode::Right => KeyHandling::Handled(Some(AppEvent::NavigateRight)),
        KeyCode::Tab => KeyHandling::Handled(Some(AppEvent::CyclePaneFocus)),
        _ => KeyHandling::Unhandled,
    }
}

fn resolve_new_key(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    key_event: &KeyEvent,
    snapshot: &NormalKeySnapshot,
) -> KeyHandling {
    match key_event.code {
        KeyCode::Char('n') => {
            KeyHandling::Handled(new_agent_or_repository_event(app_state, ctx, snapshot))
        }
        KeyCode::Char('N') => {
            debug!("N pressed: OpenNewRepository");
            KeyHandling::Handled(Some(AppEvent::OpenNewRepository))
        }
        _ => KeyHandling::Unhandled,
    }
}

fn new_agent_or_repository_event(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    snapshot: &NormalKeySnapshot,
) -> Option<AppEvent> {
    debug!(
        selected_repo_id = ?snapshot.selected_repo_id,
        "n pressed: deriving new agent/repo action"
    );
    let repo_id = snapshot
        .selected_repo_id
        .clone()
        .or_else(|| select_first_visible_repository(app_state, ctx));
    if repo_id.is_none() {
        debug!("n: no repos → OpenNewRepository");
        Some(AppEvent::OpenNewRepository)
    } else {
        debug!(repo_id = ?repo_id, "n: repo exists → OpenNewAgent");
        repo_id.map(AppEvent::OpenNewAgent)
    }
}

fn select_first_visible_repository(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
) -> Option<RepositoryId> {
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
        let persisted = to_persisted_state(&state_mut);
        drop(state_mut);
        persist_state(ctx, &persisted);
    }
    first_id
}

fn resolve_agent_lifecycle_key(key_event: &KeyEvent, snapshot: &NormalKeySnapshot) -> KeyHandling {
    match key_event.code {
        KeyCode::Char('d' | 'D') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
            KeyHandling::Handled(delete_event(snapshot))
        }
        KeyCode::Char('k' | 'K') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
            KeyHandling::Handled(snapshot.selected_agent_id.clone().map(AppEvent::KillAgent))
        }
        // Ctrl-r: kill + relaunch in one action (issue #117). Placed BEFORE
        // the plain `l`/`L` arm so the CONTROL modifier is checked first, and
        // before `handle_direct_pane_focus_key` (which handles plain `r`).
        KeyCode::Char('r' | 'R') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
            KeyHandling::Handled(
                snapshot
                    .selected_agent_id
                    .clone()
                    .map(AppEvent::RestartAgent),
            )
        }
        KeyCode::Char('l' | 'L') => KeyHandling::Handled(relaunch_event_for_selected_agent(
            snapshot.selected_agent_id.clone(),
            snapshot.selected_agent_is_running,
        )),
        _ => KeyHandling::Unhandled,
    }
}

fn delete_event(snapshot: &NormalKeySnapshot) -> Option<AppEvent> {
    match snapshot.pane_focus {
        PaneFocus::Agents | PaneFocus::Terminal => snapshot
            .selected_agent_id
            .clone()
            .map(AppEvent::OpenDeleteAgent),
        PaneFocus::Repositories => snapshot
            .selected_repo_id
            .clone()
            .map(AppEvent::OpenDeleteRepository),
    }
}

pub(super) fn resolve_mode_key(key_event: &KeyEvent, screen_mode: ScreenMode) -> KeyHandling {
    match key_event.code {
        KeyCode::Char('i' | 'I') if screen_mode == ScreenMode::Dashboard => {
            KeyHandling::Handled(Some(AppEvent::EnterIssuesMode))
        }
        // PR-mode entry: 'p'/'P' from Dashboard enters PR Mode.
        // @plan PLAN-20260624-PR-MODE.P09
        // @requirement REQ-PR-001
        // @pseudocode component-003 lines 01-09
        KeyCode::Char('p' | 'P') if screen_mode == ScreenMode::Dashboard => {
            KeyHandling::Handled(Some(AppEvent::EnterPrsMode))
        }
        KeyCode::Char('g' | 'G') if screen_mode == ScreenMode::Dashboard => {
            KeyHandling::Handled(Some(AppEvent::EnterActionsMode))
        }
        KeyCode::Char('e' | 'E') if screen_mode == ScreenMode::Dashboard => {
            KeyHandling::Handled(Some(AppEvent::EnterErrorsMode))
        }
        KeyCode::Char('s' | 'S') if screen_mode == ScreenMode::Dashboard => {
            KeyHandling::Handled(Some(AppEvent::EnterSplitMode))
        }
        KeyCode::F(7) if screen_mode == ScreenMode::Dashboard => {
            KeyHandling::Handled(Some(AppEvent::EnterTerminalManagerMode))
        }
        KeyCode::Esc if screen_mode == ScreenMode::Split => {
            KeyHandling::Handled(Some(AppEvent::ExitSplitMode))
        }
        KeyCode::Char('g' | 'G') if screen_mode == ScreenMode::Split => {
            KeyHandling::Handled(Some(AppEvent::EnterGrabMode))
        }
        _ => KeyHandling::Unhandled,
    }
}

fn resolve_help_search_key(key_event: &KeyEvent) -> KeyHandling {
    match key_event.code {
        KeyCode::Char('?' | 'h' | 'H') | KeyCode::F(1) => {
            KeyHandling::Handled(Some(AppEvent::OpenHelp))
        }
        KeyCode::Char('/') => KeyHandling::Handled(Some(AppEvent::OpenSearch)),
        _ => KeyHandling::Unhandled,
    }
}

fn resolve_visibility_key(key_event: &KeyEvent, screen_mode: ScreenMode) -> KeyHandling {
    match key_event.code {
        KeyCode::Char('v' | 'V') if screen_mode == ScreenMode::Dashboard => {
            KeyHandling::Handled(Some(AppEvent::ToggleHideIdleRepositories))
        }
        _ => KeyHandling::Unhandled,
    }
}

fn handle_direct_pane_focus_key(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    key_event: &KeyEvent,
) -> KeyHandling {
    match key_event.code {
        KeyCode::Char('r' | 'R') => {
            set_pane_focus(app_state, ctx, PaneFocus::Repositories);
            KeyHandling::Handled(None)
        }
        KeyCode::Char('a' | 'A') => {
            set_pane_focus(app_state, ctx, PaneFocus::Agents);
            KeyHandling::Handled(None)
        }
        KeyCode::Char('t' | 'T') => {
            focus_terminal_pane(app_state, ctx);
            KeyHandling::Handled(None)
        }
        _ => KeyHandling::Unhandled,
    }
}

fn set_pane_focus(app_state: &mut AppStateHandle, ctx: &SharedContext, pane_focus: PaneFocus) {
    let mut state = app_state.write();
    state.pane_focus = pane_focus;
    state.dashboard_grab = None;
    let persisted = to_persisted_state(&state);
    drop(state);
    persist_state(ctx, &persisted);
}

fn focus_terminal_pane(app_state: &mut AppStateHandle, ctx: &SharedContext) {
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
            *state = std::mem::take(&mut *state).apply(AppEvent::ToggleTerminalFocus);
        }
    } else {
        state.pane_focus = PaneFocus::Agents;
        state.dashboard_grab = None;
        state.terminal_focused = false;
    }

    running_agent_id
}

fn attach_terminal_focus(app_state: &mut AppStateHandle, ctx: &SharedContext, agent_id: &AgentId) {
    if let Some(ctx_arc) = &ctx
        && let Ok(mut ctx_guard) = ctx_arc.lock()
        && let Err(e) = ctx_guard.runtime.attach(agent_id)
    {
        warn!(
            agent_id = %agent_id.0,
            error = %e,
            "could not attach session on 't' focus"
        );
        set_pane_focus(app_state, ctx, PaneFocus::Agents);
    }
}

fn resolve_enter_key(key_event: &KeyEvent, snapshot: &NormalKeySnapshot) -> KeyHandling {
    if key_event.code != KeyCode::Enter {
        return KeyHandling::Unhandled;
    }

    let event = match snapshot.pane_focus {
        PaneFocus::Agents => snapshot
            .selected_agent_id
            .clone()
            .map(AppEvent::OpenEditAgent),
        PaneFocus::Repositories => snapshot
            .selected_repo_id
            .clone()
            .map(AppEvent::OpenEditRepository),
        PaneFocus::Terminal => Some(AppEvent::ToggleTerminalFocus),
    };
    KeyHandling::Handled(event)
}

fn handle_theme_key(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    key_event: &KeyEvent,
    screen_mode: ScreenMode,
) -> KeyHandling {
    // F9 opens the theme picker in Dashboard mode only.
    if key_event.code != KeyCode::F(9) || screen_mode != ScreenMode::Dashboard {
        return KeyHandling::Unhandled;
    }

    let event = if let Some(ctx_arc) = &ctx
        && let Ok(ctx_guard) = ctx_arc.lock()
    {
        let available = ctx_guard.theme_manager.themes_with_names();
        let active = ctx_guard.theme_manager.active_theme().slug.clone();
        AppEvent::OpenThemePicker {
            available_themes: available,
            active_slug: active,
        }
    } else {
        return KeyHandling::Unhandled;
    };

    // apply_and_persist internally locks ctx, so the guard above must be dropped.
    super::apply_and_persist(app_state, ctx, event);
    KeyHandling::Handled(None)
}

#[cfg(test)]
mod tests {
    use super::{
        KeyHandling, NormalKeySnapshot, issues_quit_shortcut_active, prs_quit_shortcut_active,
        quit_shortcut_active, relaunch_event_for_selected_agent, resolve_agent_lifecycle_key,
    };
    use iocraft::prelude::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
    use jefe::domain::AgentId;
    use jefe::input::InputMode;
    use jefe::input::input_mode_for_state;
    use jefe::state::{
        AgentChooserState, AppEvent, AppState, ComposerTarget, InlineState, IssueFocus,
        IssuesState, PaneFocus, PrFocus, PullRequestsState, ScreenMode,
    };

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

    // ─── State construction helpers (mirror issues.rs patterns) ─────────────

    fn issues_base_state() -> AppState {
        AppState {
            screen_mode: ScreenMode::DashboardIssues,
            issues_state: IssuesState {
                active: true,
                issue_focus: IssueFocus::IssueList,
                ..IssuesState::default()
            },
            ..AppState::default()
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // issues_quit_shortcut_active predicate (RED → GREEN)
    // ═══════════════════════════════════════════════════════════════════════

    /// The quit shortcut is eligible in the plain `IssuesNormal` sub-mode.
    #[test]
    fn quit_shortcut_active_in_issues_normal_submode() {
        let state = issues_base_state();
        assert!(matches!(
            input_mode_for_state(&state),
            InputMode::IssuesNormal
        ));
        assert!(issues_quit_shortcut_active(&state));
    }

    /// Quit shortcut must NOT act when filter controls overlay is open.
    #[test]
    fn quit_shortcut_inactive_when_filter_controls_open() {
        let mut state = issues_base_state();
        state.issues_state.filter_ui.controls_open = true;
        assert!(matches!(
            input_mode_for_state(&state),
            InputMode::IssuesFilter
        ));
        assert!(!issues_quit_shortcut_active(&state));
    }

    /// Quit shortcut must NOT act when search input is focused.
    #[test]
    fn quit_shortcut_inactive_when_search_input_focused() {
        let mut state = issues_base_state();
        state.issues_state.search_input_focused = true;
        assert!(matches!(
            input_mode_for_state(&state),
            InputMode::IssuesSearch
        ));
        assert!(!issues_quit_shortcut_active(&state));
    }

    /// Quit shortcut must NOT act when inline composer/editor is active.
    #[test]
    fn quit_shortcut_inactive_when_inline_composer_active() {
        let mut state = issues_base_state();
        state.issues_state.inline_state = InlineState::Composer {
            target: ComposerTarget::NewComment,
            text: String::new(),
            cursor: 0,
        };
        assert!(matches!(
            input_mode_for_state(&state),
            InputMode::IssuesInline
        ));
        assert!(!issues_quit_shortcut_active(&state));
    }

    /// The quit shortcut must NOT act while the agent chooser overlay is open.
    #[test]
    fn quit_shortcut_inactive_when_agent_chooser_open() {
        let mut state = issues_base_state();
        state.issues_state.agent_chooser = Some(AgentChooserState {
            selected_index: 0,
            agents: vec![jefe::domain::AgentChooserEntry::new(
                AgentId(String::from("a1")),
                String::from("Agent 1"),
                jefe::domain::AgentKind::Llxprt,
                jefe::domain::ChooserRuntimeConfig::default(),
            )],
            transient_available: false,
        });
        assert!(matches!(
            input_mode_for_state(&state),
            InputMode::IssuesChooser
        ));
        assert!(!issues_quit_shortcut_active(&state));
    }

    /// Issues predicate is false for plain Dashboard state.
    #[test]
    fn issues_predicate_false_for_non_issues_dashboard_state() {
        let state = AppState {
            screen_mode: ScreenMode::Dashboard,
            ..AppState::default()
        };
        assert!(matches!(input_mode_for_state(&state), InputMode::Normal));
        assert!(!issues_quit_shortcut_active(&state));
    }

    fn prs_base_state() -> AppState {
        AppState {
            screen_mode: ScreenMode::DashboardPullRequests,
            prs_state: PullRequestsState {
                active: true,
                pr_focus: PrFocus::PrList,
                ..PullRequestsState::default()
            },
            ..AppState::default()
        }
    }

    /// The quit shortcut should act while in PR Mode under plain `PrsNormal` sub-mode.
    #[test]
    fn prs_quit_shortcut_active_in_prs_normal_submode() {
        let state = prs_base_state();
        assert!(matches!(input_mode_for_state(&state), InputMode::PrsNormal));
        assert!(prs_quit_shortcut_active(&state));
    }

    /// The quit shortcut must NOT act when the PR filter controls overlay is open.
    #[test]
    fn prs_quit_shortcut_inactive_when_filter_controls_open() {
        let mut state = prs_base_state();
        state.prs_state.filter_ui.controls_open = true;
        assert!(matches!(input_mode_for_state(&state), InputMode::PrsFilter));
        assert!(!prs_quit_shortcut_active(&state));
    }

    /// The quit shortcut must NOT act when the PR search input is focused.
    #[test]
    fn prs_quit_shortcut_inactive_when_search_input_focused() {
        let mut state = prs_base_state();
        state.prs_state.search_input_focused = true;
        assert!(matches!(input_mode_for_state(&state), InputMode::PrsSearch));
        assert!(!prs_quit_shortcut_active(&state));
    }

    /// The quit shortcut must NOT act when a PR inline composer/editor is open.
    #[test]
    fn prs_quit_shortcut_inactive_when_inline_composer_active() {
        let mut state = prs_base_state();
        state.prs_state.inline_state = InlineState::Composer {
            target: ComposerTarget::NewComment,
            text: String::new(),
            cursor: 0,
        };
        assert!(matches!(input_mode_for_state(&state), InputMode::PrsInline));
        assert!(!prs_quit_shortcut_active(&state));
    }

    /// The quit shortcut must NOT act while the PR agent chooser overlay is open.
    #[test]
    fn prs_quit_shortcut_inactive_when_agent_chooser_open() {
        let mut state = prs_base_state();
        state.prs_state.agent_chooser = Some(AgentChooserState {
            selected_index: 0,
            agents: vec![jefe::domain::AgentChooserEntry::new(
                AgentId(String::from("a1")),
                String::from("Agent 1"),
                jefe::domain::AgentKind::Llxprt,
                jefe::domain::ChooserRuntimeConfig::default(),
            )],
            transient_available: false,
        });
        assert!(matches!(
            input_mode_for_state(&state),
            InputMode::PrsChooser
        ));
        assert!(!prs_quit_shortcut_active(&state));
    }

    // ── quit_shortcut_active(screen_mode) routing ──────────────────────────

    #[test]
    fn quit_shortcut_active_on_dashboard_normal() {
        let state = AppState {
            screen_mode: ScreenMode::Dashboard,
            ..AppState::default()
        };
        assert!(quit_shortcut_active(&state, ScreenMode::Dashboard));
    }

    #[test]
    fn quit_shortcut_active_in_split_mode() {
        let state = AppState {
            screen_mode: ScreenMode::Split,
            ..AppState::default()
        };
        // Split mode has no text-capturing sub-modes and does not bind `q`, so
        // the quit shortcut must remain eligible (restores the pre-refactor
        // catch-all behavior where `q` quit from Split mode).
        assert!(quit_shortcut_active(&state, ScreenMode::Split));
    }

    #[test]
    fn quit_shortcut_routes_through_issues_predicate() {
        let normal = issues_base_state();
        assert!(quit_shortcut_active(&normal, ScreenMode::DashboardIssues));
        let mut searching = issues_base_state();
        searching.issues_state.search_input_focused = true;
        assert!(!quit_shortcut_active(
            &searching,
            ScreenMode::DashboardIssues
        ));
    }

    #[test]
    fn quit_shortcut_routes_through_prs_predicate() {
        let normal = prs_base_state();
        assert!(quit_shortcut_active(
            &normal,
            ScreenMode::DashboardPullRequests
        ));
        let mut filtering = prs_base_state();
        filtering.prs_state.filter_ui.controls_open = true;
        assert!(!quit_shortcut_active(
            &filtering,
            ScreenMode::DashboardPullRequests
        ));
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Ctrl-r restart-agent key handler (RED → GREEN)
    // ═══════════════════════════════════════════════════════════════════════

    fn key_press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(KeyEventKind::Press, code)
    }

    fn key_with_mods(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        let mut evt = KeyEvent::new(KeyEventKind::Press, code);
        evt.modifiers = modifiers;
        evt
    }

    fn snapshot_with_agent(agent_id: &str, running: bool) -> NormalKeySnapshot {
        NormalKeySnapshot {
            pane_focus: PaneFocus::Agents,
            selected_agent_is_running: running,
            selected_repo_id: None,
            selected_agent_id: Some(AgentId(agent_id.to_string())),
        }
    }

    /// Ctrl-r on a running agent should emit `RestartAgent` (issue #117).
    #[test]
    fn restart_event_is_emitted_for_running_agent() {
        let snapshot = snapshot_with_agent("a1", true);
        let key_event = key_with_mods(KeyCode::Char('r'), KeyModifiers::CONTROL);
        let handling = resolve_agent_lifecycle_key(&key_event, &snapshot);
        match handling {
            KeyHandling::Handled(Some(AppEvent::RestartAgent(AgentId(id)))) => {
                assert_eq!(id, "a1");
            }
            other => panic!("expected Handled(RestartAgent), got {other:?}"),
        }
    }

    /// Ctrl-r on a dead agent also emits RestartAgent.
    #[test]
    fn restart_event_is_emitted_for_dead_agent() {
        let snapshot = snapshot_with_agent("a2", false);
        let key_event = key_with_mods(KeyCode::Char('r'), KeyModifiers::CONTROL);
        let handling = resolve_agent_lifecycle_key(&key_event, &snapshot);
        match handling {
            KeyHandling::Handled(Some(AppEvent::RestartAgent(AgentId(id)))) => {
                assert_eq!(id, "a2");
            }
            other => panic!("expected Handled(RestartAgent), got {other:?}"),
        }
    }

    /// Plain `r` without CONTROL must NOT produce a restart event.
    #[test]
    fn plain_r_does_not_restart() {
        let snapshot = snapshot_with_agent("a1", true);
        let key_event = key_press(KeyCode::Char('r'));
        let handling = resolve_agent_lifecycle_key(&key_event, &snapshot);
        assert!(
            matches!(handling, KeyHandling::Unhandled),
            "plain r should not be handled by lifecycle resolver"
        );
    }

    /// Ctrl-R (uppercase / shift) should also restart — be lenient with case.
    #[test]
    fn ctrl_shift_r_also_restarts() {
        let snapshot = snapshot_with_agent("a3", true);
        let key_event = key_with_mods(KeyCode::Char('R'), KeyModifiers::CONTROL);
        let handling = resolve_agent_lifecycle_key(&key_event, &snapshot);
        assert!(matches!(
            handling,
            KeyHandling::Handled(Some(AppEvent::RestartAgent(_)))
        ));
    }

    /// Ctrl-r with no selected agent should produce `Handled(None)` — the key
    /// is consumed (handled) but there's nothing to restart.
    #[test]
    fn ctrl_r_with_no_selection_is_handled_without_event() {
        let snapshot = NormalKeySnapshot {
            pane_focus: PaneFocus::Agents,
            selected_agent_is_running: false,
            selected_repo_id: None,
            selected_agent_id: None,
        };
        let key_event = key_with_mods(KeyCode::Char('r'), KeyModifiers::CONTROL);
        let handling = resolve_agent_lifecycle_key(&key_event, &snapshot);
        assert!(matches!(handling, KeyHandling::Handled(None)));
    }
}
