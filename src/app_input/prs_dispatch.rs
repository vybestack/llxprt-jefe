//! PR-mode dispatch helpers.
//!
//! Extracted from mod.rs to keep file sizes manageable. Mirrors
//! `issues_dispatch.rs`. All `gh` I/O runs off the UI thread via
//! `spawn_gh_task_with_panic`.
//!
//! @plan PLAN-20260624-PR-MODE.P11
//! @requirement REQ-PR-009
//! @requirement REQ-PR-010
//! @requirement REQ-PR-011
//! @requirement REQ-PR-012
//! @requirement REQ-PR-013
//! @pseudocode component-004 lines 138-175
//! @pseudocode component-003 lines 176-228

use jefe::domain::RepositoryId;
use jefe::github::PrSendPayload;
use jefe::state::AppEvent;

use super::{AppStateHandle, SharedContext, apply_and_persist, gh_async, github_client};

/// Typed unavailable-context result for PR open-in-browser (REQ-PR-013).
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-012
/// @requirement REQ-PR-013
/// @pseudocode component-003 lines 216-228
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum RepoContextError {
    NoSelection,
    InvalidSlug,
}

/// Resolved context needed to open a PR in the browser (REQ-PR-012).
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-012
/// @requirement REQ-PR-013
/// @pseudocode component-003 lines 217-228
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PrOpenInBrowserInfo {
    pub scope: RepositoryId,
    pub owner: String,
    pub name: String,
    pub number: u64,
}

// ── Repo resolution helpers ───────────────────────────────────────────────

/// Resolve the GitHub owner/repo for the currently selected repository.
/// Reads from the explicit `github_repo` field (format: `"owner/repo"`).
/// Mirrors `issues_dispatch::resolve_gh_repo`.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-009
/// @requirement REQ-PR-013
/// @pseudocode component-003 lines 217-228
pub(super) fn resolve_pr_gh_repo(state: &jefe::state::AppState) -> (String, String) {
    let repo = state
        .selected_repository_index
        .and_then(|idx| state.repositories.get(idx));
    let Some(repo) = repo else {
        return (String::new(), String::new());
    };
    let gh = repo.github_repo.trim();
    if gh.is_empty() {
        return (String::new(), String::new());
    }
    let mut parts = gh.split('/');
    let owner = parts.next().map(str::trim).unwrap_or_default();
    let name = parts.next().map(str::trim).unwrap_or_default();
    if parts.next().is_none() && !owner.is_empty() && !name.is_empty() {
        return (owner.to_owned(), name.to_owned());
    }
    (String::new(), String::new())
}

/// Resolve the scope repository ID for the currently selected repository.
/// Mirrors `issues_dispatch::current_scope_repo_id`.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-009
/// @pseudocode component-003 lines 217-228
pub(super) fn current_pr_scope_repo_id(state: &jefe::state::AppState) -> RepositoryId {
    state
        .selected_repository_index
        .and_then(|idx| state.repositories.get(idx))
        .map_or_else(|| RepositoryId(String::new()), |r| r.id.clone())
}

// ── PR detail loading ─────────────────────────────────────────────────────

/// Load PR detail for the currently selected PR in the list.
/// Used by `PrListEnter` to get the full detail with comments.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-009
/// @pseudocode component-004 lines 138-146
pub(super) fn load_pr_detail_for_selection(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let Some(mut params) = pr_detail_load_params(app_state) else {
        return;
    };
    mark_pr_detail_loading(app_state, &mut params);
    if params.owner.is_empty() || params.repo.is_empty() {
        apply_and_persist(app_state, ctx, missing_pr_detail_repo_event(&params));
        return;
    }

    let panic_params = params.clone();
    gh_async::spawn_gh_task_with_panic(
        app_state,
        ctx,
        move |mut app_state, ctx| {
            let event = pr_detail_load_event(&ctx, &params);
            apply_and_persist(&mut app_state, &ctx, event);
        },
        move |mut app_state, ctx, message| {
            apply_and_persist(
                &mut app_state,
                &ctx,
                pr_detail_load_panic_event(&panic_params, message),
            );
        },
    );
}

