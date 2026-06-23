//! Modal orchestration helpers for the root App component.
//!
//! Provides data derivation and element construction for modal overlays,
//! keeping the App component focused on orchestration flow.

use iocraft::prelude::*;

use crate::state::{AppState, ModalState, ScreenMode};
use crate::theme::ThemeColors;
use crate::ui::screens::IssuesScreen;
use crate::ui::{ConfirmModal, Dashboard, HelpModal, NewAgentForm, NewRepositoryForm, SplitScreen};

/// Data needed to render a confirmation modal.
pub struct ConfirmModalData {
    pub title: String,
    pub message: String,
    pub show_delete_work_dir: bool,
    pub delete_work_dir: bool,
}

/// Derive confirmation modal data from current state, if applicable.
#[must_use]
pub fn derive_confirm_modal_data(
    snapshot: &AppState,
    modal: &ModalState,
) -> Option<ConfirmModalData> {
    match modal {
        ModalState::ConfirmDeleteAgent {
            id,
            delete_work_dir,
        } => {
            let agent_name = snapshot
                .agents
                .iter()
                .find(|agent| &agent.id == id)
                .map_or_else(
                    || String::from("selected agent"),
                    |agent| agent.name.clone(),
                );
            Some(ConfirmModalData {
                title: String::from("Delete Agent"),
                message: format!("Delete {agent_name}?"),
                show_delete_work_dir: true,
                delete_work_dir: *delete_work_dir,
            })
        }
        ModalState::ConfirmDeleteRepository { id } => {
            let repo_name = snapshot
                .repositories
                .iter()
                .find(|repo| &repo.id == id)
                .map_or_else(
                    || String::from("selected repository"),
                    |repo| repo.name.clone(),
                );
            Some(ConfirmModalData {
                title: String::from("Delete Repository"),
                message: format!("Delete {repo_name} and all its agents?"),
                show_delete_work_dir: false,
                delete_work_dir: false,
            })
        }
        ModalState::PreflightPrompt { issue, .. } => Some(ConfirmModalData {
            title: issue.prompt_title(),
            message: issue.prompt_message(),
            show_delete_work_dir: false,
            delete_work_dir: false,
        }),
        _ => None,
    }
}

/// Build the screen element for the current screen mode.
#[must_use]
pub fn build_screen_element(
    snapshot: &AppState,
    colors: &ThemeColors,
    theme_name: &str,
    terminal_snapshot: Option<crate::runtime::TerminalSnapshot>,
) -> AnyElement<'static> {
    match snapshot.screen_mode {
        ScreenMode::Dashboard => element! {
            Dashboard(
                state: Some(snapshot.clone()),
                colors: Some(colors.clone()),
                theme_name: theme_name.to_owned(),
                terminal_snapshot: terminal_snapshot,
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
    }
}

/// Build the modal element for the current modal state, if any.
#[must_use]
pub fn build_modal_element(
    snapshot: &AppState,
    modal: &ModalState,
    colors: &ThemeColors,
    confirm_data: Option<ConfirmModalData>,
) -> Option<AnyElement<'static>> {
    match modal {
        ModalState::Help => Some(
            element! {
                HelpModal(colors: colors.clone())
            }
            .into_any(),
        ),
        ModalState::NewRepository { .. } | ModalState::EditRepository { .. } => Some(
            element! {
                NewRepositoryForm(
                    state: Some(snapshot.clone()),
                    colors: Some(colors.clone()),
                )
            }
            .into_any(),
        ),
        ModalState::NewAgent { .. } | ModalState::EditAgent { .. } => Some(
            element! {
                NewAgentForm(
                    state: Some(snapshot.clone()),
                    colors: Some(colors.clone()),
                )
            }
            .into_any(),
        ),
        ModalState::ConfirmDeleteRepository { .. }
        | ModalState::ConfirmDeleteAgent { .. }
        | ModalState::PreflightPrompt { .. } => confirm_data.map(|data| {
            element! {
                ConfirmModal(
                    title: data.title,
                    message: data.message,
                    show_delete_work_dir: data.show_delete_work_dir,
                    delete_work_dir: data.delete_work_dir,
                    colors: colors.clone(),
                )
            }
            .into_any()
        }),
        _ => None,
    }
}
