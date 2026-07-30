//! Normal-mode keyboard event dispatch.

use std::time::Instant;

use iocraft::prelude::*;
use tracing::warn;

use jefe::domain::{AgentId, RepositoryId};
use jefe::input::{InputMode, QuitOutcome, input_mode_for_state, observe_quit_sequence};
use jefe::runtime::RuntimeManager;
use jefe::state::{AppEvent, AppState, PaneFocus, ScreenMode};

use super::{
    AppStateHandle, QuitHandle, SharedContext, durable_save_request, schedule_durable_save,
};

#[derive(Debug)]
pub(super) enum KeyHandling {
    Unhandled,
    Handled(Option<AppEvent>),
}

pub fn handle_normal_key_event(
    app_state: &mut AppStateHandle,
    should_quit: &mut QuitHandle,
    ctx: &SharedContext,
    key_event: &KeyEvent,
    screen_mode: ScreenMode,
) -> Option<AppEvent> {
    if let Some(event) =
        super::dashboard_search::resolve_dashboard_search_focus(app_state, key_event, screen_mode)
    {
        return Some(event);
    }
    if let KeyHandling::Handled(event) =
        resolve_quit(app_state, should_quit, key_event, screen_mode)
    {
        return event;
    }
    if let KeyHandling::Handled(event) = super::dashboard_search::resolve_dashboard_mode_entry(
        app_state,
        ctx,
        key_event,
        screen_mode,
    ) {
        return event;
    }
    None
}

/// Whether rapid `qqq` remains eligible in a later-slice screen.
fn quit_shortcut_active(state: &AppState, screen_mode: ScreenMode) -> bool {
    match screen_mode {
        ScreenMode::DashboardIssues => issues_quit_shortcut_active(state),
        ScreenMode::DashboardPullRequests => prs_quit_shortcut_active(state),
        ScreenMode::DashboardActions => actions_quit_shortcut_active(state),
        ScreenMode::Dashboard
        | ScreenMode::Split
        | ScreenMode::DashboardErrors
        | ScreenMode::DashboardTerminals => false,
    }
}

/// Preserve rapid `qqq` for later-slice normal modes after registry fallthrough.
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
    if observe_rapid_quit(app_state, should_quit, key_event) {
        KeyHandling::Handled(None)
    } else {
        KeyHandling::Unhandled
    }
}

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

/// Returns true when the global quit shortcut should act while in
/// Issues Mode. Quit only applies in the plain `IssuesNormal` sub-mode; any
/// text-capturing or overlay sub-mode must receive the key so it is
/// not swallowed by quit.
fn issues_quit_shortcut_active(state: &AppState) -> bool {
    matches!(input_mode_for_state(state), InputMode::IssuesNormal)
}

pub(super) fn handle_dashboard_issues_key(
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
pub(super) fn handle_dashboard_prs_key(
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

pub(super) fn handle_dashboard_actions_key(
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

pub(super) fn select_first_visible_repository(
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
        let persisted = durable_save_request(&mut state_mut);
        drop(state_mut);
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

#[cfg(test)]
mod tests {
    use super::{issues_quit_shortcut_active, prs_quit_shortcut_active, quit_shortcut_active};
    use jefe::domain::AgentId;
    use jefe::input::{InputMode, input_mode_for_state};
    use jefe::state::{
        AgentChooserState, AppState, ComposerTarget, InlineState, IssueFocus, IssuesState, PrFocus,
        PullRequestsState, ScreenMode,
    };

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
                jefe::domain::shipped_agent_type(3),
                "LLxprt".to_owned(),
                "profile".to_owned(),
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
                jefe::domain::shipped_agent_type(3),
                "LLxprt".to_owned(),
                "profile".to_owned(),
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
    fn migrated_screens_do_not_reenter_legacy_quit_routing() {
        let dashboard = AppState::default();
        let split = AppState {
            screen_mode: ScreenMode::Split,
            ..AppState::default()
        };
        assert!(!quit_shortcut_active(&dashboard, ScreenMode::Dashboard));
        assert!(!quit_shortcut_active(&split, ScreenMode::Split));
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
}
