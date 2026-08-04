//! Closed S3 action-handler planning.
//!
//! One registry dispatch enters this exhaustive executor and becomes either the
//! smallest existing `AppEvent`, one named input/runtime boundary operation, or
//! an explicit later-slice result.

use jefe::domain::action_registry::HandlerKey;
use jefe::domain::keymap::{Chord, Key};
use jefe::list_viewport::PageItemCount;
use jefe::state::{AppEvent, AppState, ErrorsFocus, PaneFocus, ScreenId};

#[path = "action_handlers_s4.rs"]
mod s4;
#[derive(Debug)]
pub enum HandlerExecution {
    Event(AppEvent),
    Boundary(BoundaryAction),
    Noop,
    LaterSlice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundaryAction {
    Quit,
    JumpAgent(u8),
    ToggleTerminalFocus,
    ForwardToPty,
    HideShellOverlay,
    CloseShellOverlay,
    OpenEmbeddedShell,
    OpenExternalTerminal,
    NewAgentOrRepository,
    FocusRepositories,
    FocusAgents,
    FocusTerminal,
    OpenKeys,
    Settings(super::settings::SettingsAction),
    TerminalManagerCloseShell,
    TerminalManagerFocusShell,
    ConfirmAccept,
    AuthRetry,
    FormSubmit,
    FormSpace,
    HelpScrollUp,
    HelpScrollDown,
    HelpPageUp,
    HelpPageDown,
    HelpHome,
    HelpEnd,
}

use super::settings::SettingsAction;

const fn settings_boundary(action: SettingsAction) -> HandlerExecution {
    HandlerExecution::Boundary(BoundaryAction::Settings(action))
}

pub fn pre_mode_owned(
    handler: HandlerKey,
    state: &AppState,
    input_mode: jefe::input::InputMode,
) -> bool {
    match handler {
        HandlerKey::JumpAgent(_) => true,
        HandlerKey::ToggleTerminalFocus | HandlerKey::LeaveTerminal => matches!(
            state.screen(),
            ScreenId::Dashboard | ScreenId::Repositories | ScreenId::Actions
        ),
        HandlerKey::OpenEmbeddedShell | HandlerKey::OpenExternalTerminal => {
            input_mode == jefe::input::InputMode::Normal
                && state.screen() == ScreenId::Dashboard
                && !state.terminal_focused
        }
        HandlerKey::EmergencyExit => matches!(
            input_mode,
            jefe::input::InputMode::Normal
                | jefe::input::InputMode::DashboardSearch
                | jefe::input::InputMode::IssuesNormal
                | jefe::input::InputMode::PrsNormal
                | jefe::input::InputMode::ActionsNormal
        ),
        _ => false,
    }
}

pub fn apply_execution(
    execution: HandlerExecution,
    app_state: &mut super::AppStateHandle,
    should_quit: &mut super::QuitHandle,
    ctx: &super::SharedContext,
    suppress_next_enter: &mut iocraft::hooks::State<crate::pty_encoding::PasteEnterSuppression>,
    key_event: &iocraft::prelude::KeyEvent,
) -> bool {
    let handled = match execution {
        HandlerExecution::Event(event) => {
            dispatch_event(app_state, ctx, event);
            true
        }
        HandlerExecution::Boundary(boundary) => {
            apply_boundary(
                boundary,
                app_state,
                should_quit,
                ctx,
                suppress_next_enter,
                key_event,
            );
            true
        }
        HandlerExecution::Noop => true,
        HandlerExecution::LaterSlice => false,
    };
    if handled {
        super::refresh_action_availability(app_state);
    }
    handled
}

fn dispatch_event(
    app_state: &mut super::AppStateHandle,
    ctx: &super::SharedContext,
    event: AppEvent,
) {
    if matches!(
        event,
        AppEvent::TerminalScrollPageUp
            | AppEvent::TerminalScrollPageDown
            | AppEvent::TerminalScrollToTop
            | AppEvent::TerminalFollowTail
            | AppEvent::TerminalScrollUp
            | AppEvent::TerminalScrollDown
    ) {
        super::dispatch_terminal_scroll(app_state, ctx, event);
    } else {
        super::dispatch_app_event(app_state, ctx, event);
    }
}

fn apply_boundary(
    boundary: BoundaryAction,
    app_state: &mut super::AppStateHandle,
    should_quit: &mut super::QuitHandle,
    ctx: &super::SharedContext,
    suppress_next_enter: &mut iocraft::hooks::State<crate::pty_encoding::PasteEnterSuppression>,
    key_event: &iocraft::prelude::KeyEvent,
) {
    match boundary {
        BoundaryAction::Quit => {
            app_state.write().quit_sequence = jefe::state::QuitSequenceState::default();
            should_quit.set(true);
        }
        BoundaryAction::JumpAgent(slot) => {
            let _ = super::jump_to_shortcut_agent(app_state, ctx, slot);
        }
        BoundaryAction::ToggleTerminalFocus => super::handle_f12_toggle(app_state, ctx),
        BoundaryAction::ForwardToPty => {
            super::forward_key_to_pty(ctx.as_ref(), suppress_next_enter, key_event);
        }
        BoundaryAction::HideShellOverlay => {
            super::shell_overlay::hide_shell_overlay(app_state, ctx);
        }
        BoundaryAction::CloseShellOverlay => {
            super::shell_overlay::close_shell_overlay(app_state, ctx);
        }
        BoundaryAction::OpenEmbeddedShell => {
            super::shell_overlay::open_embedded_shell(app_state, ctx);
        }
        BoundaryAction::OpenExternalTerminal => {
            super::shell_overlay::open_external_terminal(app_state, ctx);
        }
        BoundaryAction::NewAgentOrRepository => new_agent_or_repository(app_state, ctx),
        BoundaryAction::FocusRepositories => {
            super::normal::set_pane_focus(app_state, ctx, PaneFocus::Repositories);
        }
        BoundaryAction::FocusAgents => {
            super::normal::set_pane_focus(app_state, ctx, PaneFocus::Agents);
        }
        BoundaryAction::FocusTerminal => super::normal::focus_terminal_pane(app_state, ctx),
        BoundaryAction::OpenKeys => super::keys_editor::open(app_state, ctx),
        BoundaryAction::Settings(action) => super::settings::apply(action, app_state, ctx),
        BoundaryAction::TerminalManagerCloseShell | BoundaryAction::TerminalManagerFocusShell => {
            apply_terminal_manager_boundary(boundary, app_state, ctx);
        }
        BoundaryAction::ConfirmAccept
        | BoundaryAction::AuthRetry
        | BoundaryAction::FormSubmit
        | BoundaryAction::FormSpace => {
            s4::apply_modal_boundary(boundary, app_state, ctx);
        }
        BoundaryAction::HelpScrollUp
        | BoundaryAction::HelpScrollDown
        | BoundaryAction::HelpPageUp
        | BoundaryAction::HelpPageDown
        | BoundaryAction::HelpHome
        | BoundaryAction::HelpEnd => apply_help_scroll(boundary, app_state),
    }
}

fn apply_terminal_manager_boundary(
    boundary: BoundaryAction,
    app_state: &mut super::AppStateHandle,
    ctx: &super::SharedContext,
) {
    let event = match boundary {
        BoundaryAction::TerminalManagerCloseShell => {
            super::terminal_manager::close_selected_shell(app_state)
        }
        BoundaryAction::TerminalManagerFocusShell => {
            super::terminal_manager::focus_selected_shell(app_state, ctx)
        }
        _ => None,
    };
    if let Some(event) = event {
        super::dispatch_app_event(app_state, ctx, event);
    }
}

fn apply_help_scroll(boundary: BoundaryAction, app_state: &mut super::AppStateHandle) {
    let (_, terminal_rows) = crossterm::terminal::size().unwrap_or((120, 40));
    let viewport_rows = jefe::ui::modals::help_viewport_rows(terminal_rows);
    let mut state = app_state.write();
    let content_rows = state
        .action_registry_snapshot
        .as_ref()
        .map_or(0, |snapshot| {
            jefe::ui::modals::help_content_lines(snapshot).len()
        });
    let max_scroll = content_rows.saturating_sub(viewport_rows);
    state.help_scroll_offset = match boundary {
        BoundaryAction::HelpScrollUp => state.help_scroll_offset.saturating_sub(1),
        BoundaryAction::HelpScrollDown => state.help_scroll_offset.saturating_add(1),
        BoundaryAction::HelpPageUp => state.help_scroll_offset.saturating_sub(8),
        BoundaryAction::HelpPageDown => state.help_scroll_offset.saturating_add(8),
        BoundaryAction::HelpHome => 0,
        BoundaryAction::HelpEnd => max_scroll,
        _ => state.help_scroll_offset,
    }
    .min(max_scroll);
}

fn new_agent_or_repository(app_state: &mut super::AppStateHandle, ctx: &super::SharedContext) {
    let selected = app_state
        .read()
        .selected_repository()
        .map(|repository| repository.id.clone());
    let selected =
        selected.or_else(|| super::normal::select_first_visible_repository(app_state, ctx));
    let event = selected.map_or(AppEvent::OpenNewRepository, AppEvent::OpenNewAgent);
    super::dispatch_app_event(app_state, ctx, event);
}

macro_rules! handler_execution {
    ($handler:expr, $chord:expr, $state:expr, $page_items:expr) => {{
        use BoundaryAction as B;
        use HandlerExecution as E;
        use HandlerKey as H;
        let handler = $handler;
        let chord = $chord;
        let state = $state;
        let page_items = $page_items;
        match handler {
            H::EmergencyExit => E::Boundary(B::Quit),
            H::OpenKeys => E::Boundary(B::OpenKeys),
            H::OpenSettings => settings_boundary(SettingsAction::Open),
            H::SettingsBack => settings_boundary(SettingsAction::Back),
            H::SettingsUp => settings_boundary(SettingsAction::Up),
            H::SettingsDown => settings_boundary(SettingsAction::Down),
            H::SettingsCyclePane => settings_boundary(SettingsAction::CyclePane),
            H::SettingsCyclePaneReverse => settings_boundary(SettingsAction::CyclePaneReverse),
            H::SettingsActivate => settings_boundary(SettingsAction::Activate),
            H::SettingsSelectPrevious => settings_boundary(SettingsAction::SelectPrevious),
            H::SettingsSelectNext => settings_boundary(SettingsAction::SelectNext),
            H::SettingsSave => settings_boundary(SettingsAction::Save),
            H::SettingsSaveAndExit => settings_boundary(SettingsAction::SaveAndExit),
            H::SettingsReset => settings_boundary(SettingsAction::Reset),
            H::JumpAgent(slot) => E::Boundary(B::JumpAgent(slot)),
            H::TerminalScrollPageUp
            | H::TerminalScrollPageDown
            | H::TerminalScrollTop
            | H::TerminalScrollTail
            | H::TerminalScrollUp
            | H::TerminalScrollDown => terminal_execution(handler, state),
            H::LeaveTerminal | H::ToggleTerminalFocus => E::Boundary(B::ToggleTerminalFocus),
            H::HideShellOverlay => E::Boundary(B::HideShellOverlay),
            H::CloseShellOverlay => E::Boundary(B::CloseShellOverlay),
            H::OpenEmbeddedShell => E::Boundary(B::OpenEmbeddedShell),
            H::OpenExternalTerminal => E::Boundary(B::OpenExternalTerminal),
            H::OpenHelp => E::Event(AppEvent::OpenHelp),
            H::NavigateUp => E::Event(AppEvent::NavigateUp),
            H::NavigateDown => E::Event(AppEvent::NavigateDown),
            H::NavigatePageUp => E::Event(AppEvent::NavigatePageUp(page_items)),
            H::NavigatePageDown => E::Event(AppEvent::NavigatePageDown(page_items)),
            H::NavigateHome if state.screen() == ScreenId::Errors => {
                E::Event(AppEvent::ErrorsNavigateHome)
            }
            H::NavigateEnd if state.screen() == ScreenId::Errors => {
                E::Event(AppEvent::ErrorsNavigateEnd)
            }
            H::NavigateHome => E::Event(AppEvent::NavigateHome),
            H::NavigateEnd => E::Event(AppEvent::NavigateEnd),
            H::NavigateLeft => E::Event(AppEvent::NavigateLeft),
            H::NavigateRight => E::Event(AppEvent::NavigateRight),
            H::CyclePaneFocus => E::Event(AppEvent::CyclePaneFocus),
            H::NewAgentOrRepository => E::Boundary(B::NewAgentOrRepository),
            H::OpenNewRepository => E::Event(AppEvent::OpenNewRepository),
            H::OpenDeleteSelection => optional_event(delete_event(state)),
            H::KillSelectedAgent => {
                optional_event(selected_agent_event(state, AppEvent::KillAgent))
            }
            H::RestartSelectedAgent => {
                optional_event(selected_agent_event(state, AppEvent::RestartAgent))
            }
            H::RelaunchSelectedAgent => relaunch_execution(state),
            H::EnterIssues => E::Event(AppEvent::EnterIssuesMode),
            H::EnterPullRequests => E::Event(AppEvent::EnterPrsMode),
            H::EnterActions => E::Event(AppEvent::EnterActionsMode),
            H::EnterErrors => E::Event(AppEvent::EnterErrorsMode),
            H::EnterSplit => E::Event(AppEvent::EnterSplitMode),
            H::EnterTerminalManager => E::Event(AppEvent::EnterTerminalManagerMode),
            H::FocusDashboardSearch if state.screen() == ScreenId::Dashboard => {
                E::Event(AppEvent::FocusDashboardSearch)
            }
            H::FocusDashboardSearch => E::Event(AppEvent::OpenSearch),
            H::ToggleHiddenRepositories => E::Event(AppEvent::ToggleHideIdleRepositories),
            H::FocusRepositories => E::Boundary(B::FocusRepositories),
            H::FocusAgents => E::Boundary(B::FocusAgents),
            H::FocusTerminal => E::Boundary(B::FocusTerminal),
            H::ActivateDashboardSelection => activate_execution(state),
            H::DashboardGrabStart => E::Event(AppEvent::EnterDashboardGrab),
            H::DashboardGrabDrop => E::Event(AppEvent::ExitDashboardGrab),
            H::DashboardGrabUp => E::Event(AppEvent::DashboardGrabMoveUp),
            H::DashboardGrabDown => E::Event(AppEvent::DashboardGrabMoveDown),
            H::ExitSplit => E::Event(AppEvent::ExitSplitMode),
            H::EnterSplitGrab => E::Event(AppEvent::EnterGrabMode),
            H::WorkbenchToggleFilter => E::Event(AppEvent::ToggleWorkbenchStatusBucket(
                state.workbench_filter_cursor_bucket(),
            )),
            H::WorkbenchFilterPrev => E::Event(AppEvent::WorkbenchFilterCursorPrev),
            H::WorkbenchFilterNext => E::Event(AppEvent::WorkbenchFilterCursorNext),
            H::WorkbenchPrevPage => E::Event(AppEvent::WorkbenchPrevPage),
            H::WorkbenchNextPage => E::Event(AppEvent::WorkbenchNextPage),
            H::WorkbenchSelectPrev => E::Event(AppEvent::WorkbenchSelectPrev),
            H::WorkbenchSelectNext => E::Event(AppEvent::WorkbenchSelectNext),
            H::WorkbenchAttach => E::Event(AppEvent::WorkbenchAttach),
            H::ErrorsBack => errors_back(state),
            H::ErrorsUp => errors_vertical(state, chord, true),
            H::ErrorsDown => errors_vertical(state, chord, false),
            H::ErrorsPageUp => E::Event(AppEvent::ErrorsScrollDetailPageUp),
            H::ErrorsPageDown => E::Event(AppEvent::ErrorsScrollDetailPageDown),
            H::ErrorsActivate => errors_activate(state),
            H::ErrorsCyclePane => errors_cycle(chord),
            H::ErrorsClear => E::Event(AppEvent::ErrorsClearAll),
            H::TerminalManagerBack => E::Event(AppEvent::ExitTerminalManagerMode),
            H::TerminalManagerUp => E::Event(AppEvent::TerminalManagerNavigateUp),
            H::TerminalManagerDown => E::Event(AppEvent::TerminalManagerNavigateDown),
            H::TerminalManagerHome => E::Event(AppEvent::TerminalManagerNavigateHome),
            H::TerminalManagerEnd => E::Event(AppEvent::TerminalManagerNavigateEnd),
            H::TerminalManagerCloseShell => E::Boundary(B::TerminalManagerCloseShell),
            H::TerminalManagerFocusShell => E::Boundary(B::TerminalManagerFocusShell),
            H::HelpClose
            | H::HelpScrollUp
            | H::HelpScrollDown
            | H::HelpPageUp
            | H::HelpPageDown
            | H::HelpHome
            | H::HelpEnd
            | H::ConfirmCancel
            | H::ConfirmCycleFocus
            | H::ConfirmAccept
            | H::ConfirmToggleDeleteWorkDir
            | H::AuthCancel
            | H::AuthRetry
            | H::FormCancel
            | H::FormSubmit
            | H::FormNextField
            | H::FormPreviousField
            | H::SearchApply
            | H::SearchCancel
            | H::SearchBackspace
            | H::FilterApply
            | H::FilterCancel
            | H::FilterNextField
            | H::FilterPreviousField
            | H::FilterClearCurrent
            | H::FilterClearAll
            | H::FilterPreviousChoice
            | H::FilterNextChoice
            | H::IssuesExit
            | H::IssuesBack
            | H::IssuesOpen
            | H::IssuesNew
            | H::IssuesOpenFilter
            | H::IssuesFocusSearch
            | H::IssuesEdit
            | H::IssuesComment
            | H::IssuesReply
            | H::IssuesSendToAgent
            | H::IssuesCyclePane
            | H::IssuesSubmitInline
            | H::IssuesCancelInline
            | H::IssuesChooserPrevious
            | H::IssuesChooserNext
            | H::IssuesChooserConfirm
            | H::IssuesChooserCancel
            | H::PullRequestsExit
            | H::PullRequestsBack
            | H::PullRequestsOpen
            | H::PullRequestsOpenFilter
            | H::PullRequestsComment
            | H::PullRequestsReply
            | H::PullRequestsResolveThread
            | H::PullRequestsEdit
            | H::PullRequestsSendToAgent
            | H::PullRequestsOpenBrowser
            | H::PullRequestsOpenMerge
            | H::PullRequestsCyclePane
            | H::PullRequestsSubmitInline
            | H::PullRequestsCancelInline
            | H::PullRequestsChooserPrevious
            | H::PullRequestsChooserNext
            | H::PullRequestsChooserConfirm
            | H::PullRequestsChooserCancel
            | H::ActionsExit
            | H::ActionsReload
            | H::ActionsOpenFilter
            | H::ActionsFocusSearch
            | H::ActionsUp
            | H::ActionsDown
            | H::ActionsPageUp
            | H::ActionsPageDown
            | H::ActionsActivate
            | H::ActionsBack => E::LaterSlice,
        }
    }};
}

#[must_use]
pub fn execution_for(
    handler: HandlerKey,
    chord: Chord,
    state: &AppState,
    page_items: PageItemCount,
) -> HandlerExecution {
    if let Some(execution) = s4::execution_for(handler, chord, state, page_items) {
        return execution;
    }
    handler_execution!(handler, chord, state, page_items)
}

#[cfg(test)]
pub(super) fn event_for_test(
    handler: HandlerKey,
    chord: Chord,
    state: &AppState,
    page_items: PageItemCount,
) -> Option<AppEvent> {
    match execution_for(handler, chord, state, page_items) {
        HandlerExecution::Event(event) => Some(event),
        HandlerExecution::Boundary(_) | HandlerExecution::Noop | HandlerExecution::LaterSlice => {
            None
        }
    }
}

fn terminal_execution(handler: HandlerKey, state: &AppState) -> HandlerExecution {
    use HandlerExecution::{Boundary, Event};
    if !state.is_kennel_mode() {
        return Boundary(BoundaryAction::ForwardToPty);
    }
    match handler {
        HandlerKey::TerminalScrollPageUp => Event(AppEvent::TerminalScrollPageUp),
        HandlerKey::TerminalScrollPageDown => Event(AppEvent::TerminalScrollPageDown),
        HandlerKey::TerminalScrollTop => Event(AppEvent::TerminalScrollToTop),
        HandlerKey::TerminalScrollTail if state.terminal_history_offset.is_some() => {
            Event(AppEvent::TerminalFollowTail)
        }
        HandlerKey::TerminalScrollUp if state.terminal_history_offset.is_some() => {
            Event(AppEvent::TerminalScrollUp)
        }
        HandlerKey::TerminalScrollDown if state.terminal_history_offset.is_some() => {
            Event(AppEvent::TerminalScrollDown)
        }
        _ => Boundary(BoundaryAction::ForwardToPty),
    }
}

fn errors_back(state: &AppState) -> HandlerExecution {
    if state.errors_state.focus == ErrorsFocus::ErrorDetail {
        HandlerExecution::Event(AppEvent::RefocusErrorList)
    } else {
        HandlerExecution::Event(AppEvent::ExitErrorsMode)
    }
}

fn errors_vertical(state: &AppState, chord: Chord, up: bool) -> HandlerExecution {
    if matches!(chord.key, Key::Char('j' | 'k')) {
        if state.errors_state.focus != ErrorsFocus::ErrorDetail {
            return HandlerExecution::Noop;
        }
        return HandlerExecution::Event(if up {
            AppEvent::ErrorsScrollDetailUp
        } else {
            AppEvent::ErrorsScrollDetailDown
        });
    }
    HandlerExecution::Event(if up {
        AppEvent::ErrorsNavigateUp
    } else {
        AppEvent::ErrorsNavigateDown
    })
}

fn errors_cycle(chord: Chord) -> HandlerExecution {
    HandlerExecution::Event(if matches!(chord.key, Key::Left | Key::BackTab) {
        AppEvent::ErrorsCycleFocusReverse
    } else {
        AppEvent::ErrorsCycleFocus
    })
}

fn errors_activate(state: &AppState) -> HandlerExecution {
    if state.errors_state.focus == ErrorsFocus::ErrorList {
        HandlerExecution::Event(AppEvent::ErrorsEnter)
    } else {
        HandlerExecution::Noop
    }
}

fn delete_event(state: &AppState) -> Option<AppEvent> {
    match state.pane_focus {
        PaneFocus::Repositories => state
            .selected_repository()
            .map(|repository| AppEvent::OpenDeleteRepository(repository.id.clone())),
        PaneFocus::Agents | PaneFocus::Terminal => state
            .selected_agent()
            .map(|agent| AppEvent::OpenDeleteAgent(agent.id.clone())),
    }
}

fn selected_agent_event(
    state: &AppState,
    constructor: fn(jefe::domain::AgentId) -> AppEvent,
) -> Option<AppEvent> {
    state
        .selected_agent()
        .map(|agent| constructor(agent.id.clone()))
}

fn relaunch_execution(state: &AppState) -> HandlerExecution {
    if state
        .selected_agent()
        .is_some_and(jefe::domain::Agent::is_running)
    {
        HandlerExecution::Noop
    } else {
        optional_event(selected_agent_event(state, AppEvent::RelaunchAgent))
    }
}

fn activate_execution(state: &AppState) -> HandlerExecution {
    if state.agents.is_empty()
        && let Some(observation) = state
            .agent_type_availability
            .get(state.selected_agent_type_index)
    {
        return HandlerExecution::Event(AppEvent::OpenAgentTypeForm(observation.type_id().clone()));
    }
    match state.pane_focus {
        PaneFocus::Repositories => optional_event(
            state
                .selected_repository()
                .map(|repository| AppEvent::OpenEditRepository(repository.id.clone())),
        ),
        PaneFocus::Agents => optional_event(
            state
                .selected_agent()
                .map(|agent| AppEvent::OpenEditAgent(agent.id.clone())),
        ),
        PaneFocus::Terminal => HandlerExecution::Boundary(BoundaryAction::ToggleTerminalFocus),
    }
}

fn optional_event(event: Option<AppEvent>) -> HandlerExecution {
    event.map_or(HandlerExecution::Noop, HandlerExecution::Event)
}
