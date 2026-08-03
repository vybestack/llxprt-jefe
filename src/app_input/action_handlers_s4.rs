//! Typed S4 action planning for workspace and special contexts.

use jefe::domain::action_registry::HandlerKey;
use jefe::domain::keymap::{Chord, Key};
use jefe::list_viewport::PageItemCount;
use jefe::state::{
    ActionsFilterField, ActionsFocus, AppEvent, AppState, DetailSubfocus, IssueFocus,
    IssuePropertyKind, NewIssueFormFocus, PrChangesFocus, PrDetailSubfocus, PrFocus,
    PrLifecycleEvent, PrPropertyKind, ReadOnlyHintKind, ScreenId,
};

use super::{BoundaryAction, HandlerExecution};

pub(super) fn execution_for(
    handler: HandlerKey,
    chord: Chord,
    state: &AppState,
    page: PageItemCount,
) -> Option<HandlerExecution> {
    if state.modal != jefe::state::ModalState::None {
        return modal_execution(handler, chord);
    }
    match state.screen {
        ScreenId::Issues => issues_execution(handler, chord, state, page),
        ScreenId::PullRequests => prs_execution(handler, chord, state, page),
        ScreenId::Actions => actions_execution(handler, chord, state, page),
        ScreenId::Dashboard if state.dashboard_search.input_focused => {
            dashboard_search_execution(handler, state)
        }
        _ => None,
    }
}

fn issues_execution(
    handler: HandlerKey,
    chord: Chord,
    state: &AppState,
    page: PageItemCount,
) -> Option<HandlerExecution> {
    use HandlerExecution::{Event, Noop};
    let event = match handler {
        HandlerKey::NavigateUp => Some(issue_vertical(state, true)),
        HandlerKey::NavigateDown => Some(issue_vertical(state, false)),
        HandlerKey::NavigatePageUp => Some(issue_page(state, true, page)),
        HandlerKey::NavigatePageDown => Some(issue_page(state, false, page)),
        HandlerKey::NavigateHome => Some(AppEvent::IssuesNavigateHome),
        HandlerKey::NavigateEnd => Some(AppEvent::IssuesNavigateEnd),
        HandlerKey::IssuesExit => Some(AppEvent::ExitIssuesMode),
        HandlerKey::IssuesBack => issue_back(state, chord),
        HandlerKey::IssuesOpen => Some(issue_open(state, chord)),
        HandlerKey::IssuesNew => Some(issue_new(state, chord)),
        HandlerKey::IssuesOpenFilter => Some(AppEvent::OpenFilterControls),
        HandlerKey::IssuesFocusSearch => Some(AppEvent::FocusSearchInput),
        HandlerKey::IssuesEdit => issue_edit(state, chord),
        HandlerKey::IssuesComment => Some(AppEvent::OpenNewCommentComposer),
        HandlerKey::IssuesReply => {
            super::super::issues::reply_event_for_subfocus(state.issues_state.detail_subfocus)
        }
        HandlerKey::IssuesSendToAgent => Some(AppEvent::OpenAgentChooser {
            metadata: super::super::build_chooser_metadata(state),
        }),
        HandlerKey::IssuesCyclePane => Some(if matches!(chord.key, Key::Left | Key::BackTab) {
            AppEvent::IssuesCycleFocusReverse
        } else {
            AppEvent::IssuesCycleFocus
        }),
        HandlerKey::IssuesSubmitInline => Some(issue_submit(state)),
        HandlerKey::IssuesCancelInline => Some(issue_cancel(state)),
        HandlerKey::IssuesChooserPrevious => issue_chooser(state, chord, true),
        HandlerKey::IssuesChooserNext => issue_chooser(state, chord, false),
        HandlerKey::IssuesChooserConfirm => issue_chooser_confirm(state),
        HandlerKey::IssuesChooserCancel => issue_chooser_cancel(state),
        HandlerKey::SearchApply => Some(AppEvent::ApplySearch),
        HandlerKey::SearchCancel => Some(if state.issues_state.search_query.is_empty() {
            AppEvent::BlurSearchInput
        } else {
            AppEvent::ClearSearch
        }),
        HandlerKey::FilterApply
        | HandlerKey::FilterCancel
        | HandlerKey::FilterNextField
        | HandlerKey::FilterPreviousField
        | HandlerKey::FilterClearCurrent
        | HandlerKey::FilterClearAll
        | HandlerKey::FilterPreviousChoice
        | HandlerKey::FilterNextChoice => {
            super::super::issues_filter::control_event(state, handler)
        }
        HandlerKey::FormNextField | HandlerKey::FormPreviousField => {
            new_issue_control(state, handler, chord)
        }
        _ => return None,
    };
    Some(event.map_or(Noop, Event))
}

