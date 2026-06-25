//! Normal-mode keyboard event dispatch.

use iocraft::prelude::*;
use tracing::{debug, warn};

use jefe::domain::{AgentId, RepositoryId};
use jefe::input::{InputMode, input_mode_for_state};
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
    let snapshot = normal_key_snapshot(app_state);

    if let KeyHandling::Handled(event) =
        handle_dashboard_issues_key(app_state, should_quit, ctx, key_event, screen_mode)
    {
        return event;
    }
    // PR-mode delegation: must run BEFORE resolve_mode_key so that while
    // screen_mode == DashboardPullRequests, p/P is intercepted here
    // (-> handle_prs_mode_key) and never reaches resolve_mode_key (whose p/P
    // arm only fires for screen == Dashboard).
    // @plan PLAN-20260624-PR-MODE.P09
    // @requirement REQ-PR-001
    // @requirement REQ-PR-002
    // @pseudocode component-003 lines 10-14
    if let KeyHandling::Handled(event) =
        handle_dashboard_prs_key(app_state, should_quit, ctx, key_event, screen_mode)
    {
        return event;
    }
    if let KeyHandling::Handled(event) = resolve_quit_key(should_quit, key_event) {
        return event;
    }
    if let KeyHandling::Handled(event) = resolve_navigation_key(key_event) {
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
    if let KeyHandling::Handled(event) = handle_theme_key(ctx, key_event) {
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

/// Returns true when `q`/`Q` should act as the global quit shortcut while in
/// Issues Mode. Quit only applies in the plain `IssuesNormal` sub-mode; any
/// text-capturing or overlay sub-mode (inline editor/composer, search input,
/// filter controls, agent chooser) must receive the key so the character is
/// not swallowed by quit.
fn issues_quit_shortcut_active(state: &AppState) -> bool {
    matches!(input_mode_for_state(state), InputMode::IssuesNormal)
}

fn handle_dashboard_issues_key(
    app_state: &AppStateHandle,
    should_quit: &mut QuitHandle,
    ctx: &SharedContext,
    key_event: &KeyEvent,
    screen_mode: ScreenMode,
) -> KeyHandling {
    if screen_mode != ScreenMode::DashboardIssues {
        return KeyHandling::Unhandled;
    }

    let quit_active = {
        let state = app_state.read();
        issues_quit_shortcut_active(&state)
    };

    if quit_active && matches!(key_event.code, KeyCode::Char('q' | 'Q')) {
        should_quit.set(true);
        KeyHandling::Handled(None)
    } else {
        KeyHandling::Handled(super::issues::handle_issues_mode_key(
            app_state, ctx, key_event,
        ))
    }
}

/// Returns true when `q`/`Q` should act as the global quit shortcut while in
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
/// the key is `q`/`Q`, quit; otherwise delegate to `prs::handle_prs_mode_key`.
/// The entire result is wrapped in `KeyHandling::Handled(...)` so every key is
/// consumed while in PR Mode (never leaks to dashboard/destructive handlers).
///
/// @plan PLAN-20260624-PR-MODE.P09
/// @requirement REQ-PR-001
/// @requirement REQ-PR-002
/// @pseudocode component-003 lines 05-14
fn handle_dashboard_prs_key(
    app_state: &AppStateHandle,
    should_quit: &mut QuitHandle,
    ctx: &SharedContext,
    key_event: &KeyEvent,
    screen_mode: ScreenMode,
) -> KeyHandling {
    if screen_mode != ScreenMode::DashboardPullRequests {
        return KeyHandling::Unhandled;
    }

    let quit_active = {
        let state = app_state.read();
        prs_quit_shortcut_active(&state)
    };

    if quit_active && matches!(key_event.code, KeyCode::Char('q' | 'Q')) {
        should_quit.set(true);
        KeyHandling::Handled(None)
    } else {
        KeyHandling::Handled(super::prs::handle_prs_mode_key(app_state, ctx, key_event))
    }
}

fn resolve_quit_key(should_quit: &mut QuitHandle, key_event: &KeyEvent) -> KeyHandling {
    if matches!(key_event.code, KeyCode::Char('q' | 'Q')) {
        should_quit.set(true);
        KeyHandling::Handled(None)
    } else {
        KeyHandling::Unhandled
    }
}

fn resolve_navigation_key(key_event: &KeyEvent) -> KeyHandling {
    match key_event.code {
        KeyCode::Up => KeyHandling::Handled(Some(AppEvent::NavigateUp)),
        KeyCode::Down => KeyHandling::Handled(Some(AppEvent::NavigateDown)),
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
        KeyCode::Char('s' | 'S') if screen_mode == ScreenMode::Dashboard => {
            KeyHandling::Handled(Some(AppEvent::EnterSplitMode))
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
        if !state.terminal_focused {
            *state = std::mem::take(&mut *state).apply(AppEvent::ToggleTerminalFocus);
        }
    } else {
        state.pane_focus = PaneFocus::Agents;
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

fn handle_theme_key(ctx: &SharedContext, key_event: &KeyEvent) -> KeyHandling {
    let theme = match key_event.code {
        KeyCode::Char('1') => "green-screen",
        KeyCode::Char('2') => "dracula",
        KeyCode::Char('3') => "default-dark",
        _ => return KeyHandling::Unhandled,
    };

    if let Some(ctx_arc) = &ctx
        && let Ok(mut ctx_guard) = ctx_arc.lock()
    {
        let _ = ctx_guard.theme_manager.set_active(theme);
    }
    KeyHandling::Handled(None)
}

#[cfg(test)]
mod tests {
    use super::{issues_quit_shortcut_active, relaunch_event_for_selected_agent};
    use jefe::domain::AgentId;
    use jefe::input::InputMode;
    use jefe::input::input_mode_for_state;
    use jefe::state::{
        AgentChooserState, AppEvent, AppState, ComposerTarget, InlineState, IssueFocus,
        IssuesState, ScreenMode,
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

    /// `q`/`Q` quits in the plain `IssuesNormal` sub-mode.
    #[test]
    fn q_quits_in_issues_normal_submode() {
        let state = issues_base_state();
        assert!(matches!(
            input_mode_for_state(&state),
            InputMode::IssuesNormal
        ));
        assert!(issues_quit_shortcut_active(&state));
    }

    /// `q`/`Q` must NOT quit when the filter controls overlay is open — it
    /// types into the filter instead.
    #[test]
    fn q_does_not_quit_when_filter_controls_open() {
        let mut state = issues_base_state();
        state.issues_state.filter_ui.controls_open = true;
        assert!(matches!(
            input_mode_for_state(&state),
            InputMode::IssuesFilter
        ));
        assert!(!issues_quit_shortcut_active(&state));
    }

    /// `q`/`Q` must NOT quit when the search input is focused — it types
    /// into the search query instead.
    #[test]
    fn q_does_not_quit_when_search_input_focused() {
        let mut state = issues_base_state();
        state.issues_state.search_input_focused = true;
        assert!(matches!(
            input_mode_for_state(&state),
            InputMode::IssuesSearch
        ));
        assert!(!issues_quit_shortcut_active(&state));
    }

    /// `q`/`Q` must NOT quit when an inline composer/editor is active — it
    /// types into the composer body instead.
    #[test]
    fn q_does_not_quit_when_inline_composer_active() {
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

    /// `q`/`Q` must NOT quit while the agent chooser overlay is open.
    #[test]
    fn q_does_not_quit_when_agent_chooser_open() {
        let mut state = issues_base_state();
        state.issues_state.agent_chooser = Some(AgentChooserState {
            selected_index: 0,
            agents: vec![(AgentId(String::from("a1")), String::from("Agent 1"))],
        });
        assert!(matches!(
            input_mode_for_state(&state),
            InputMode::IssuesChooser
        ));
        assert!(!issues_quit_shortcut_active(&state));
    }

    /// Sanity: for a non-issues `ScreenMode::Dashboard` state the predicate
    /// returns false, because `input_mode_for_state` would be `Normal`.
    #[test]
    fn q_quit_predicate_false_for_non_issues_dashboard_state() {
        let state = AppState {
            screen_mode: ScreenMode::Dashboard,
            ..AppState::default()
        };
        assert!(matches!(input_mode_for_state(&state), InputMode::Normal));
        assert!(!issues_quit_shortcut_active(&state));
    }
}
