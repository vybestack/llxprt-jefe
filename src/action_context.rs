//! Pure selection of the action-registry context for one application state.
//!
//! The selector encodes current source precedence without resolving a key. A
//! stack always orders the active modal/editor/chooser, focused panel, screen,
//! and global contexts from most to least specific.

use jefe::domain::input_context::{ContextStack, ContextStackError};
use jefe::input::InputMode;
use jefe::state::{ActionsFocus, AppState, IssueFocus, ModalState, PaneFocus, PrFocus, ScreenId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchScope {
    ShellOverlay,
    TerminalCapture,
    FullS3,
    FullS4,
    PreModeOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionContext {
    pub stack: ContextStack,
    pub scope: DispatchScope,
}

pub fn derive_action_context(
    state: &AppState,
    input_mode: InputMode,
) -> Result<ActionContext, ContextStackError> {
    if state.shell_overlay_active() {
        return action_context(&["shell-overlay"], false, DispatchScope::ShellOverlay);
    }
    if input_mode == InputMode::TerminalCapture {
        return action_context(
            &["terminal", "global"],
            true,
            DispatchScope::TerminalCapture,
        );
    }
    if let Some(modal) = modal_context(&state.modal) {
        return modal_stack(state.screen(), modal);
    }
    match state.screen() {
        ScreenId::Dashboard if input_mode == InputMode::DashboardSearch => action_context(
            &["dashboard.search", "dashboard.pre-mode", "global"],
            false,
            DispatchScope::FullS4,
        ),
        ScreenId::Dashboard if input_mode == InputMode::Normal => dashboard_context(state),
        ScreenId::Repositories if input_mode == InputMode::Normal => full_s3(&["split", "global"]),
        ScreenId::Errors if input_mode == InputMode::Normal => full_s3(&["errors", "global"]),
        ScreenId::Terminals if input_mode == InputMode::Normal => {
            full_s3(&["terminal-manager", "global"])
        }
        ScreenId::Settings if input_mode == InputMode::Normal => full_s3(&["settings", "global"]),
        ScreenId::Issues => issues_context(state),
        ScreenId::PullRequests => prs_context(state),
        ScreenId::Actions => actions_context(state),
        ScreenId::Dashboard => pre_mode(&["dashboard", "global"]),
        ScreenId::Repositories => pre_mode(&["split", "global"]),
        ScreenId::Errors | ScreenId::Terminals | ScreenId::Settings => pre_mode(&["global"]),
    }
}

fn modal_context(modal: &ModalState) -> Option<&'static str> {
    match modal {
        ModalState::Help => Some("help"),
        // The Keys editor owns its own input, but it must still derive a
        // distinct context: naming `global` here repeats the stack's own tail
        // and `ContextStack` rejects duplicates, which would swallow the
        // protected emergency exit the editor deliberately lets through.
        ModalState::Keys { .. } => Some("keys"),
        ModalState::Search { .. } => Some("search"),
        ModalState::NewRepository { .. }
        | ModalState::EditRepository { .. }
        | ModalState::NewAgent { .. }
        | ModalState::EditAgent { .. }
        | ModalState::GeneratedAgent { .. }
        | ModalState::WorkflowDispatch { .. } => Some("modal.form"),
        ModalState::ConfirmDeleteRepository { .. }
        | ModalState::ConfirmDeleteAgent { .. }
        | ModalState::ConfirmKillAgent { .. }
        | ModalState::ConfirmServerLostRecovery { .. }
        | ModalState::PreflightPrompt { .. }
        | ModalState::ConfirmIssueDirtyCopy { .. }
        | ModalState::ConfirmIssueOriginMismatch { .. } => Some("modal.confirm"),
        ModalState::Auth { .. } => Some("modal.auth"),
        ModalState::None => None,
    }
}

fn modal_stack(screen: ScreenId, modal: &str) -> Result<ActionContext, ContextStackError> {
    if matches!(
        screen,
        ScreenId::Dashboard | ScreenId::Repositories | ScreenId::Actions
    ) {
        return action_context(
            &[modal, "dashboard.pre-mode", "global"],
            false,
            DispatchScope::FullS4,
        );
    }
    action_context(&[modal, "global"], false, DispatchScope::FullS4)
}

fn issues_context(state: &AppState) -> Result<ActionContext, ContextStackError> {
    let focused = match state.issues_state.issue_focus {
        IssueFocus::RepoList => "issues.repo-list",
        IssueFocus::IssueList => "issues.list",
        IssueFocus::IssueDetail => "issues.detail",
    };
    let special = if state.issues_state.property_editor.is_some() {
        Some("issues.property")
    } else if state.issues_state.close_reason_chooser.is_some() {
        Some("issues.close-reason")
    } else if state.issues_state.delete_confirm.is_some() {
        Some("issues.delete-confirm")
    } else if state.issues_state.new_issue_form.is_some() {
        Some("issues.new-form")
    } else if state.issues_state.inline_state != jefe::state::InlineState::None {
        Some("issues.inline")
    } else if state.issues_state.agent_chooser.is_some() {
        Some("issues.agent-chooser")
    } else if state.issues_state.search_input_focused {
        Some("issues.search")
    } else if state.issues_state.filter_ui.controls_open {
        Some("issues.filter")
    } else {
        None
    };
    workspace_stack(special, focused, "issues")
}

fn prs_context(state: &AppState) -> Result<ActionContext, ContextStackError> {
    let focused = match state.prs_state.pr_focus {
        PrFocus::RepoList => "prs.repo-list",
        PrFocus::PrList => "prs.list",
        PrFocus::PrDetail => "prs.detail",
        PrFocus::PrChanges => "prs.changes",
    };
    let special = if state.prs_state.inline_state != jefe::state::InlineState::None {
        Some("prs.inline")
    } else if state.prs_state.agent_chooser.is_some() {
        Some("prs.agent-chooser")
    } else if state.prs_state.merge_chooser.is_some() {
        Some("prs.merge-chooser")
    } else if state.prs_state.property_editor.is_some() {
        Some("prs.property")
    } else if state.prs_state.delete_confirm.is_some() {
        Some("prs.delete-confirm")
    } else if state.prs_state.new_pr_form.is_some() {
        Some("prs.new-form")
    } else if state.prs_state.search_input_focused {
        Some("prs.search")
    } else if state.prs_state.filter_ui.controls_open {
        Some("prs.filter")
    } else {
        None
    };
    workspace_stack(special, focused, "prs")
}

fn actions_context(state: &AppState) -> Result<ActionContext, ContextStackError> {
    let focused = match state.actions_state.focus {
        ActionsFocus::RepoList => "actions.repo-list",
        ActionsFocus::RunList => "actions.run-list",
        ActionsFocus::Detail => "actions.detail",
    };
    let special = if state.actions_state.ui.search_input_focused {
        Some("actions.search")
    } else if state.actions_state.ui.filter_ui_open {
        Some("actions.filter")
    } else {
        None
    };
    workspace_stack(special, focused, "actions")
}

fn workspace_stack(
    special: Option<&str>,
    focused: &str,
    screen: &str,
) -> Result<ActionContext, ContextStackError> {
    match special {
        Some(value) => action_context(&[value, "global"], false, DispatchScope::FullS4),
        None => action_context(&[focused, screen, "global"], false, DispatchScope::FullS4),
    }
}

fn dashboard_context(state: &AppState) -> Result<ActionContext, ContextStackError> {
    if state.dashboard_grab.is_some() {
        return full_s3(&["dashboard.grab", "dashboard.reorder", "dashboard", "global"]);
    }
    if matches!(
        state.pane_focus,
        PaneFocus::Repositories | PaneFocus::Agents
    ) {
        return full_s3(&["dashboard.reorder", "dashboard", "global"]);
    }
    full_s3(&["dashboard", "global"])
}

fn full_s3(contexts: &[&str]) -> Result<ActionContext, ContextStackError> {
    action_context(contexts, false, DispatchScope::FullS3)
}

fn pre_mode(contexts: &[&str]) -> Result<ActionContext, ContextStackError> {
    action_context(contexts, false, DispatchScope::PreModeOnly)
}

fn action_context(
    contexts: &[&str],
    terminal_capture: bool,
    scope: DispatchScope,
) -> Result<ActionContext, ContextStackError> {
    ContextStack::from_ordered(contexts.iter().copied(), terminal_capture)
        .map(|stack| ActionContext { stack, scope })
}

#[cfg(test)]
#[path = "action_context_tests.rs"]
mod tests;