/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-009
/// @pseudocode component-004 lines 139-145
#[derive(Clone)]
struct PrDetailLoadParams {
    scope_repo_id: RepositoryId,
    pr_number: u64,
    owner: String,
    repo: String,
    request_id: u64,
}

/// Gather detail-load params from state (returns None if no PR selected).
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-009
/// @pseudocode component-004 lines 139-145
fn pr_detail_load_params(app_state: &AppStateHandle) -> Option<PrDetailLoadParams> {
    let state = app_state.read();
    let pr_number = state
        .prs_state
        .selected_pr_index
        .and_then(|idx| state.prs_state.pull_requests.get(idx))
        .map(|pr| pr.number)?;
    let (owner, repo) = resolve_pr_gh_repo(&state);
    let params = PrDetailLoadParams {
        scope_repo_id: current_pr_scope_repo_id(&state),
        pr_number,
        owner,
        repo,
        request_id: 0,
    };
    drop(state);
    Some(params)
}

/// Mark the PR detail as loading and assign a monotonic request id.
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-009
/// @pseudocode component-004 lines 139-145
fn mark_pr_detail_loading(app_state: &mut AppStateHandle, params: &mut PrDetailLoadParams) {
    let mut state = app_state.write();
    let request_id = state.next_pr_detail_request_id();
    state.mark_pr_detail_loading(params.scope_repo_id.clone(), params.pr_number, request_id);
    drop(state);
    params.request_id = request_id;
}

/// Build the detail-loaded/failed event from the gh result (background thread).
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-009
/// @pseudocode component-004 lines 139-145
fn pr_detail_load_event(ctx: &SharedContext, params: &PrDetailLoadParams) -> AppEvent {
    let result = github_client(ctx).map(|client| {
        client.get_pull_request_detail(&params.owner, &params.repo, params.pr_number)
    });
    match result {
        Some(Ok(detail)) => AppEvent::PrDetailLoaded {
            scope_repo_id: params.scope_repo_id.clone(),
            pr_number: params.pr_number,
            request_id: params.request_id,
            detail: std::boxed::Box::new(detail),
        },
        Some(Err(error)) => AppEvent::PrDetailLoadFailed {
            scope_repo_id: params.scope_repo_id.clone(),
            pr_number: params.pr_number,
            request_id: params.request_id,
            error: error.to_string(),
        },
        None => AppEvent::PrDetailLoadFailed {
            scope_repo_id: params.scope_repo_id.clone(),
            pr_number: params.pr_number,
            request_id: params.request_id,
            error: "Application context unavailable".to_string(),
        },
    }
}

/// Build the missing-repo failure event (synchronous, no spawn).
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-013
/// @pseudocode component-004 lines 139-145
fn missing_pr_detail_repo_event(params: &PrDetailLoadParams) -> AppEvent {
    AppEvent::PrDetailLoadFailed {
        scope_repo_id: params.scope_repo_id.clone(),
        pr_number: params.pr_number,
        request_id: params.request_id,
        error: "No GitHub repository configured. Set the GitHub Repo field (owner/repo) in repository settings.".to_string(),
    }
}

/// Build the panic failure event (clears loading + delivers error).
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-NFR-001
/// @pseudocode component-004 lines 139-145
fn pr_detail_load_panic_event(params: &PrDetailLoadParams, message: String) -> AppEvent {
    AppEvent::PrDetailLoadFailed {
        scope_repo_id: params.scope_repo_id.clone(),
        pr_number: params.pr_number,
        request_id: params.request_id,
        error: format!("GitHub PR detail task panicked: {message}"),
    }
}

// ── PR preview from list (zero I/O) ───────────────────────────────────────

