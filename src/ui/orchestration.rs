//! Modal orchestration helpers for the root App component.
//!
//! Provides data derivation and element construction for modal overlays,
//! keeping the App component focused on orchestration flow.

use iocraft::prelude::*;

use crate::host_controls::PanelHitTarget;
use crate::overlay_controls::{
    ConfirmationContent, OverlayControlProjection, project_confirmation, project_help,
    project_repository_form,
};
use crate::state::{AppState, ConfirmFocus, ModalState, ScreenId};
use crate::theme::ThemeColors;
use crate::ui::components::{HostControlOverlay, ProviderScreen};
use crate::ui::screens::{
    ActionsScreen, ErrorsScreen, IssuesScreen, PullRequestsScreen, SettingsScreen,
};
use crate::ui::{AuthModal, GeneratedAgentForm, SplitScreen, WorkflowDispatchForm};

/// Data needed to render a confirmation modal.
pub struct ConfirmModalData {
    pub title: String,
    pub message: String,
    pub show_delete_work_dir: bool,
    pub delete_work_dir: bool,
    pub confirm_focus: ConfirmFocus,
}

/// Viewport available to a blocking modal.
#[derive(Clone, Copy)]
pub struct ModalViewport {
    pub cols: u16,
    pub rows: u16,
}

/// Terminal render data threaded from the app shell into shared screen controls.
///
/// Bundles the live snapshot, retained scrollback history, and the actual PTY
/// pane dimensions so `build_screen_element` stays under the argument-count
/// limit and a private host terminal control always receives the real pane size,
/// even when the live snapshot is absent or empty (issue #198 follow-up).
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

/// Derive generic confirmation data from the current screen instance.
///
/// The exhaustive request match keeps all seven host confirmations in one
/// exact-instance authority. Provider confirmations never enter this path.
#[must_use]
pub fn derive_confirm_modal_data(snapshot: &AppState) -> Option<ConfirmModalData> {
    use crate::state::screen_overlays::ConfirmationRequest;

    let request = snapshot.nav.current().overlays().generic_confirmation()?;
    let (title, message, show_delete_work_dir, delete_work_dir) = match request {
        ConfirmationRequest::DeleteAgent {
            id,
            delete_work_dir,
        } => {
            let (title, message, show) = confirm_text(snapshot, ConfirmKind::DeleteAgent(id));
            (title, message, show, *delete_work_dir)
        }
        ConfirmationRequest::KillAgent { id } => {
            let (title, message, show) = confirm_text(snapshot, ConfirmKind::KillAgent(id));
            (title, message, show, false)
        }
        ConfirmationRequest::ServerLostRecovery { agent_ids } => {
            server_lost_confirmation(agent_ids.len())
        }
        ConfirmationRequest::DeleteRepository { id } => {
            let (title, message, show) = confirm_text(snapshot, ConfirmKind::DeleteRepository(id));
            (title, message, show, false)
        }
        ConfirmationRequest::Preflight { issue, .. } => {
            (issue.prompt_title(), issue.prompt_message(), false, false)
        }
        ConfirmationRequest::IssueDirtyCopy { .. } => {
            let (title, message, show) = confirm_text(snapshot, ConfirmKind::IssueDirtyCopy);
            (title, message, show, false)
        }
        ConfirmationRequest::IssueOriginMismatch {
            actual, expected, ..
        } => {
            let (title, message, show) = confirm_text(
                snapshot,
                ConfirmKind::IssueOriginMismatch { actual, expected },
            );
            (title, message, show, false)
        }
    };
    Some(ConfirmModalData {
        title,
        message,
        show_delete_work_dir,
        delete_work_dir,
        confirm_focus: snapshot
            .current_confirm_focus()
            .unwrap_or(ConfirmFocus::Cancel),
    })
}