fn issue_vertical(state: &AppState, up: bool) -> AppEvent {
    match state.issues_state.issue_focus {
        IssueFocus::IssueDetail if up => AppEvent::IssuesScrollDetailUp,
        IssueFocus::IssueDetail => AppEvent::IssuesScrollDetailDown,
        _ if up => AppEvent::IssuesNavigateUp,
        _ => AppEvent::IssuesNavigateDown,
    }
}

fn issue_page(state: &AppState, up: bool, page: PageItemCount) -> AppEvent {
    match (state.issues_state.issue_focus, up) {
        (IssueFocus::IssueDetail, true) => AppEvent::IssuesScrollDetailPageUp,
        (IssueFocus::IssueDetail, false) => AppEvent::IssuesScrollDetailPageDown,
        (_, true) => AppEvent::IssuesNavigatePageUp(page),
        (_, false) => AppEvent::IssuesNavigatePageDown(page),
    }
}

fn issue_back(state: &AppState, chord: Chord) -> Option<AppEvent> {
    match chord.key {
        Key::Char('p') => Some(AppEvent::EnterPrsMode),
        Key::Function(12) if state.terminal_focused => Some(AppEvent::ToggleTerminalFocus),
        Key::Function(12) | Key::Esc | Key::Char('i')
            if state.issues_state.issue_focus == IssueFocus::IssueDetail =>
        {
            Some(AppEvent::RefocusIssueList)
        }
        _ => None,
    }
}

fn issue_open(state: &AppState, chord: Chord) -> AppEvent {
    if state.issues_state.issue_focus == IssueFocus::IssueDetail
        && matches!(chord.key, Key::Tab | Key::Char('j'))
    {
        AppEvent::IssueDetailSubfocusNext
    } else {
        AppEvent::IssuesEnter
    }
}

fn issue_new(state: &AppState, chord: Chord) -> AppEvent {
    if state.issues_state.issue_focus == IssueFocus::IssueDetail
        && matches!(chord.key, Key::BackTab | Key::Char('k'))
    {
        AppEvent::IssueDetailSubfocusPrev
    } else {
        AppEvent::OpenNewIssueComposer
    }
}

fn issue_edit(state: &AppState, chord: Chord) -> Option<AppEvent> {
    match chord.key {
        Key::Char('e') => {
            super::super::issues::editor_event_for_subfocus(state.issues_state.detail_subfocus)
        }
        Key::Char('r') => Some(AppEvent::RequestIssueRewrite),
        Key::Char('C') => Some(AppEvent::OpenCloseReasonChooser),
        Key::Char('D') => Some(AppEvent::OpenDeleteIssueConfirm),
        Key::Char('L') => issue_property(state, IssuePropertyKind::Labels),
        Key::Char('A') => issue_property(state, IssuePropertyKind::Assignees),
        Key::Char('M') => issue_property(state, IssuePropertyKind::Milestone),
        Key::Char('T') => issue_property(state, IssuePropertyKind::Title),
        Key::Char('Y') => issue_property(state, IssuePropertyKind::Type),
        Key::Char('W') => issue_property(state, IssuePropertyKind::State),
        _ => None,
    }
}

fn issue_property(state: &AppState, kind: IssuePropertyKind) -> Option<AppEvent> {
    (state.issues_state.detail_subfocus == DetailSubfocus::Body)
        .then_some(AppEvent::IssueOpenPropertyEditor { kind })
}

fn issue_submit(state: &AppState) -> AppEvent {
    if state.issues_state.new_issue_form.is_some() {
        AppEvent::NewIssueSubmit
    } else {
        AppEvent::InlineSubmit
    }
}

fn issue_cancel(state: &AppState) -> AppEvent {
    if state.issues_state.new_issue_form.is_some() {
        AppEvent::NewIssueCancel
    } else {
        AppEvent::InlineCancelOrEsc
    }
}