/// Build a lightweight PR detail preview from list data (no I/O).
/// Used for instant preview while arrowing through the PR list.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-003
/// @pseudocode component-004 lines 119-126
pub(super) fn preview_pr_from_list(app_state: &mut AppStateHandle) {
    let preview = {
        let state = app_state.read();
        state
            .prs_state
            .selected_pr_index
            .and_then(|idx| state.prs_state.pull_requests.get(idx))
            .map(|pr| {
                let (owner, repo) = resolve_pr_gh_repo(&state);
                let repo_owner_name = if owner.is_empty() || repo.is_empty() {
                    String::new()
                } else {
                    format!("{owner}/{repo}")
                };
                jefe::domain::PullRequestDetail {
                    repo_owner_name,
                    number: pr.number,
                    title: pr.title.clone(),
                    state: pr.state,
                    is_draft: pr.is_draft,
                    author_login: pr.author_login.clone(),
                    created_at: String::new(),
                    updated_at: pr.updated_at.clone(),
                    head_ref: pr.head_ref.clone(),
                    base_ref: pr.base_ref.clone(),
                    labels: pr
                        .labels_summary
                        .split(", ")
                        .filter(|s| !s.is_empty())
                        .map(String::from)
                        .collect(),
                    assignees: pr
                        .assignee_summary
                        .split(", ")
                        .filter(|s| !s.is_empty())
                        .map(String::from)
                        .collect(),
                    milestone: None,
                    body: String::new(),
                    external_url: String::new(),
                    review_decision: pr.review_decision,
                    checks_status: pr.checks_status,
                    reviews: Vec::new(),
                    checks: Vec::new(),
                    comments: Vec::new(),
                    has_more_comments: false,
                    comments_cursor: None,
                }
            })
    };

    if let Some(detail) = preview {
        let mut state = app_state.write();
        state.prs_state.pr_detail = Some(detail);
        state.prs_state.loading.detail = false;
        state.prs_state.loading.comments = false;
        state.prs_state.detail_pending = None;
        state.prs_state.comments_page_pending = None;
        state.prs_state.detail_subfocus = jefe::state::PrDetailSubfocus::Body;
        state.prs_state.detail_scroll_offset = 0;
    }
}

// ── PR send-to-agent prompt formatting ────────────────────────────────────

/// Format a `PrSendPayload` into a markdown PR prompt for the agent.
/// Mirrors `format_issue_prompt`.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-011
/// @pseudocode component-003 lines 176-187
#[must_use]
pub(super) fn format_pr_prompt(payload: &PrSendPayload) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "# Pull Request #{}: {}",
        payload.pr_number, payload.pr_title
    );
    let _ = writeln!(out);
    let _ = writeln!(out, "**Repository:** {}", payload.repository);
    let _ = writeln!(out, "**State:** {}", payload.pr_state);
    let _ = writeln!(
        out,
        "**Branch:** {} -> {}",
        payload.head_ref, payload.base_ref
    );
    if !payload.review_summary.is_empty() {
        let _ = writeln!(out, "**Reviews:** {}", payload.review_summary.join(", "));
    }
    if !payload.check_summary.is_empty() {
        let _ = writeln!(out, "**Checks:** {}", payload.check_summary.join(", "));
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "## Body");
    let _ = writeln!(out);
    let _ = writeln!(out, "{}", payload.pr_body);

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

    if !payload.pr_base_prompt.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "## Instructions");
        let _ = writeln!(out);
        let _ = writeln!(out, "{}", payload.pr_base_prompt);
    }

    out
}

// ── Open-in-browser dispatch ──────────────────────────────────────────────

