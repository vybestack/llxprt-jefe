//! Modal orchestration helpers for the root App component.
//!
//! Provides data derivation and element construction for modal overlays,
//! keeping the App component focused on orchestration flow.

use iocraft::prelude::*;

use crate::state::{AppState, ConfirmFocus, ModalState, ScreenMode};
use crate::theme::ThemeColors;
use crate::ui::screens::{
    ActionsScreen, ErrorsScreen, IssuesScreen, PullRequestsScreen, TerminalManagerScreen,
    ThemePickerScreen,
};
use crate::ui::{
    AuthModal, ConfirmModal, Dashboard, HelpModal, NewAgentForm, NewRepositoryForm, SplitScreen,
    WorkflowDispatchForm,
};

/// Data needed to render a confirmation modal.
pub struct ConfirmModalData {
    pub title: String,
    pub message: String,
    pub show_delete_work_dir: bool,
    pub delete_work_dir: bool,
    pub confirm_focus: ConfirmFocus,
}

/// Terminal render data threaded from the app shell into the dashboard.
///
/// Bundles the live snapshot, retained scrollback history, and the actual PTY
/// pane dimensions so `build_screen_element` stays under the argument-count
/// limit and the projection always knows the real pane size even when the live
/// snapshot is absent/empty (issue #198 follow-up).
#[must_use]
pub struct TerminalRenderData {
    /// Live PTY snapshot (styled grid), if available.
    pub snapshot: Option<crate::runtime::TerminalSnapshot>,
    /// Retained scrollback history lines (plain text).
    pub history_lines: Vec<String>,
    /// Actual embedded-terminal pane row count (PTY layout).
    pub pane_rows: usize,
    /// Actual embedded-terminal pane column count (PTY layout).
    pub pane_cols: usize,
}

/// Derive confirmation modal data from current state, if applicable.
///
/// The match covers all six confirm variants and extracts `confirm_focus`
/// alongside the title/message/checkbox in a single arm per variant, so the
/// six variants cannot drift out of sync with each other. NOTE: the
/// `_ => return None` catch-all means a NEWLY-added `ModalState` confirm
/// variant would silently return `None` (modal won't render) rather than
/// fail at compile time — so new confirm variants must be explicitly added
/// here. The `confirm_modal_renders_all_variants` test in
/// `src/selection/overlay_content.rs` guards against this by asserting every
/// confirm variant renders.
#[must_use]
pub fn derive_confirm_modal_data(
    snapshot: &AppState,
    modal: &ModalState,
) -> Option<ConfirmModalData> {
    let (title, message, show_delete_work_dir, delete_work_dir, confirm_focus) = match modal {
        ModalState::ConfirmDeleteAgent {
            id,
            delete_work_dir,
            confirm_focus,
        } => {
            let (title, message, show) = confirm_text(snapshot, ConfirmKind::DeleteAgent(id));
            (title, message, show, *delete_work_dir, *confirm_focus)
        }
        ModalState::ConfirmKillAgent { id, confirm_focus } => {
            let (title, message, show) = confirm_text(snapshot, ConfirmKind::KillAgent(id));
            (title, message, show, false, *confirm_focus)
        }
        ModalState::ConfirmDeleteRepository { id, confirm_focus } => {
            let (title, message, show) = confirm_text(snapshot, ConfirmKind::DeleteRepository(id));
            (title, message, show, false, *confirm_focus)
        }
        ModalState::PreflightPrompt {
            issue,
            confirm_focus,
            ..
        } => (
            issue.prompt_title(),
            issue.prompt_message(),
            false,
            false,
            *confirm_focus,
        ),
        ModalState::ConfirmIssueDirtyCopy { confirm_focus, .. } => {
            let (title, message, show) = confirm_text(snapshot, ConfirmKind::IssueDirtyCopy);
            (title, message, show, false, *confirm_focus)
        }
        ModalState::ConfirmIssueOriginMismatch {
            actual,
            expected,
            confirm_focus,
            ..
        } => {
            let (title, message, show) = confirm_text(
                snapshot,
                ConfirmKind::IssueOriginMismatch { actual, expected },
            );
            (title, message, show, false, *confirm_focus)
        }
        _ => return None,
    };
    Some(ConfirmModalData {
        title,
        message,
        show_delete_work_dir,
        delete_work_dir,
        confirm_focus,
    })
}