fn issue_chooser(state: &AppState, chord: Chord, previous: bool) -> Option<AppEvent> {
    if state.issues_state.property_editor.is_some() {
        if matches!(chord.key, Key::Char(' ')) {
            return Some(AppEvent::IssuePropertyEditorToggle);
        }
        return Some(if previous {
            AppEvent::IssuePropertyEditorNavigateUp
        } else {
            AppEvent::IssuePropertyEditorNavigateDown
        });
    }
    if let Some(chooser) = &state.issues_state.close_reason_chooser {
        return Some(if chooser.duplicate_search.is_some() {
            if previous {
                AppEvent::CloseReasonDuplicateSearchNavigateUp
            } else {
                AppEvent::CloseReasonDuplicateSearchNavigateDown
            }
        } else if previous {
            AppEvent::CloseReasonNavigateUp
        } else {
            AppEvent::CloseReasonNavigateDown
        });
    }
    state.issues_state.agent_chooser.as_ref().map(|_| {
        if previous {
            AppEvent::AgentChooserNavigateUp
        } else {
            AppEvent::AgentChooserNavigateDown
        }
    })
}

fn issue_chooser_confirm(state: &AppState) -> Option<AppEvent> {
    if state.issues_state.property_editor.is_some() {
        Some(AppEvent::IssuePropertyEditorConfirm)
    } else if let Some(chooser) = &state.issues_state.close_reason_chooser {
        Some(
            if chooser.duplicate_search.is_some() || chooser.awaiting_confirmation {
                AppEvent::CloseReasonConfirm
            } else {
                AppEvent::CloseReasonSelect
            },
        )
    } else if state.issues_state.delete_confirm.is_some() {
        Some(AppEvent::IssueDeleteConfirm)
    } else {
        state
            .issues_state
            .agent_chooser
            .as_ref()
            .map(|_| AppEvent::AgentChooserConfirm)
    }
}

fn issue_chooser_cancel(state: &AppState) -> Option<AppEvent> {
    if state.issues_state.property_editor.is_some() {
        Some(AppEvent::IssuePropertyEditorCancel)
    } else if state.issues_state.close_reason_chooser.is_some() {
        Some(AppEvent::CloseReasonCancel)
    } else if state.issues_state.delete_confirm.is_some() {
        Some(AppEvent::IssueDeleteCancel)
    } else {
        state
            .issues_state
            .agent_chooser
            .as_ref()
            .map(|_| AppEvent::AgentChooserCancel)
    }
}

fn new_issue_control(state: &AppState, handler: HandlerKey, chord: Chord) -> Option<AppEvent> {
    let focus = state.issues_state.new_issue_form.as_ref()?.focus;
    match (handler, chord.key, focus) {
        (HandlerKey::FormNextField, Key::Char(' '), NewIssueFormFocus::Template) => {
            Some(AppEvent::NewIssueTemplateNext)
        }
        (HandlerKey::FormNextField, Key::Char(' '), NewIssueFormFocus::Type) => {
            Some(AppEvent::NewIssueTypeNext)
        }
        (HandlerKey::FormNextField, Key::Enter, NewIssueFormFocus::Body) => {
            Some(AppEvent::NewIssueBodyNewline)
        }
        (HandlerKey::FormNextField, Key::Down, NewIssueFormFocus::Body) => {
            Some(AppEvent::NewIssueBodyCursorDown)
        }
        (HandlerKey::FormPreviousField, Key::Up, NewIssueFormFocus::Body) => {
            Some(AppEvent::NewIssueBodyCursorUp)
        }
        (HandlerKey::FormNextField, Key::Char(' '), NewIssueFormFocus::Title) => {
            Some(AppEvent::NewIssueTitleChar(' '))
        }
        (HandlerKey::FormNextField, Key::Char(' '), NewIssueFormFocus::Body) => {
            Some(AppEvent::NewIssueBodyChar(' '))
        }
        (HandlerKey::FormNextField, Key::Char(' '), _) => None,
        (HandlerKey::FormPreviousField, _, _) => Some(AppEvent::NewIssueFocusPrev),
        (HandlerKey::FormNextField, _, _) => Some(AppEvent::NewIssueFocusNext),
        _ => None,
    }
}

