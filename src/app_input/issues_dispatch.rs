//! Issues-mode dispatch helpers.
//!
//! Extracted from mod.rs to keep file sizes manageable.

use jefe::domain::PageToken;
use jefe::messages::IssuesMessage;
use jefe::state::AppEvent;

use super::tracker_resolver::{ResolvedTracker, resolve_tracker_outcome};
use super::{
    AppStateHandle, SharedContext, apply_and_persist, dispatch_issues_lifecycle, gh_async,
    github_client, issues_navigation::dispatch_issues_navigation,
};
use super::{
    issues_list_dispatch, issues_mutation, issues_property_edit, issues_send,
    issues_subfocus_dispatch,
};

/// Resolve the effective issue repository, returning empty components when
/// no valid target exists. User-visible callers use [`resolve_gh_repo_or_error`]
/// to retain malformed-configuration details.
///
/// @plan PLAN-20260329-ISSUES-MODE.P15
/// @requirement REQ-ISS-013
pub(super) fn resolve_gh_repo(state: &jefe::state::AppState) -> (String, String) {
    resolve_gh_repo_or_error(state).unwrap_or_default()
}

/// Resolve the effective GitHub `owner/repo` with source-aware error
/// distinction (issue #266 defect remediation).
///
/// Returns the validated `(owner, repo)` pair, or a [`MalformedRepo`] error
/// that carries the raw override and reason so the caller can surface it
/// in a user-visible message — rather than a misleading "missing GitHub
/// Repo" when a malformed override is actually the cause.
pub(super) fn resolve_gh_repo_or_error(
    state: &jefe::state::AppState,
) -> Result<(String, String), MalformedRepo> {
    let Some(repo) = state
        .selected_repository_index
        .and_then(|idx| state.repositories.get(idx))
    else {
        return Ok((String::new(), String::new()));
    };
    match resolve_tracker_outcome(repo) {
        ResolvedTracker::Resolved(target) => {
            Ok((target.owner().to_owned(), target.repo().to_owned()))
        }
        ResolvedTracker::Absent => Ok((String::new(), String::new())),
        ResolvedTracker::Malformed(error) => Err(MalformedRepo {
            message: format!("{error}"),
        }),
    }
}

/// User-visible error describing a malformed tracker override (issue #266).
pub(super) struct MalformedRepo {
    /// Human-readable message including the raw value and reason.
    pub message: String,
}

pub(super) fn current_scope_repo_id(state: &jefe::state::AppState) -> jefe::domain::RepositoryId {
    state
        .selected_repository_index
        .and_then(|idx| state.repositories.get(idx))
        .map_or_else(
            || jefe::domain::RepositoryId(String::new()),
            |r| r.id.clone(),
        )
}

/// Build a lightweight issue detail preview from list data (no I/O).
/// Used for instant preview while arrowing through the issue list.
pub(super) fn preview_issue_from_list(app_state: &mut AppStateHandle) {
    let preview = {
        let state = app_state.read();
        state
            .issues_state
            .selected_issue_index()
            .and_then(|idx| state.issues_state.issues().get(idx))
            .map(|issue| {
                let gh_repo = resolve_gh_repo(&state);
                jefe::domain::IssueDetail {
                    repo_owner_name: format!("{}/{}", gh_repo.0, gh_repo.1),
                    number: issue.number,
                    node_id: issue.node_id.clone(),
                    title: issue.title.clone(),
                    state: issue.state,
                    author_login: issue.author_login.clone(),
                    created_at: String::new(),
                    updated_at: issue.updated_at.clone(),
                    labels: issue.labels.clone(),
                    assignees: issue.assignees.clone(),
                    milestone: None,
                    body: preview_body_from_list(&issue.body),
                    external_url: String::new(),
                    comments: jefe::domain::PaginatedList::from_loaded(
                        jefe::domain::CommentDetailIdentity {
                            scope_repo_id: current_scope_repo_id(&state),
                            number: issue.number,
                        },
                        Vec::new(),
                        PageToken::Done,
                    ),
                    issue_type_name: if issue.issue_type.is_empty() {
                        None
                    } else {
                        Some(issue.issue_type.clone())
                    },
                }
            })
    };

    if let Some(detail) = preview {
        let mut state = app_state.write();
        if let Some(previous_detail) = &mut state.issues_state.issue_detail {
            previous_detail.comments.cancel_pending();
        }
        state.issues_state.issue_detail = Some(detail);
        state.issues_state.loading.detail = false;
        state.issues_state.loading.comments = false;
        state.issues_state.detail_pending = None;
        state.issues_state.detail_subfocus = jefe::state::DetailSubfocus::Body;
        state.issues_state.detail_scroll_offset = 0;
    }
}