/// Resolve one displayed confirmation content line to its typed control target.
#[must_use]
pub fn confirmation_hit_target_at_content_line(
    snapshot: &AppState,
    content_line: usize,
    cols: u16,
    rows: u16,
) -> Option<PanelHitTarget> {
    let data = derive_confirm_modal_data(snapshot)?;
    let (cols, rows) = crate::layout::effective_render_size(cols, rows);
    let layout = crate::overlay_controls::HostOverlayLayout::confirmation(cols, rows);
    let projection = project_confirmation(
        ConfirmationContent {
            title: &data.title,
            message: &data.message,
            show_delete_work_dir: data.show_delete_work_dir,
            delete_work_dir: data.delete_work_dir,
            focus: data.confirm_focus,
        },
        layout.content_width,
    );
    // Only the visible projected window is a control surface. Title/footer
    // clicks and rows scrolled out of view must never resolve to a hidden
    // Decision/Submit target.
    if !(1..=layout.viewport_rows).contains(&content_line) {
        return None;
    }
    let row = projection.rows.get(
        projection
            .viewport
            .checked_add(content_line.checked_sub(1)?)?,
    )?;
    row.target.clone()
}

/// Consume one mouse event owned by the current blocking overlay.
pub fn consume_blocking_overlay_mouse(
    state: &mut AppState,
    kind: crossterm::event::MouseEventKind,
    cols: u16,
    rows: u16,
) -> bool {
    crate::overlay_controls::consume_blocking_overlay_mouse(state, kind, cols, rows)
}

fn server_lost_confirmation(count: usize) -> (String, String, bool, bool) {
    let noun = if count == 1 { "agent" } else { "agents" };
    (
        String::from("Recover psmux Agents"),
        format!("Relaunch {count} {noun} whose psmux server was lost?"),
        false,
        false,
    )
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
                "Delete the working copy and re-clone from the configured origin? \
                 It is dirty or not on the default branch.",
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
                    "Replace it with a fresh clone? Working copy origin is {actual_repr}, expected {expected}."
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

/// Build a screen element for a component taking the shared screen props.
///
/// Every screen except the dashboard and the Terminal Manager renders from the
/// same `(state, colors, theme_name)` triple; repeating it once per screen is
/// what pushed this dispatch past the too-many-lines gate.
/// Each screen builds in a frame of its own, so the dispatch does not carry the
/// sum of every screen's props on one stack.
macro_rules! screen_element {
    ($component:ident, $snapshot:expr, $colors:expr, $theme_name:expr) => {{
        fn build(
            snapshot: &AppState,
            colors: &ThemeColors,
            theme_name: &str,
        ) -> AnyElement<'static> {
            element! {
                $component(
                    state: Some(snapshot.clone()),
                    colors: Some(colors.clone()),
                    theme_name: theme_name.to_owned(),
                )
            }
            .into_any()
        }
        build($snapshot, $colors, $theme_name)
    }};
}

