//! Modal orchestration helpers for the root App component.
//!
//! Provides data derivation and element construction for modal overlays,
//! keeping the App component focused on orchestration flow.

use iocraft::prelude::*;

use crate::state::{AppState, ConfirmFocus, ModalState, ScreenMode};
use crate::theme::ThemeColors;
use crate::ui::screens::{ActionsScreen, IssuesScreen, PullRequestsScreen, ThemePickerScreen};
use crate::ui::{
    ConfirmModal, Dashboard, HelpModal, NewAgentForm, NewRepositoryForm, SplitScreen,
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
#[must_use]
pub fn derive_confirm_modal_data(
    snapshot: &AppState,
    modal: &ModalState,
) -> Option<ConfirmModalData> {
    let focus = confirm_focus_from_modal(modal);
    confirm_modal_text(modal, snapshot).map(
        |(title, message, show_delete_work_dir, delete_work_dir)| ConfirmModalData {
            title,
            message,
            show_delete_work_dir,
            delete_work_dir,
            confirm_focus: focus,
        },
    )
}

/// Extract the confirm focus from any confirm variant (issue #228).
fn confirm_focus_from_modal(modal: &ModalState) -> ConfirmFocus {
    match modal {
        ModalState::ConfirmDeleteAgent { confirm_focus, .. }
        | ModalState::ConfirmDeleteRepository { confirm_focus, .. }
        | ModalState::ConfirmKillAgent { confirm_focus, .. }
        | ModalState::PreflightPrompt { confirm_focus, .. }
        | ModalState::ConfirmIssueDirtyCopy { confirm_focus, .. }
        | ModalState::ConfirmIssueOriginMismatch { confirm_focus, .. } => *confirm_focus,
        _ => ConfirmFocus::Cancel,
    }
}

/// Derive the title, message, and checkbox state from a confirm modal.
/// Returns `None` for non-confirm modals.
fn confirm_modal_text(
    modal: &ModalState,
    snapshot: &AppState,
) -> Option<(String, String, bool, bool)> {
    match modal {
        ModalState::ConfirmDeleteAgent {
            id,
            delete_work_dir,
            ..
        } => Some((
            String::from("Delete Agent"),
            format!("Delete {}?", agent_display_name(snapshot, id)),
            true,
            *delete_work_dir,
        )),
        ModalState::ConfirmKillAgent { id, .. } => Some((
            String::from("Kill Agent"),
            format!("Kill {}?", agent_display_name(snapshot, id)),
            false,
            false,
        )),
        ModalState::ConfirmDeleteRepository { id, .. } => Some((
            String::from("Delete Repository"),
            format!(
                "Delete {} and all its agents?",
                repo_display_name(snapshot, id)
            ),
            false,
            false,
        )),
        ModalState::PreflightPrompt { issue, .. } => {
            Some((issue.prompt_title(), issue.prompt_message(), false, false))
        }
        ModalState::ConfirmIssueDirtyCopy { .. } => Some((
            String::from("Dirty Working Copy"),
            String::from(
                "Working copy has uncommitted changes. Discard them (git reset --hard + git clean)?",
            ),
            false,
            false,
        )),
        ModalState::ConfirmIssueOriginMismatch {
            actual, expected, ..
        } => Some((
            String::from("Wrong Repository"),
            format!(
                "Working copy origin is {actual_repr}, expected {expected}. \
                     Replace it with a fresh clone?",
                actual_repr = if actual.is_empty() {
                    "(no origin remote)"
                } else {
                    actual
                },
            ),
            false,
            false,
        )),
        _ => None,
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
        _ => None,
    }
}