/// Body text for instant issue previews built from lightweight list rows.
/// @plan PLAN-20260630-ISSUES-REGRESSION.P01
/// @requirement REQ-ISS-006
/// @pseudocode component-004 lines 1-5
fn preview_body_from_list(body: &str) -> String {
    if body.is_empty() {
        "Press Enter to load issue body.".to_string()
    } else {
        body.to_string()
    }
}

/// Load issue detail for the currently selected issue in the list.
/// Used by IssuesEnter to get the full detail with comments.
pub(super) fn load_issue_detail_for_selection(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let Some(mut params) = detail_load_params(app_state) else {
        return;
    };
    mark_detail_loading(app_state, &mut params);
    if params.owner.is_empty() || params.repo.is_empty() {
        let error = params
            .malformed_message
            .as_deref()
            .unwrap_or(MISSING_DETAIL_REPO_MSG);
        apply_and_persist(app_state, ctx, missing_detail_repo_event(&params, error));
        return;
    }

    let panic_params = params.clone();
    gh_async::spawn_gh_task_with_panic(
        app_state,
        ctx,
        move |mut app_state, ctx| {
            let event = detail_load_event(&ctx, params);
            // Offer the in-app auth dialog when gh is unauthenticated (issue #244).
            if let AppEvent::IssueDetailLoadFailed { error, .. } = &event
                && super::auth_remediation::offer_auth_remediation(&mut app_state, &ctx, error)
            {
                return;
            }
            apply_and_persist(&mut app_state, &ctx, event);
        },
        move |mut app_state, ctx, message| {
            apply_and_persist(
                &mut app_state,
                &ctx,
                detail_load_panic_event(&panic_params, message),
            );
        },
    );
}

/// Silently refresh issue detail for the currently selected issue (issue #175).
/// Mirrors `load_issue_detail_for_selection` but does NOT set `loading.detail`
/// (no spinner flash), preserves `detail_scroll_offset` on success, and does
/// NOT surface errors visibly on failure (delivers `IssueDetailSilentRefreshed`
/// / `IssueDetailSilentRefreshFailed`).
pub(super) fn load_issue_detail_silent_refresh(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
) {
    let Some(mut params) = issue_detail_load_params(app_state) else {
        return;
    };
    mark_detail_silent_loading(app_state, &mut params);
    if params.owner.is_empty() || params.repo.is_empty() {
        // Missing repo: silently clear the pending marker (no visible error).
        apply_and_persist(app_state, ctx, detail_silent_refresh_failed_event(&params));
        return;
    }

    let panic_params = params.clone();
    gh_async::spawn_gh_task_with_panic(
        app_state,
        ctx,
        move |mut app_state, ctx| {
            let event = detail_silent_refresh_event(&ctx, params);
            apply_and_persist(&mut app_state, &ctx, event);
        },
        move |mut app_state, ctx, _message| {
            // On panic: silently clear the pending marker (no visible error).
            apply_and_persist(
                &mut app_state,
                &ctx,
                detail_silent_refresh_failed_event(&panic_params),
            );
        },
    );
}

/// Mark issue detail as silently loading (does NOT set `loading.detail`).
fn mark_detail_silent_loading(app_state: &mut AppStateHandle, params: &mut DetailLoadParams) {
    let mut state = app_state.write();
    let request_id = state.next_issue_detail_request_id();
    state.mark_issue_detail_silent_loading(
        params.scope_repo_id.clone(),
        params.issue_number,
        request_id,
    );
    drop(state);
    params.request_id = request_id;
}