fn prs_execution(
    handler: HandlerKey,
    chord: Chord,
    state: &AppState,
    page: PageItemCount,
) -> Option<HandlerExecution> {
    use HandlerExecution::{Event, Noop};
    let event = match handler {
        HandlerKey::NavigateUp => Some(pr_vertical(state, true)),
        HandlerKey::NavigateDown => Some(pr_vertical(state, false)),
        HandlerKey::NavigatePageUp => Some(pr_page(state, true, page)),
        HandlerKey::NavigatePageDown => Some(pr_page(state, false, page)),
        HandlerKey::NavigateHome => Some(AppEvent::PrNavigateHome),
        HandlerKey::NavigateEnd => Some(AppEvent::PrNavigateEnd),
        HandlerKey::PullRequestsExit => Some(AppEvent::ExitPrsMode),
        HandlerKey::PullRequestsBack => pr_back(state, chord),
        HandlerKey::PullRequestsOpen => pr_open(state, chord),
        HandlerKey::PullRequestsOpenFilter => Some(AppEvent::PrOpenFilterControls),
        HandlerKey::PullRequestsComment => Some(super::super::prs::comment_event_for_subfocus(
            state.prs_state.detail_subfocus,
        )),
        HandlerKey::PullRequestsReply => Some(super::super::prs::reply_event_for_subfocus(
            state.prs_state.detail_subfocus,
        )),
        HandlerKey::PullRequestsResolveThread => Some(
            super::super::prs::resolve_event_for_subfocus(state.prs_state.detail_subfocus),
        ),
        HandlerKey::PullRequestsEdit => pr_edit(state, chord),
        HandlerKey::PullRequestsSendToAgent => Some(AppEvent::PrOpenAgentChooser {
            metadata: super::super::build_chooser_metadata(state),
        }),
        HandlerKey::PullRequestsOpenBrowser => Some(
            super::super::prs::pr_open_in_browser_or_notice(pr_target_present(state)),
        ),
        HandlerKey::PullRequestsOpenMerge => {
            Some(super::super::prs::pr_merge_event_for_detail(state))
        }
        HandlerKey::PullRequestsCyclePane => {
            Some(if matches!(chord.key, Key::Left | Key::BackTab) {
                AppEvent::PrCycleFocusReverse
            } else {
                AppEvent::PrCycleFocus
            })
        }
        HandlerKey::PullRequestsSubmitInline => Some(AppEvent::PrInlineSubmit),
        HandlerKey::PullRequestsCancelInline => Some(AppEvent::PrInlineCancelOrEsc),
        HandlerKey::PullRequestsChooserPrevious => pr_chooser(state, true, chord),
        HandlerKey::PullRequestsChooserNext => pr_chooser(state, false, chord),
        HandlerKey::PullRequestsChooserConfirm => pr_chooser_confirm(state),
        HandlerKey::PullRequestsChooserCancel => pr_chooser_cancel(state),
        HandlerKey::SearchApply => Some(AppEvent::PrApplySearch),
        HandlerKey::SearchCancel => Some(pr_search_event(state)),
        HandlerKey::FilterApply
        | HandlerKey::FilterCancel
        | HandlerKey::FilterNextField
        | HandlerKey::FilterPreviousField
        | HandlerKey::FilterClearCurrent
        | HandlerKey::FilterClearAll
        | HandlerKey::FilterPreviousChoice
        | HandlerKey::FilterNextChoice => super::super::prs_filter::control_event(state, handler),
        _ => return None,
    };
    Some(event.map_or(Noop, Event))
}

fn pr_search_event(state: &AppState) -> AppEvent {
    if state.prs_state.search_query.is_empty() {
        AppEvent::PrBlurSearchInput
    } else {
        AppEvent::PrClearSearch
    }
}

fn pr_vertical(state: &AppState, up: bool) -> AppEvent {
    match (state.prs_state.pr_focus, up) {
        (PrFocus::PrDetail, true) => AppEvent::PrScrollDetailUp,
        (PrFocus::PrDetail, false) => AppEvent::PrScrollDetailDown,
        (_, true) => AppEvent::PrNavigateUp,
        (_, false) => AppEvent::PrNavigateDown,
    }
}