/// Dispatch the open-in-browser side effect for the selected PR.
///
/// The reducer `apply_pr_open_in_browser` has ALREADY applied the "opening..."
/// notice when `PullRequests(OpenInBrowser)` was dispatched and persisted in
/// the mod.rs arm BEFORE this call. This fn resolves the selected PR's
/// scope/number and, only for a valid repo+selection, spawns
/// `GhClient::open_pull_request_in_browser` via `spawn_gh_task_with_panic`
/// (OFF the UI thread), delivering `PrOpenedInBrowser` on success and
/// `PrOpenInBrowserFailed` on Err/panic.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-012
/// @requirement REQ-PR-013
/// @pseudocode component-003 lines 190-215
/// @pseudocode component-004 lines 160-175
pub(super) fn dispatch_pr_open_in_browser(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    match pr_open_in_browser_info(app_state) {
        Ok(info) => spawn_pr_open_in_browser(app_state, ctx, info),
        Err(RepoContextError::NoSelection) => {
            // Visible notice, no spawn (REQ-PR-013: never a silent drop).
            apply_and_persist(
                app_state,
                ctx,
                AppEvent::PrShowNotice(jefe::state::ReadOnlyHintKind::NoSelectionToOpen),
            );
        }
        Err(RepoContextError::InvalidSlug) => {
            // Categorized visible error — NEVER a silent return (REQ-PR-013).
            let (scope, pr_number) = pr_open_in_browser_failure_context(app_state);
            apply_and_persist(
                app_state,
                ctx,
                AppEvent::PrOpenInBrowserFailed {
                    scope_repo_id: scope,
                    pr_number,
                    error: "Configure repository (owner/name) before opening in browser"
                        .to_string(),
                },
            );
        }
    }
}

/// Spawn the off-thread `gh pr view --web` task for a valid repo + PR.
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-012
/// @pseudocode component-004 lines 160-175
fn spawn_pr_open_in_browser(
    app_state: &AppStateHandle,
    ctx: &SharedContext,
    info: PrOpenInBrowserInfo,
) {
    let panic_info = info.clone();
    gh_async::spawn_gh_task_with_panic(
        app_state,
        ctx,
        move |mut app_state, ctx| {
            let event = pr_open_in_browser_event(&ctx, &info);
            apply_and_persist(&mut app_state, &ctx, event);
        },
        move |mut app_state, ctx, message| {
            apply_and_persist(
                &mut app_state,
                &ctx,
                AppEvent::PrOpenInBrowserFailed {
                    scope_repo_id: panic_info.scope.clone(),
                    pr_number: panic_info.number,
                    error: format!("GitHub open-in-browser task panicked: {message}"),
                },
            );
        },
    );
}

/// Resolve the scope + PR number for an `InvalidSlug` failure event.
///
/// Mirrors how `pr_open_in_browser_info` resolves these: scope from the
/// current repo id, pr_number from `selected_pr_index`→`pull_requests`.
/// Returns `(empty_id, 0)` when no selection is present (the InvalidSlug
/// path only fires when a selection exists but the slug is malformed).
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-012
/// @requirement REQ-PR-013
/// @pseudocode component-003 lines 217-228
/// @pseudocode component-004 lines 166-168
fn pr_open_in_browser_failure_context(app_state: &AppStateHandle) -> (RepositoryId, u64) {
    let state = app_state.read();
    let result = pr_open_in_browser_failure_context_from_state(&state);
    drop(state);
    result
}

/// Build the open-in-browser success/failure event from the gh result.
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-012
/// @pseudocode component-004 lines 160-175
fn pr_open_in_browser_event(ctx: &SharedContext, info: &PrOpenInBrowserInfo) -> AppEvent {
    let result = github_client(ctx)
        .map(|client| client.open_pull_request_in_browser(&info.owner, &info.name, info.number));
    match result {
        Some(Ok(())) => AppEvent::PrOpenedInBrowser {
            scope_repo_id: info.scope.clone(),
            pr_number: info.number,
        },
        Some(Err(error)) => AppEvent::PrOpenInBrowserFailed {
            scope_repo_id: info.scope.clone(),
            pr_number: info.number,
            error: error.to_string(),
        },
        None => AppEvent::PrOpenInBrowserFailed {
            scope_repo_id: info.scope.clone(),
            pr_number: info.number,
            error: "Application context unavailable".to_string(),
        },
    }
}