/// Build the silent-refresh detail event from the gh result.
fn detail_silent_refresh_event(ctx: &SharedContext, params: DetailLoadParams) -> AppEvent {
    let result = github_client(ctx)
        .map(|client| client.get_issue_detail(&params.owner, &params.repo, params.issue_number));
    match result {
        Some(Ok(detail)) => AppEvent::IssueDetailSilentRefreshed {
            scope_repo_id: params.scope_repo_id,
            issue_number: params.issue_number,
            request_id: params.request_id,
            detail: std::boxed::Box::new(detail),
        },
        _ => detail_silent_refresh_failed_event_owned(params),
    }
}

/// Build the silent-refresh failure event from owned params (clears pending).
fn detail_silent_refresh_failed_event_owned(params: DetailLoadParams) -> AppEvent {
    AppEvent::IssueDetailSilentRefreshFailed {
        scope_repo_id: params.scope_repo_id,
        issue_number: params.issue_number,
        request_id: params.request_id,
    }
}

/// Build the silent-refresh failure event from borrowed params (clears pending).
fn detail_silent_refresh_failed_event(params: &DetailLoadParams) -> AppEvent {
    AppEvent::IssueDetailSilentRefreshFailed {
        scope_repo_id: params.scope_repo_id.clone(),
        issue_number: params.issue_number,
        request_id: params.request_id,
    }
}

fn detail_load_params(app_state: &AppStateHandle) -> Option<DetailLoadParams> {
    let state = app_state.read();
    let issue_number = state
        .issues_state
        .selected_issue_index()
        .and_then(|idx| state.issues_state.issues().get(idx))
        .map(|issue| issue.number)?;
    let (owner, repo, malformed_message) = resolve_gh_repo_or_triple(&state);
    let params = DetailLoadParams {
        scope_repo_id: current_scope_repo_id(&state),
        issue_number,
        owner,
        repo,
        // Assigned by mark_detail_loading before the request is dispatched.
        request_id: 0,
        malformed_message,
    };
    drop(state);
    Some(params)
}

/// Resolve `(owner, repo, malformed_message)` from state.
///
/// When the tracker resolves cleanly, returns `(owner, repo, None)`. When a
/// nonblank override is malformed, returns `(empty, empty, Some(message))`
/// so the caller can surface the malformed reason instead of a misleading
/// "missing GitHub Repo" (issue #266 defect remediation).
fn resolve_gh_repo_or_triple(state: &jefe::state::AppState) -> (String, String, Option<String>) {
    match resolve_gh_repo_or_error(state) {
        Ok((owner, repo)) => (owner, repo, None),
        Err(error) => (String::new(), String::new(), Some(error.message)),
    }
}

fn mark_detail_loading(app_state: &mut AppStateHandle, params: &mut DetailLoadParams) {
    let mut state = app_state.write();
    let request_id = state.next_issue_detail_request_id();
    state.mark_issue_detail_loading_with_request_id(
        params.scope_repo_id.clone(),
        params.issue_number,
        request_id,
    );
    drop(state);
    params.request_id = request_id;
}

fn detail_load_event(ctx: &SharedContext, params: DetailLoadParams) -> AppEvent {
    let result = github_client(ctx)
        .map(|client| client.get_issue_detail(&params.owner, &params.repo, params.issue_number));
    match result {
        Some(Ok(detail)) => AppEvent::IssueDetailLoaded {
            scope_repo_id: params.scope_repo_id,
            issue_number: params.issue_number,
            request_id: params.request_id,
            detail: std::boxed::Box::new(detail),
        },
        Some(Err(error)) => AppEvent::IssueDetailLoadFailed {
            scope_repo_id: params.scope_repo_id,
            issue_number: params.issue_number,
            request_id: params.request_id,
            error: error.to_string(),
        },
        None => AppEvent::IssueDetailLoadFailed {
            scope_repo_id: params.scope_repo_id,
            issue_number: params.issue_number,
            request_id: params.request_id,
            error: "Application context unavailable".to_string(),
        },
    }
}

fn missing_detail_repo_event(params: &DetailLoadParams, error: &str) -> AppEvent {
    AppEvent::IssueDetailLoadFailed {
        scope_repo_id: params.scope_repo_id.clone(),
        issue_number: params.issue_number,
        request_id: params.request_id,
        error: error.to_string(),
    }
}