/// Which confirm variant to format, carrying only the fields needed for
/// title/message construction.
enum ConfirmKind<'a> {
    DeleteAgent(&'a crate::domain::AgentId),
    KillAgent(&'a crate::domain::AgentId),
    DeleteRepository(&'a crate::domain::RepositoryId),
    IssueDirtyCopy,
    IssueOriginMismatch {
        actual: &'a String,
        expected: &'a String,
    },
}

/// Build `(title, message, show_delete_work_dir)` for a confirm variant.
fn confirm_text(snapshot: &AppState, kind: ConfirmKind) -> (String, String, bool) {
    match kind {
        ConfirmKind::DeleteAgent(id) => (
            String::from("Delete Agent"),
            format!("Delete {}?", agent_display_name(snapshot, id)),
            true,
        ),
        ConfirmKind::KillAgent(id) => (
            String::from("Kill Agent"),
            format!("Kill {}?", agent_display_name(snapshot, id)),
            false,
        ),
        ConfirmKind::DeleteRepository(id) => (
            String::from("Delete Repository"),
            format!(
                "Delete {} and all its agents?",
                repo_display_name(snapshot, id)
            ),
            false,
        ),
        ConfirmKind::IssueDirtyCopy => (
            String::from("Working Copy Not Ready"),
            String::from(
                "The agent working copy is not on the default branch or has uncommitted changes. \
                 Switch to the default branch, discard non-owned changes, and pull?",
            ),
            false,
        ),
        ConfirmKind::IssueOriginMismatch { actual, expected } => {
            let actual_repr = if actual.is_empty() {
                "(no origin remote)"
            } else {
                actual
            };
            (
                String::from("Wrong Repository"),
                format!(
                    "Working copy origin is {actual_repr}, expected {expected}. Replace it with a fresh clone?"
                ),
                false,
            )
        }
    }
}

/// Resolve an agent's display name, falling back to a generic label.
fn agent_display_name(snapshot: &AppState, id: &crate::domain::AgentId) -> String {
    snapshot
        .agents
        .iter()
        .find(|a| &a.id == id)
        .map_or_else(|| String::from("selected agent"), |a| a.name.clone())
}

/// Resolve a repository's display name, falling back to a generic label.
fn repo_display_name(snapshot: &AppState, id: &crate::domain::RepositoryId) -> String {
    snapshot
        .repositories
        .iter()
        .find(|r| &r.id == id)
        .map_or_else(|| String::from("selected repository"), |r| r.name.clone())
}

fn terminal_manager_element(
    snapshot: &AppState,
    colors: &ThemeColors,
    theme_name: &str,
) -> AnyElement<'static> {
    element! {
        TerminalManagerScreen(
            state: Some(snapshot.clone()),
            colors: Some(colors.clone()),
            theme_name: theme_name.to_owned(),
        )
    }
    .into_any()
}

/// Build the screen element for the current screen mode.
#[must_use]
pub fn build_screen_element(
    snapshot: &AppState,
    colors: &ThemeColors,
    theme_name: &str,
    terminal: TerminalRenderData,
) -> AnyElement<'static> {
    match snapshot.screen_mode {
        ScreenMode::Dashboard => element! {
            Dashboard(
                state: Some(snapshot.clone()),
                colors: Some(colors.clone()),
                theme_name: theme_name.to_owned(),
                terminal_snapshot: terminal.snapshot,
                history_lines: terminal.history_lines,
                terminal_pane_rows: terminal.pane_rows,
                terminal_pane_cols: terminal.pane_cols,
                git_info: snapshot.selection_dashboard_git_info.clone().or_else(|| {
                    crate::dashboard_git_info::resolve_dashboard_git_info(snapshot)
                }),
            )
        }
        .into_any(),
        ScreenMode::DashboardIssues => element! {
            IssuesScreen(
                state: Some(snapshot.clone()),
                colors: Some(colors.clone()),
                theme_name: theme_name.to_owned(),
            )
        }
        .into_any(),
        ScreenMode::Split => element! {
            SplitScreen(
                state: Some(snapshot.clone()),
                colors: Some(colors.clone()),
                theme_name: theme_name.to_owned(),
            )
        }
        .into_any(),
        // @plan PLAN-20260624-PR-MODE.P12
        // @requirement REQ-PR-001
        ScreenMode::DashboardPullRequests => element! {
            PullRequestsScreen(
                state: Some(snapshot.clone()),
                colors: Some(colors.clone()),
                theme_name: theme_name.to_owned(),
            )
        }
        .into_any(),
        ScreenMode::DashboardActions => element! {
            ActionsScreen(
                state: Some(snapshot.clone()),
                colors: Some(colors.clone()),
                theme_name: theme_name.to_owned(),
            )
        }
        .into_any(),
        ScreenMode::DashboardErrors => element! {
            ErrorsScreen(
                state: Some(snapshot.clone()),
                colors: Some(colors.clone()),
                theme_name: theme_name.to_owned(),
            )
        }
        .into_any(),
        ScreenMode::DashboardTerminals => terminal_manager_element(snapshot, colors, theme_name),
    }
}