/// Resolve the repo/owner/name/number needed to open a PR in the browser.
///
/// Reads the selected PR number + repo slug. Returns `NoSelection` when no PR
/// is selected, `InvalidSlug` when the repo slug is missing/malformed, and
/// `Ok(info)` when both are well-formed.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-012
/// @requirement REQ-PR-013
/// @pseudocode component-003 lines 217-228
pub(super) fn pr_open_in_browser_info(
    app_state: &AppStateHandle,
) -> Result<PrOpenInBrowserInfo, RepoContextError> {
    let state = app_state.read();
    let result = pr_open_in_browser_info_from_state(&state);
    drop(state);
    result
}

/// Resolve the repo/owner/name/number from a raw `AppState` (testable without
/// `AppStateHandle`).
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-012
/// @requirement REQ-PR-013
/// @pseudocode component-003 lines 217-228
pub(super) fn pr_open_in_browser_info_from_state(
    state: &jefe::state::AppState,
) -> Result<PrOpenInBrowserInfo, RepoContextError> {
    let number = state
        .prs_state
        .selected_pr_index
        .and_then(|idx| state.prs_state.pull_requests.get(idx))
        .map(|pr| pr.number)
        .ok_or(RepoContextError::NoSelection)?;
    let (owner, name) = resolve_pr_gh_repo(state);
    let scope = current_pr_scope_repo_id(state);
    if owner.is_empty() || name.is_empty() {
        return Err(RepoContextError::InvalidSlug);
    }
    Ok(PrOpenInBrowserInfo {
        scope,
        owner,
        name,
        number,
    })
}