/// Default message when no tracker is configured (distinct from malformed).
const MISSING_DETAIL_REPO_MSG: &str = "No GitHub repository configured. Set the GitHub Repo field (owner/repo) in repository settings.";

fn detail_load_panic_event(params: &DetailLoadParams, message: String) -> AppEvent {
    AppEvent::IssueDetailLoadFailed {
        scope_repo_id: params.scope_repo_id.clone(),
        issue_number: params.issue_number,
        request_id: params.request_id,
        error: format!("GitHub issue detail task panicked: {message}"),
    }
}

#[derive(Clone)]
pub(super) struct DetailLoadParams {
    pub(super) scope_repo_id: jefe::domain::RepositoryId,
    pub(super) issue_number: u64,
    pub(super) owner: String,
    pub(super) repo: String,
    pub(super) request_id: u64,
    /// When the tracker override is malformed, this carries the
    /// user-visible reason so it can be surfaced instead of a misleading
    /// "missing GitHub Repo" (issue #266).
    pub(super) malformed_message: Option<String>,
}

/// Gather detail-load params from state (returns None if no issue selected).
pub(super) fn issue_detail_load_params(app_state: &AppStateHandle) -> Option<DetailLoadParams> {
    detail_load_params(app_state)
}

/// Load the next comments page when the detail view is scrolled to the bottom.
pub(super) fn load_more_comments(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let mut params = match comment_page_params(app_state) {
        CommentPageRequest::Ready(params) => params,
        CommentPageRequest::Fail(event) => {
            if let Some(event) = mark_comment_failure_pending(app_state, event) {
                apply_and_persist(app_state, ctx, event);
            }
            return;
        }
        CommentPageRequest::Skip => return,
    };

    let request_id = {
        let mut state = app_state.write();
        state.begin_issue_comment_page(
            &params.scope_repo_id,
            params.issue_number,
            params.cursor.clone(),
        )
    };
    let Some(request_id) = request_id else {
        return;
    };
    params.request_id = request_id;

    let panic_params = params.clone();
    gh_async::spawn_gh_task_with_panic(
        app_state,
        ctx,
        move |mut app_state, ctx| {
            let event = comment_page_event(&ctx, &params);
            apply_and_persist(&mut app_state, &ctx, event);
        },
        move |mut app_state, ctx, message| {
            apply_and_persist(
                &mut app_state,
                &ctx,
                AppEvent::IssueCommentsPageFailed {
                    scope_repo_id: panic_params.scope_repo_id,
                    issue_number: panic_params.issue_number,
                    request_id: panic_params.request_id,
                    request_cursor: panic_params.cursor,
                    error: format!("GitHub comments task panicked: {message}"),
                },
            );
        },
    );
}

fn mark_comment_failure_pending(
    app_state: &mut AppStateHandle,
    event: AppEvent,
) -> Option<AppEvent> {
    let AppEvent::IssueCommentsPageFailed {
        scope_repo_id,
        issue_number,
        request_cursor,
        error,
        ..
    } = event
    else {
        return None;
    };
    let request_id = app_state.write().begin_issue_comment_page(
        &scope_repo_id,
        issue_number,
        request_cursor.clone(),
    )?;
    Some(AppEvent::IssueCommentsPageFailed {
        scope_repo_id,
        issue_number,
        request_id,
        request_cursor,
        error,
    })
}

/// Return the GraphQL cursor for issue comments.
///
/// Comment pagination is cursor-only. `PageNumber` is a REST-list token and is
/// intentionally rejected here rather than translated into unrelated behavior.
fn issue_comment_cursor(token: &PageToken) -> Option<String> {
    match token {
        PageToken::Cursor(cursor) => Some(cursor.clone()),
        PageToken::PageNumber(_) | PageToken::Done => None,
    }
}