fn pr_page(state: &AppState, up: bool, page: PageItemCount) -> AppEvent {
    match (state.prs_state.pr_focus, up) {
        (PrFocus::PrDetail, true) => AppEvent::PrScrollDetailPageUp,
        (PrFocus::PrDetail, false) => AppEvent::PrScrollDetailPageDown,
        (_, true) => AppEvent::PrNavigatePageUp(page),
        (_, false) => AppEvent::PrNavigatePageDown(page),
    }
}

fn pr_back(state: &AppState, chord: Chord) -> Option<AppEvent> {
    match chord.key {
        Key::Char('p' | 'P') => Some(AppEvent::RefocusPrList),
        Key::Function(12) if state.terminal_focused => Some(AppEvent::ToggleTerminalFocus),
        Key::Function(12) if state.prs_state.pr_focus == PrFocus::PrDetail => {
            Some(AppEvent::RefocusPrList)
        }
        Key::Esc if state.prs_state.pr_focus == PrFocus::PrChanges => Some(AppEvent::PrChangesBack),
        Key::BackTab if state.prs_state.pr_focus == PrFocus::PrChanges => {
            (state.prs_state.changes.focus == PrChangesFocus::Content)
                .then_some(AppEvent::PrChangesFocusFiles)
        }
        Key::Esc if state.prs_state.pr_focus == PrFocus::PrDetail => Some(AppEvent::RefocusPrList),
        Key::BackTab | Key::Char('k') if state.prs_state.pr_focus == PrFocus::PrDetail => {
            Some(AppEvent::PrDetailSubfocusPrev)
        }
        _ => None,
    }
}

fn pr_open(state: &AppState, chord: Chord) -> Option<AppEvent> {
    match chord.key {
        Key::Char('g' | 'G') => Some(super::super::prs::pr_to_actions_event(state)),
        Key::Char('i' | 'I') => Some(AppEvent::EnterIssuesMode),
        Key::Tab | Key::Enter if state.prs_state.pr_focus == PrFocus::PrChanges => {
            (state.prs_state.changes.focus == PrChangesFocus::FileList
                && state.prs_state.changes.selected_file.is_some())
            .then_some(AppEvent::PrChangesFocusContent)
        }
        Key::Tab | Key::Char('j') if state.prs_state.pr_focus == PrFocus::PrDetail => {
            Some(AppEvent::PrDetailSubfocusNext)
        }
        _ => Some(AppEvent::PrListEnter),
    }
}

fn pr_edit(state: &AppState, chord: Chord) -> Option<AppEvent> {
    if state.prs_state.pr_focus == PrFocus::PrChanges {
        return pr_changes_edit(state, chord);
    }
    match chord.key {
        Key::Char('e') => Some(AppEvent::PrShowNotice(
            ReadOnlyHintKind::ReadOnlyNotEditable,
        )),
        Key::Char('d') => Some(AppEvent::PrOpenChanges),
        Key::Char('L') => pr_property(state, PrPropertyKind::Labels),
        Key::Char('A') => pr_property(state, PrPropertyKind::Assignees),
        Key::Char('M') => pr_property(state, PrPropertyKind::Milestone),
        Key::Char('T') => pr_property(state, PrPropertyKind::Title),
        Key::Char('W') => pr_property(state, PrPropertyKind::State),
        _ => None,
    }
}

fn pr_property(state: &AppState, kind: PrPropertyKind) -> Option<AppEvent> {
    (state.prs_state.detail_subfocus == PrDetailSubfocus::Body)
        .then_some(AppEvent::PrOpenPropertyEditor { kind })
}

fn pr_changes_edit(state: &AppState, chord: Chord) -> Option<AppEvent> {
    match chord.key {
        Key::Char('v') => Some(AppEvent::PrChangesToggleView),
        Key::Char('c') if state.prs_state.changes.focus == PrChangesFocus::Content => {
            Some(AppEvent::PrOpenChangesComment)
        }
        Key::Char('R') if state.prs_state.changes.focus == PrChangesFocus::Content => {
            super::super::prs::selected_changes_thread(state)
                .map(|thread_index| AppEvent::PrToggleThreadResolve { thread_index })
        }
        Key::Char('r') => pr_changes_retry_or_reply(state),
        _ => None,
    }
}