/// Resolve the scope + PR number for an `InvalidSlug` failure event from a raw
/// `AppState` (testable without `AppStateHandle`).
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-012
/// @requirement REQ-PR-013
/// @pseudocode component-003 lines 217-228
/// @pseudocode component-004 lines 166-168
pub(super) fn pr_open_in_browser_failure_context_from_state(
    state: &jefe::state::AppState,
) -> (RepositoryId, u64) {
    let scope = current_pr_scope_repo_id(state);
    let pr_number = state
        .prs_state
        .selected_pr_index
        .and_then(|idx| state.prs_state.pull_requests.get(idx))
        .map_or(0, |pr| pr.number);
    (scope, pr_number)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jefe::domain::{Repository, RepositoryId};
    use jefe::state::{AppEvent, AppState, PullRequestsState, ScreenMode};
    use std::path::PathBuf;

    /// @plan PLAN-20260624-PR-MODE.P11
    /// @requirement REQ-PR-012
    /// @pseudocode component-004 lines 160-175
    fn test_pr(number: u64) -> jefe::domain::PullRequest {
        use jefe::domain::{PrCheckStatus, PrState};
        jefe::domain::PullRequest {
            number,
            title: format!("PR #{number}"),
            state: PrState::Open,
            author_login: "octocat".to_string(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
            head_ref: "feature".to_string(),
            base_ref: "main".to_string(),
            is_draft: false,
            review_decision: None,
            checks_status: PrCheckStatus::None,
            assignee_summary: String::new(),
            labels_summary: String::new(),
            comment_count: 0,
        }
    }

    /// Build an AppState with a selected PR and a repository whose `github_repo`
    /// slug is malformed (empty) — triggering the InvalidSlug path.
    /// @plan PLAN-20260624-PR-MODE.P11
    /// @requirement REQ-PR-012
    /// @pseudocode component-004 lines 160-175
    fn state_with_invalid_slug() -> AppState {
        let prs_state = PullRequestsState {
            active: true,
            pull_requests: vec![test_pr(42)],
            selected_pr_index: Some(0),
            ..PullRequestsState::default()
        };
        let mut state = AppState {
            screen_mode: ScreenMode::DashboardPullRequests,
            prs_state,
            ..AppState::default()
        };
        // Repository with an EMPTY github_repo slug → InvalidSlug.
        state.repositories.push(Repository::new(
            RepositoryId("repo-1".to_string()),
            "Repo 1".to_string(),
            "repo-1".to_string(),
            PathBuf::from("/tmp/repo1"),
        ));
        state.selected_repository_index = Some(0);
        state
    }

    /// Build an AppState with a selected PR and a valid `owner/repo` slug.
    /// @plan PLAN-20260624-PR-MODE.P11
    /// @requirement REQ-PR-012
    /// @pseudocode component-004 lines 160-175
    fn state_with_valid_slug() -> AppState {
        let mut state = state_with_invalid_slug();
        state.repositories[0].github_repo = "owner/repo".to_string();
        state
    }

    /// Build an AppState with NO selected PR → NoSelection.
    /// @plan PLAN-20260624-PR-MODE.P11
    /// @requirement REQ-PR-012
    /// @pseudocode component-004 lines 160-175
    fn state_with_no_selection() -> AppState {
        let mut state = state_with_invalid_slug();
        state.prs_state.selected_pr_index = None;
        state
    }

    /// `pr_open_in_browser_info_from_state` returns `InvalidSlug` for a
    /// malformed repo slug, and the failure context carries the scope +
    /// pr_number for a categorized `PrOpenInBrowserFailed` event (never silent).
    ///
    /// @plan PLAN-20260624-PR-MODE.P11
    /// @requirement REQ-PR-012
    /// @requirement REQ-PR-013
    /// @pseudocode component-003 lines 217-228
    /// @pseudocode component-004 lines 166-168
    #[test]
    fn test_open_in_browser_invalid_slug_surfaces_error_not_silent() {
        let state = state_with_invalid_slug();

        // The info path returns InvalidSlug (NOT Ok, NOT NoSelection).
        let result = pr_open_in_browser_info_from_state(&state);
        assert!(
            matches!(result, Err(RepoContextError::InvalidSlug)),
            "malformed slug must yield InvalidSlug (got {result:?})"
        );

        // The failure context resolves scope + pr_number for the categorized event.
        let (scope, pr_number) = pr_open_in_browser_failure_context_from_state(&state);
        assert_eq!(scope, RepositoryId("repo-1".to_string()));
        assert_eq!(pr_number, 42);

        // Build the event the dispatch WOULD deliver (mirrors dispatch arm).
        let event = AppEvent::PrOpenInBrowserFailed {
            scope_repo_id: scope,
            pr_number,
            error: "Configure repository (owner/name) before opening in browser".to_string(),
        };

        // The reducer surfaces a visible error from PrOpenInBrowserFailed
        // (observable state, NOT a silent no-op).
        let after = state.apply(event);
        assert!(
            after.prs_state.error.is_some() || after.prs_state.draft_notice.is_some(),
            "PrOpenInBrowserFailed must surface a visible error/notice (got error={:?}, notice={:?})",
            after.prs_state.error,
            after.prs_state.draft_notice
        );
    }

    /// A valid slug yields `Ok(info)` — the happy path is unaffected.
    ///
    /// @plan PLAN-20260624-PR-MODE.P11
    /// @requirement REQ-PR-012
    /// @pseudocode component-003 lines 217-228
    #[test]
    fn test_open_in_browser_valid_slug_yields_ok() {
        let state = state_with_valid_slug();
        let result = pr_open_in_browser_info_from_state(&state);
        assert!(result.is_ok(), "valid slug must yield Ok");
        if let Ok(info) = result {
            assert_eq!(info.owner, "owner");
            assert_eq!(info.name, "repo");
            assert_eq!(info.number, 42);
        }
    }

    /// No selection yields `NoSelection` (not InvalidSlug, not Ok).
    ///
    /// @plan PLAN-20260624-PR-MODE.P11
    /// @requirement REQ-PR-012
    /// @requirement REQ-PR-013
    /// @pseudocode component-003 lines 217-228
    #[test]
    fn test_open_in_browser_no_selection_yields_no_selection() {
        let state = state_with_no_selection();
        let result = pr_open_in_browser_info_from_state(&state);
        assert!(
            matches!(result, Err(RepoContextError::NoSelection)),
            "no selection must yield NoSelection (got {result:?})"
        );
    }
}