/// Build a state+colors form modal element for a given iocraft component.
///
/// The repository/agent/workflow-dispatch forms all share the same
/// `(state, colors)` prop shape; this macro keeps the modal dispatch free of
/// repeated boilerplate (and under the too-many-lines gate).
macro_rules! form_modal {
    ($component:ident, $state:expr, $colors:expr) => {
        element! {
            $component(
                state: Some($state.clone()),
                colors: Some($colors.clone()),
            )
        }
        .into_any()
    };
}

/// Build the modal element for the current modal state, if any.
///
/// `help_scroll_offset` and `available_rows` are forwarded to the `HelpModal`
/// so its `ScrollableText` viewport never overflows the screen.
#[must_use]
pub fn build_modal_element(
    snapshot: &AppState,
    modal: &ModalState,
    colors: &ThemeColors,
    confirm_data: Option<ConfirmModalData>,
    help_scroll_offset: usize,
    available_rows: u16,
) -> Option<AnyElement<'static>> {
    match modal {
        ModalState::Help => Some(
            element! {
                HelpModal(
                    colors: colors.clone(),
                    scroll_offset: help_scroll_offset,
                    available_rows: available_rows,
                    selection: snapshot.selection,
                )
            }
            .into_any(),
        ),
        ModalState::ThemePicker { .. } => Some(
            element! {
                ThemePickerScreen(
                    state: Some(snapshot.clone()),
                    colors: Some(colors.clone()),
                )
            }
            .into_any(),
        ),
        ModalState::NewRepository { .. } | ModalState::EditRepository { .. } => {
            Some(form_modal!(NewRepositoryForm, snapshot, colors))
        }
        ModalState::NewAgent { .. } | ModalState::EditAgent { .. } => {
            Some(form_modal!(NewAgentForm, snapshot, colors))
        }
        ModalState::WorkflowDispatch { .. } => {
            Some(form_modal!(WorkflowDispatchForm, snapshot, colors))
        }
        ModalState::ConfirmDeleteRepository { .. }
        | ModalState::ConfirmDeleteAgent { .. }
        | ModalState::ConfirmKillAgent { .. }
        | ModalState::PreflightPrompt { .. }
        | ModalState::ConfirmIssueDirtyCopy { .. }
        | ModalState::ConfirmIssueOriginMismatch { .. } => confirm_data.map(|data| {
            element! {
                ConfirmModal(
                    title: data.title,
                    message: data.message,
                    show_delete_work_dir: data.show_delete_work_dir,
                    delete_work_dir: data.delete_work_dir,
                    confirm_focus: data.confirm_focus,
                    colors: colors.clone(),
                    selection: snapshot.selection,
                )
            }
            .into_any()
        }),
        // In-app device-code auth remediation dialog (issue #244). Render-only:
        // receives the dialog state as plain data.
        ModalState::Auth { state } => Some(auth_modal_element(state, colors, snapshot)),
        _ => None,
    }
}

/// Build the render-only auth remediation modal element (issue #244).
fn auth_modal_element(
    state: &crate::state::AuthDialogState,
    colors: &ThemeColors,
    snapshot: &AppState,
) -> AnyElement<'static> {
    element! {
        AuthModal(
            state: state.clone(),
            colors: colors.clone(),
            selection: snapshot.selection,
        )
    }
    .into_any()
}