fn pr_changes_retry_or_reply(state: &AppState) -> Option<AppEvent> {
    if state.prs_state.changes.error.is_some()
        && state.prs_state.changes.focus == PrChangesFocus::FileList
    {
        return Some(AppEvent::PrChangesRetryFiles);
    }
    if state.prs_state.changes.focus != PrChangesFocus::Content {
        return None;
    }
    let selected = super::super::prs::selected_changes_thread(state);
    if state.prs_state.changes.blob_error.is_some() && selected.is_none() {
        Some(AppEvent::PrChangesRetryBlob)
    } else {
        selected.map(|thread_index| AppEvent::PrOpenThreadReplyComposer { thread_index })
    }
}

fn pr_target_present(state: &AppState) -> bool {
    match state.prs_state.pr_focus {
        PrFocus::PrList => state.prs_state.selected_pr_index().is_some(),
        PrFocus::PrDetail => state.prs_state.pr_detail.is_some(),
        PrFocus::RepoList | PrFocus::PrChanges => false,
    }
}

fn pr_chooser(state: &AppState, previous: bool, chord: Chord) -> Option<AppEvent> {
    if state.prs_state.property_editor.is_some() {
        if matches!(chord.key, Key::Char(' ')) {
            return Some(AppEvent::PrPropertyEditorToggle);
        }
        return Some(if previous {
            AppEvent::PrPropertyEditorNavigateUp
        } else {
            AppEvent::PrPropertyEditorNavigateDown
        });
    }
    if state.prs_state.merge_chooser.is_some() {
        return Some(if previous {
            PrLifecycleEvent::MergeNavigateUp.into()
        } else {
            PrLifecycleEvent::MergeNavigateDown.into()
        });
    }
    state.prs_state.agent_chooser.as_ref().map(|_| {
        if previous {
            AppEvent::PrAgentChooserNavigateUp
        } else {
            AppEvent::PrAgentChooserNavigateDown
        }
    })
}

fn pr_chooser_confirm(state: &AppState) -> Option<AppEvent> {
    if state.prs_state.property_editor.is_some() {
        Some(AppEvent::PrPropertyEditorConfirm)
    } else if state.prs_state.merge_chooser.is_some() {
        Some(PrLifecycleEvent::MergeConfirm.into())
    } else {
        state
            .prs_state
            .agent_chooser
            .as_ref()
            .map(|_| AppEvent::PrAgentChooserConfirm)
    }
}

fn pr_chooser_cancel(state: &AppState) -> Option<AppEvent> {
    if state.prs_state.property_editor.is_some() {
        Some(AppEvent::PrPropertyEditorCancel)
    } else if state.prs_state.merge_chooser.is_some() {
        Some(PrLifecycleEvent::MergeCancel.into())
    } else {
        state
            .prs_state
            .agent_chooser
            .as_ref()
            .map(|_| AppEvent::PrAgentChooserCancel)
    }
}

fn actions_execution(
    handler: HandlerKey,
    chord: Chord,
    state: &AppState,
    page: PageItemCount,
) -> Option<HandlerExecution> {
    use HandlerExecution::{Event, Noop};
    let event = match handler {
        HandlerKey::ActionsExit => Some(AppEvent::ExitActionsMode),
        HandlerKey::ActionsBack => Some(AppEvent::ActionsDetailEscape),
        HandlerKey::ActionsReload => Some(AppEvent::ActionsReload),
        HandlerKey::ActionsOpenFilter => Some(AppEvent::ActionsOpenFilterControls),
        HandlerKey::ActionsFocusSearch => Some(AppEvent::ActionsFocusSearchInput),
        HandlerKey::ActionsUp => Some(if state.actions_state.focus == ActionsFocus::Detail {
            AppEvent::ActionsNavigateJobUp
        } else {
            AppEvent::ActionsNavigateUp
        }),
        HandlerKey::ActionsDown => Some(if state.actions_state.focus == ActionsFocus::Detail {
            AppEvent::ActionsNavigateJobDown
        } else {
            AppEvent::ActionsNavigateDown
        }),
        HandlerKey::ActionsPageUp => Some(if state.actions_state.focus == ActionsFocus::Detail {
            AppEvent::ActionsScrollDetailUp
        } else {
            AppEvent::ActionsNavigatePageUp(page)
        }),
        HandlerKey::ActionsPageDown => Some(if state.actions_state.focus == ActionsFocus::Detail {
            AppEvent::ActionsScrollDetailDown
        } else {
            AppEvent::ActionsNavigatePageDown(page)
        }),
        HandlerKey::ActionsActivate => actions_activate(state, chord),
        HandlerKey::SearchApply => Some(AppEvent::ActionsApplySearch),
        HandlerKey::SearchCancel => Some(if state.actions_state.search_query.is_empty() {
            AppEvent::ActionsBlurSearchInput
        } else {
            AppEvent::ActionsClearSearch
        }),
        HandlerKey::FilterApply
        | HandlerKey::FilterCancel
        | HandlerKey::FilterNextField
        | HandlerKey::FilterPreviousField
        | HandlerKey::FilterClearCurrent
        | HandlerKey::FilterClearAll
        | HandlerKey::FilterPreviousChoice
        | HandlerKey::FilterNextChoice => actions_filter(state, handler),
        _ => return None,
    };
    Some(event.map_or(Noop, Event))
}

