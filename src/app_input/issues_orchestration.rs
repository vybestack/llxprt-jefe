//! Issues-mode dispatch routing helpers (extracted from `mod.rs`).
//!
//! Mirrors `prs_orchestration.rs`: moves the `route_issues_message` and
//! `route_issues_property` helpers out of `mod.rs` to keep that file under
//! the source-file-size limit (issue #175).

use jefe::messages::IssuesMessage;
use jefe::state::AppEvent;

use super::{
    AppStateHandle, SharedContext, apply_and_persist, dispatch_issues_navigation, issues_dispatch,
    issues_list_dispatch, issues_mutation, issues_property_edit, issues_send,
    issues_subfocus_dispatch,
};

/// Route an `IssuesMessage` to its dispatch helper (or reducer).
pub(super) fn route_issues_message(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    message: IssuesMessage,
) {
    match message {
        message @ (IssuesMessage::NavigateUp
        | IssuesMessage::NavigateDown
        | IssuesMessage::NavigatePageUp
        | IssuesMessage::NavigatePageDown
        | IssuesMessage::NavigateHome
        | IssuesMessage::NavigateEnd) => {
            dispatch_issues_navigation(app_state, ctx, message);
        }
        message @ (IssuesMessage::EnterMode
        | IssuesMessage::RefocusList
        | IssuesMessage::ApplyFilter
        | IssuesMessage::ClearFilter
        | IssuesMessage::ApplySearch) => {
            issues_list_dispatch::dispatch_issue_list_reload(app_state, ctx, message);
        }
        IssuesMessage::Enter => {
            apply_and_persist(app_state, ctx, AppEvent::IssuesEnter);
            issues_dispatch::load_issue_detail_for_selection(app_state, ctx);
        }
        message @ (IssuesMessage::ScrollDetailDown
        | IssuesMessage::ScrollDetailPageDown
        | IssuesMessage::DetailSubfocusNext
        | IssuesMessage::DetailSubfocusPrev) => {
            issues_subfocus_dispatch::dispatch_issues_detail_scroll_or_subfocus(
                app_state, ctx, message,
            );
        }
        IssuesMessage::AgentChooserConfirm => {
            issues_send::dispatch_agent_chooser_confirm(app_state, ctx);
        }
        IssuesMessage::InlineSubmit => {
            issues_mutation::handle_inline_submit(app_state, ctx);
        }
        message @ (IssuesMessage::OpenPropertyEditor { .. }
        | IssuesMessage::PropertyEditorConfirm
        | IssuesMessage::PropertyEditSucceeded { .. }) => {
            route_issues_property(app_state, ctx, message);
        }
        message => apply_and_persist(app_state, ctx, AppEvent::from(message)),
    }
}

/// Request a silent background refresh of both the issue list and the issue
/// detail after a property edit succeeds (issue #175). Neither load sets the
/// visible loading/error flags, so there is no spinner flash and no loss of
/// selection, scroll offset, or filter state.
pub(super) fn request_issue_background_refresh(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
) {
    let should_refresh = {
        let state = app_state.read();
        state.screen_mode == jefe::state::ScreenMode::DashboardIssues
            && state.issues_state.list_reload_pending.is_none()
            && state.issues_state.list_page_pending.is_none()
            && state.issues_state.detail_pending.is_none()
    };
    if should_refresh {
        issues_list_dispatch::request_issue_list_silent_refresh(app_state, ctx);
        issues_dispatch::load_issue_detail_silent_refresh(app_state, ctx);
    }
}

/// Whether a property kind requires a background fetch of repo options.
fn needs_background_options(kind: jefe::state::IssuePropertyKind) -> bool {
    use jefe::state::IssuePropertyKind;
    matches!(
        kind,
        IssuePropertyKind::Labels
            | IssuePropertyKind::Assignees
            | IssuePropertyKind::Milestone
            | IssuePropertyKind::Type
    )
}

/// Route property-editor issue messages (issue #175).
fn route_issues_property(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    message: IssuesMessage,
) {
    match message {
        IssuesMessage::OpenPropertyEditor { kind } => {
            apply_and_persist(app_state, ctx, AppEvent::IssueOpenPropertyEditor { kind });
            let needs_options = {
                let state = app_state.read();
                state
                    .issues_state
                    .property_editor
                    .as_ref()
                    .is_some_and(|e| needs_background_options(e.kind))
            };
            if needs_options {
                issues_property_edit::handle_issue_property_options_load(app_state, ctx);
            }
        }
        IssuesMessage::PropertyEditorConfirm => {
            issues_property_edit::handle_issue_property_confirm(app_state, ctx);
        }
        IssuesMessage::PropertyEditSucceeded { .. } => {
            issues_property_edit::dispatch_issue_property_post_mutation(
                app_state,
                ctx,
                AppEvent::from(message),
            );
        }
        other => apply_and_persist(app_state, ctx, AppEvent::from(other)),
    }
}
