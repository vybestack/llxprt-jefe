use jefe::messages::IssuesMessage;
use jefe::state::{AppEvent, InlineState, IssueFocus};

use super::{
    AppStateHandle, SharedContext, apply_and_persist, dispatch_app_event, issues_dispatch,
    issues_list_dispatch,
};

pub(super) fn dispatch_issues_navigation(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    message: IssuesMessage,
) {
    let (focus, prev_repo_idx, prev_issue_idx) = {
        let state = app_state.read();
        (
            state.issues_state.issue_focus,
            state.selected_repository_index,
            state.issues_state.selected_issue_index(),
        )
    };

    apply_and_persist(app_state, ctx, AppEvent::from(message));
    refresh_issue_navigation(app_state, ctx, focus, prev_repo_idx, prev_issue_idx);
}

fn refresh_issue_navigation(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    focus: IssueFocus,
    prev_repo_idx: Option<usize>,
    prev_issue_idx: Option<usize>,
) {
    match focus {
        IssueFocus::RepoList => {
            refresh_repo_scope_if_changed(app_state, ctx, prev_repo_idx);
        }
        IssueFocus::IssueList => {
            refresh_issue_preview_if_changed(app_state, prev_issue_idx);
            issues_list_dispatch::load_more_issues_if_at_end(app_state, ctx);
        }
        IssueFocus::IssueDetail => {}
    }
}

fn refresh_repo_scope_if_changed(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    prev_repo_idx: Option<usize>,
) {
    let new_repo_idx = app_state.read().selected_repository_index;
    if new_repo_idx == prev_repo_idx {
        return;
    }
    reset_issue_list_for_repo_change(app_state);
    dispatch_app_event(app_state, ctx, AppEvent::RefocusIssueList);
    app_state.write().issues_state.issue_focus = IssueFocus::RepoList;
    issues_list_dispatch::dispatch_issue_list_fetch(app_state, ctx, true);
}

fn reset_issue_list_for_repo_change(app_state: &mut AppStateHandle) {
    let mut state = app_state.write();
    state.issues_state.list.clear();
    state.issues_state.issue_detail = None;
    state.issues_state.error = None;
    state.issues_state.property_editor = None;
    state.issues_state.property_mutation_pending = None;
    if state.issues_state.inline_state != InlineState::None {
        state.issues_state.draft_notice = Some("Unsent draft discarded".to_string());
    }
    state.issues_state.inline_state = InlineState::None;
    state.issues_state.mutation_pending = None;
    state.issues_state.loading.detail = false;
    state.issues_state.loading.comments = false;
    state.issues_state.detail_pending = None;
    state.issues_state.comments_page_pending = None;
    state.issues_state.agent_chooser = None;
}

fn refresh_issue_preview_if_changed(app_state: &mut AppStateHandle, prev_issue_idx: Option<usize>) {
    let new_issue_idx = app_state.read().issues_state.selected_issue_index();
    if new_issue_idx != prev_issue_idx {
        issues_dispatch::preview_issue_from_list(app_state);
    }
}