/// Build the screen element for the current active screen.
#[must_use]
pub fn build_screen_element(
    snapshot: &AppState,
    colors: &ThemeColors,
    theme_name: &str,
    terminal: TerminalRenderData,
) -> AnyElement<'static> {
    match snapshot.compiled_screen() {
        Some(ScreenId::Issues) => screen_element!(IssuesScreen, snapshot, colors, theme_name),
        Some(ScreenId::Repositories) => screen_element!(SplitScreen, snapshot, colors, theme_name),
        // @plan PLAN-20260624-PR-MODE.P12
        // @requirement REQ-PR-001
        Some(ScreenId::PullRequests) => {
            screen_element!(PullRequestsScreen, snapshot, colors, theme_name)
        }
        Some(ScreenId::Actions) => screen_element!(ActionsScreen, snapshot, colors, theme_name),
        Some(ScreenId::Errors) => screen_element!(ErrorsScreen, snapshot, colors, theme_name),
        Some(ScreenId::Settings) => screen_element!(SettingsScreen, snapshot, colors, theme_name),
        None => element! {
            ProviderScreen(
                state: Some(snapshot.clone()),
                colors: colors.clone(),
                theme_name: theme_name.to_owned(),
                terminal_snapshot: terminal.snapshot,
                terminal_history_lines: terminal.history_lines,
                terminal_pane_rows: terminal.pane_rows,
                terminal_pane_cols: terminal.pane_cols,
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

fn generated_agent_modal(
    snapshot: &AppState,
    colors: &ThemeColors,
    available_rows: u16,
) -> AnyElement<'static> {
    element! {
        GeneratedAgentForm(
            state: snapshot.clone(),
            colors: colors.clone(),
            available_rows: available_rows,
        )
    }
    .into_any()
}

/// Build the modal element for the current modal state, if any.
#[must_use]
pub fn build_modal_element(
    snapshot: &AppState,
    modal: &ModalState,
    colors: &ThemeColors,
    confirm_data: Option<ConfirmModalData>,
    viewport: ModalViewport,
) -> Option<AnyElement<'static>> {
    if snapshot.active_overlay_kind() == Some(crate::workbench::OverlayKind::Help) {
        let layout = crate::overlay_controls::HostOverlayLayout::help(viewport.cols, viewport.rows);
        return Some(host_overlay_element(
            project_help(snapshot, layout.content_width),
            layout,
            colors,
            crate::overlay_controls::HELP_FOOTER,
        ));
    }
    if let Some(data) = confirm_data {
        let layout =
            crate::overlay_controls::HostOverlayLayout::confirmation(viewport.cols, viewport.rows);
        let projection = project_confirmation(
            ConfirmationContent {
                title: &data.title,
                message: &data.message,
                show_delete_work_dir: data.show_delete_work_dir,
                delete_work_dir: data.delete_work_dir,
                focus: data.confirm_focus,
            },
            layout.content_width,
        );
        return Some(host_overlay_element(
            projection,
            layout,
            colors,
            crate::overlay_controls::CONFIRMATION_FOOTER,
        ));
    }
    match modal {
        ModalState::NewRepository { .. } | ModalState::EditRepository { .. } => form_overlay(
            snapshot,
            viewport,
            colors,
            project_repository_form,
            crate::overlay_controls::REPOSITORY_FORM_FOOTER,
        ),
        ModalState::NewAgent { .. } | ModalState::EditAgent { .. } => form_overlay(
            snapshot,
            viewport,
            colors,
            crate::overlay_controls_agent_form::project_agent_form,
            crate::overlay_controls_agent_form::AGENT_FORM_FOOTER,
        ),
        ModalState::GeneratedAgent { .. } => {
            Some(generated_agent_modal(snapshot, colors, viewport.rows))
        }
        ModalState::WorkflowDispatch { .. } => {
            Some(form_modal!(WorkflowDispatchForm, snapshot, colors))
        }
        // In-app device-code auth remediation dialog (issue #244). Render-only:
        // receives the dialog state as plain data.
        ModalState::Auth { state } => Some(auth_modal_element(state, colors, snapshot)),
        ModalState::None => None,
    }
}

/// Render a provider action surface through the same closed host-control overlay shell.
#[must_use]
pub fn build_provider_overlay_element(
    projection: &crate::state::provider_view::ProviderViewProjection,
    colors: &ThemeColors,
    viewport: ModalViewport,
) -> AnyElement<'static> {
    let footer = crate::overlay_controls::provider_surface_footer(projection);
    let layout = crate::overlay_controls::HostOverlayLayout::provider(viewport.cols, viewport.rows);
    host_overlay_element(
        crate::overlay_controls::project_provider_surface(projection, layout.content_width),
        layout,
        colors,
        footer,
    )
}

/// Route a definition-backed form modal through the shared overlay shell.
fn form_overlay(
    snapshot: &AppState,
    viewport: ModalViewport,
    colors: &ThemeColors,
    project: fn(&AppState, usize) -> Option<OverlayControlProjection>,
    footer: &str,
) -> Option<AnyElement<'static>> {
    let layout = crate::overlay_controls::HostOverlayLayout::form(viewport.cols, viewport.rows);
    let projection = project(snapshot, layout.content_width)?;
    Some(host_overlay_element(projection, layout, colors, footer))
}

fn host_overlay_element(
    projection: OverlayControlProjection,
    layout: crate::overlay_controls::HostOverlayLayout,
    colors: &ThemeColors,
    footer: &str,
) -> AnyElement<'static> {
    let rows: Vec<String> = projection.rows.into_iter().map(|row| row.text).collect();
    element! {
        HostControlOverlay(
            title: projection.title,
            rows: rows,
            viewport: projection.viewport,
            viewport_rows: layout.viewport_rows,
            width: u32::from(layout.width),
            height: u32::from(layout.height),
            colors: colors.clone(),
            footer: footer.to_owned(),
        )
    }
    .into_any()
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