fn comment_page_params(app_state: &AppStateHandle) -> CommentPageRequest {
    let state = app_state.read();
    let Some(detail) = state.issues_state.issue_detail.as_ref() else {
        return CommentPageRequest::Skip;
    };
    if !detail.comments.has_more() || state.issues_state.loading.comments {
        return CommentPageRequest::Skip;
    }
    if state.issues_state.detail_scroll_offset < state.issues_state.max_detail_scroll_offset() {
        return CommentPageRequest::Skip;
    }
    let scope_repo_id = current_scope_repo_id(&state);
    let issue_number = detail.number;
    let requested_cursor = issue_comment_cursor(detail.comments.next_page());
    let tracker = match jefe::domain::GitHubRepoRef::parse(&detail.repo_owner_name) {
        Ok(Some(tracker)) => tracker,
        Ok(None) => {
            return CommentPageRequest::Fail(AppEvent::IssueCommentsPageFailed {
                scope_repo_id,
                issue_number,
                request_id: 0,
                request_cursor: requested_cursor,
                error: MISSING_DETAIL_REPO_MSG.to_owned(),
            });
        }
        Err(error) => {
            return CommentPageRequest::Fail(AppEvent::IssueCommentsPageFailed {
                scope_repo_id,
                issue_number,
                request_id: 0,
                request_cursor: requested_cursor,
                error: error.to_string(),
            });
        }
    };
    let params = CommentPageParams {
        scope_repo_id,
        issue_number,
        owner: tracker.owner().to_owned(),
        repo: tracker.repo().to_owned(),
        cursor: requested_cursor,
        page_size: 30,
        request_id: 0,
    };
    drop(state);
    CommentPageRequest::Ready(params)
}

fn comment_page_event(ctx: &SharedContext, params: &CommentPageParams) -> AppEvent {
    let result = github_client(ctx).map(|client| {
        client.list_comments(
            &params.owner,
            &params.repo,
            params.issue_number,
            params.cursor.as_deref(),
            params.page_size,
        )
    });

    match result {
        Some(Ok(response)) => AppEvent::IssueCommentsPageLoaded {
            scope_repo_id: params.scope_repo_id.clone(),
            issue_number: params.issue_number,
            request_id: params.request_id,
            request_cursor: params.cursor.clone(),
            comments: response.comments,
            cursor: response.cursor,
            has_more: response.has_more,
        },
        Some(Err(error)) => AppEvent::IssueCommentsPageFailed {
            scope_repo_id: params.scope_repo_id.clone(),
            issue_number: params.issue_number,
            request_id: params.request_id,
            request_cursor: params.cursor.clone(),
            error: error.to_string(),
        },
        None => AppEvent::IssueCommentsPageFailed {
            scope_repo_id: params.scope_repo_id.clone(),
            issue_number: params.issue_number,
            request_id: params.request_id,
            request_cursor: params.cursor.clone(),
            error: "Application context unavailable".to_string(),
        },
    }
}

#[derive(Clone)]
struct CommentPageParams {
    scope_repo_id: jefe::domain::RepositoryId,
    issue_number: u64,
    owner: String,
    repo: String,
    cursor: Option<String>,
    page_size: u32,
    request_id: u64,
}

enum CommentPageRequest {
    Ready(CommentPageParams),
    Fail(AppEvent),
    Skip,
}

/// Format a `SendPayload` into a markdown issue prompt for the agent.
pub(super) fn format_issue_prompt(payload: &jefe::github::SendPayload) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "# GitHub Issue #{}: {}",
        payload.issue_number, payload.issue_title
    );
    let _ = writeln!(out);
    let _ = writeln!(out, "**Repository:** {}", payload.repository);
    let _ = writeln!(out, "**State:** {}", payload.issue_state);
    if !payload.issue_labels.is_empty() {
        let _ = writeln!(out, "**Labels:** {}", payload.issue_labels.join(", "));
    }
    if !payload.issue_assignees.is_empty() {
        let _ = writeln!(out, "**Assignees:** {}", payload.issue_assignees.join(", "));
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "## Body");
    let _ = writeln!(out);
    let _ = writeln!(out, "{}", payload.issue_body);

    if let Some(comment) = &payload.focused_comment {
        let _ = writeln!(out);
        if let Some(author) = &payload.focused_comment_author {
            let _ = writeln!(out, "## Focused Comment (by @{author})");
        } else {
            let _ = writeln!(out, "## Focused Comment");
        }
        let _ = writeln!(out);
        let _ = writeln!(out, "{comment}");
    }

    if !payload.issue_base_prompt.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "## Instructions");
        let _ = writeln!(out);
        let _ = writeln!(out, "{}", payload.issue_base_prompt);
    }

    out
}