fn actions_activate(state: &AppState, chord: Chord) -> Option<AppEvent> {
    match chord.key {
        Key::Char('d') => state.actions_state.run_detail.as_ref().map_or_else(
            || {
                state
                    .actions_state
                    .workflows
                    .first()
                    .cloned()
                    .map(AppEvent::OpenWorkflowDispatch)
            },
            |detail| {
                state
                    .actions_state
                    .workflows
                    .iter()
                    .find(|workflow| workflow.name == detail.run.workflow_name)
                    .cloned()
                    .map(AppEvent::OpenWorkflowDispatch)
            },
        ),
        Key::Home => Some(AppEvent::ActionsNavigateHome),
        Key::End => Some(AppEvent::ActionsNavigateEnd),
        Key::Enter if state.actions_state.focus == ActionsFocus::RunList => {
            Some(AppEvent::ActionsEnter)
        }
        Key::Enter | Key::Right if state.actions_state.focus == ActionsFocus::Detail => {
            Some(AppEvent::ActionsExpandJob)
        }
        Key::Left if state.actions_state.focus == ActionsFocus::Detail => {
            Some(AppEvent::ActionsCollapseJob)
        }
        Key::Left => Some(AppEvent::ActionsCycleFocusReverse),
        Key::Right | Key::Tab => Some(AppEvent::ActionsCycleFocus),
        _ => None,
    }
}

fn actions_filter(state: &AppState, handler: HandlerKey) -> Option<AppEvent> {
    let index = state.actions_state.ui.filter_field_index;
    match handler {
        HandlerKey::FilterApply => Some(AppEvent::ActionsApplyFilter),
        HandlerKey::FilterCancel => Some(AppEvent::ActionsCloseFilterControls),
        HandlerKey::FilterNextField => Some(AppEvent::ActionsFilterNavigateNext),
        HandlerKey::FilterPreviousField => Some(AppEvent::ActionsFilterNavigatePrev),
        HandlerKey::FilterClearAll => Some(AppEvent::ActionsClearDraftFilter),
        HandlerKey::FilterClearCurrent => match index {
            0 => Some(actions_clear(ActionsFilterField::Workflow)),
            1 => Some(actions_clear(ActionsFilterField::Status)),
            2 => Some(actions_clear(ActionsFilterField::Pr)),
            _ => None,
        },
        HandlerKey::FilterNextChoice if index == 3 => Some(AppEvent::CycleActionsSortByNext),
        HandlerKey::FilterPreviousChoice if index == 3 => Some(AppEvent::CycleActionsSortByPrev),
        HandlerKey::FilterNextChoice | HandlerKey::FilterPreviousChoice if index == 4 => {
            Some(AppEvent::ToggleActionsSortOrder)
        }
        HandlerKey::FilterNextChoice | HandlerKey::FilterPreviousChoice => {
            Some(AppEvent::ActionsCycleFilterStatus)
        }
        _ => None,
    }
}

fn actions_clear(field: ActionsFilterField) -> AppEvent {
    AppEvent::ActionsUpdateDraftFilter {
        field,
        value: String::new(),
    }
}

fn dashboard_search_execution(handler: HandlerKey, state: &AppState) -> Option<HandlerExecution> {
    match handler {
        HandlerKey::SearchApply => Some(HandlerExecution::Event(AppEvent::BlurDashboardSearch)),
        HandlerKey::SearchCancel => Some(HandlerExecution::Event(
            if state.dashboard_search.query.is_empty() {
                AppEvent::BlurDashboardSearch
            } else {
                AppEvent::ClearDashboardSearch
            },
        )),
        _ => None,
    }
}