/// Dispatch Issues-mode messages that need orchestration beyond a plain reducer
/// event (navigation reloads, detail load, send-to-agent, inline submit, and
/// the close/delete lifecycle). Plain issues messages fall through to the
/// generic `AppEvent::from` arm in `dispatch_app_message`.
pub(super) fn dispatch_issues_message(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    message: IssuesMessage,
) {
    match message {
        message @ (IssuesMessage::NavigateUp
        | IssuesMessage::NavigateDown
        | IssuesMessage::NavigatePageUp(_)
        | IssuesMessage::NavigatePageDown(_)
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
            load_issue_detail_for_selection(app_state, ctx);
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
            dispatch_issue_property_message(app_state, ctx, message);
        }
        message @ (IssuesMessage::CloseIssue
        | IssuesMessage::OpenDeleteIssueConfirm
        | IssuesMessage::IssueDeleteConfirm
        | IssuesMessage::IssueDeleteCancel
        | IssuesMessage::OpenCloseReasonChooser
        | IssuesMessage::CloseReasonNavigateUp
        | IssuesMessage::CloseReasonNavigateDown
        | IssuesMessage::CloseReasonSelect
        | IssuesMessage::CloseReasonDuplicateSearchChar(_)
        | IssuesMessage::CloseReasonDuplicateSearchBackspace
        | IssuesMessage::CloseReasonDuplicateSearchNavigateUp
        | IssuesMessage::CloseReasonDuplicateSearchNavigateDown
        | IssuesMessage::CloseReasonCancel
        | IssuesMessage::CloseReasonConfirm) => {
            dispatch_issues_lifecycle(app_state, ctx, message);
        }
        message => apply_and_persist(app_state, ctx, AppEvent::from(message)),
    }
}

/// Route property-editor messages that require boundary I/O.
fn dispatch_issue_property_message(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    message: IssuesMessage,
) {
    match message {
        IssuesMessage::OpenPropertyEditor { kind } => {
            apply_and_persist(app_state, ctx, AppEvent::IssueOpenPropertyEditor { kind });
            let should_load_options = {
                let state = app_state.read();
                state
                    .issues_state
                    .property_editor
                    .as_ref()
                    .is_some_and(|editor| {
                        matches!(
                            editor.kind,
                            jefe::state::IssuePropertyKind::Labels
                                | jefe::state::IssuePropertyKind::Assignees
                                | jefe::state::IssuePropertyKind::Milestone
                                | jefe::state::IssuePropertyKind::Type
                        )
                    })
            };
            if should_load_options {
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

/// Start a coalesced post-mutation refresh once earlier requests have settled.
pub(super) fn resume_issue_post_mutation_refresh(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
) {
    let ready = {
        let state = app_state.read();
        state.screen_mode == jefe::state::ScreenMode::DashboardIssues
            && state.issue_post_mutation_refresh_ready()
    };
    if !ready {
        return;
    }
    apply_and_persist(app_state, ctx, AppEvent::IssuePostMutationRefreshStarted);
    issues_list_dispatch::request_issue_list_silent_refresh(app_state, ctx);
    load_issue_detail_silent_refresh(app_state, ctx);
}

#[cfg(test)]
mod tests {
    use super::{issue_comment_cursor, preview_body_from_list};
    use jefe::domain::PageToken;

    #[test]
    fn issue_comment_cursor_rejects_rest_page_tokens() {
        assert_eq!(issue_comment_cursor(&PageToken::PageNumber(2)), None);
    }

    #[test]
    fn issue_comment_cursor_extracts_graphql_cursor() {
        assert_eq!(
            issue_comment_cursor(&PageToken::Cursor("next".to_string())),
            Some("next".to_string())
        );
    }

    #[test]
    fn empty_list_preview_body_prompts_for_detail_load() {
        assert_eq!(
            preview_body_from_list(""),
            "Press Enter to load issue body."
        );
    }

    #[test]
    fn populated_list_preview_body_is_preserved() {
        assert_eq!(preview_body_from_list("existing body"), "existing body");
    }
}