pub(super) fn modal_execution(handler: HandlerKey, chord: Chord) -> Option<HandlerExecution> {
    use HandlerExecution::{Boundary, Event, Noop};
    Some(match handler {
        HandlerKey::HelpClose | HandlerKey::ConfirmCancel | HandlerKey::FormCancel => {
            Event(AppEvent::CloseModal)
        }
        HandlerKey::HelpScrollUp => Boundary(BoundaryAction::HelpScrollUp),
        HandlerKey::HelpScrollDown => Boundary(BoundaryAction::HelpScrollDown),
        HandlerKey::HelpPageUp => Boundary(BoundaryAction::HelpPageUp),
        HandlerKey::HelpPageDown => Boundary(BoundaryAction::HelpPageDown),
        HandlerKey::HelpHome => Boundary(BoundaryAction::HelpHome),
        HandlerKey::HelpEnd => Boundary(BoundaryAction::HelpEnd),
        HandlerKey::ConfirmCycleFocus => Event(AppEvent::ConfirmCycleFocus),
        HandlerKey::ConfirmToggleDeleteWorkDir => Event(AppEvent::ToggleDeleteWorkDir),
        HandlerKey::AuthCancel => Event(AppEvent::AuthCancelled),
        HandlerKey::FormNextField if matches!(chord.key, Key::Char(' ')) => {
            Boundary(BoundaryAction::FormSpace)
        }
        HandlerKey::FormNextField => Event(AppEvent::FormNextField),
        HandlerKey::FormPreviousField => Event(AppEvent::FormPrevField),
        HandlerKey::ThemeToggleOverride => Event(AppEvent::ThemePickerToggleOverride),
        HandlerKey::SearchBackspace => Noop,
        HandlerKey::ConfirmAccept => Boundary(BoundaryAction::ConfirmAccept),
        HandlerKey::AuthRetry => Boundary(BoundaryAction::AuthRetry),
        HandlerKey::FormSubmit => Boundary(BoundaryAction::FormSubmit),
        HandlerKey::ThemeUp => Boundary(BoundaryAction::ThemeUp),
        HandlerKey::ThemeDown => Boundary(BoundaryAction::ThemeDown),
        HandlerKey::ThemeApply => Boundary(BoundaryAction::ThemeApply),
        HandlerKey::ThemeCancel => Boundary(BoundaryAction::ThemeCancel),
        HandlerKey::SearchApply | HandlerKey::SearchCancel => Event(AppEvent::CloseModal),
        _ => return None,
    })
}

pub(super) fn apply_modal_boundary(
    boundary: BoundaryAction,
    app_state: &mut super::super::AppStateHandle,
    ctx: &super::super::SharedContext,
) {
    use super::super::modal_handlers as modal;
    match boundary {
        BoundaryAction::ConfirmAccept => modal::handle_confirm_enter(app_state, ctx),
        BoundaryAction::AuthRetry => {
            super::super::apply_and_persist(app_state, ctx, AppEvent::AuthRetry);
            super::super::auth_remediation::spawn_device_auth_flow(app_state, ctx);
        }
        BoundaryAction::FormSubmit => modal::handle_form_submit(app_state, ctx),
        BoundaryAction::FormSpace => {
            if let Some(evt) = modal::handle_form_space(app_state, ctx) {
                super::super::apply_and_persist(app_state, ctx, evt);
            }
        }
        BoundaryAction::ThemeUp => {
            super::super::apply_and_persist(app_state, ctx, AppEvent::ThemePickerNavigateUp);
            modal::preview_theme_selection(app_state, ctx);
        }
        BoundaryAction::ThemeDown => {
            super::super::apply_and_persist(app_state, ctx, AppEvent::ThemePickerNavigateDown);
            modal::preview_theme_selection(app_state, ctx);
        }
        BoundaryAction::ThemeApply => modal::apply_theme_picker_selection(app_state, ctx),
        BoundaryAction::ThemeCancel => {
            modal::revert_theme_to_active(app_state, ctx);
            super::super::apply_and_persist(app_state, ctx, AppEvent::CloseThemePicker);
        }
        _ => {}
    }
}
