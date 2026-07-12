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

use super::{
    AppStateHandle, SharedContext, apply_and_persist, dispatch_app_event, gh_async, github_client,
};

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
            // Offer the in-app auth dialog when gh is unauthenticated (issue #244).
            if let AppEvent::PrDetailLoadFailed { error, .. } = &event {
                if super::auth_remediation::offer_auth_remediation(&mut app_state, &ctx, error) {
                    return;
                }
            }
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

/// Silently refresh PR detail for the currently selected PR (issue #128).
/// Lives in `prs_orchestration.rs` (re-exported here for the dispatch chain) to
/// keep this file under the architecture boundary line limit.
///
/// @requirement issue #128
pub(super) use super::prs_orchestration::load_pr_detail_silent_refresh;

/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-009
/// @pseudocode component-004 lines 139-145
#[derive(Clone)]
pub(super) struct PrDetailLoadParams {
    pub(super) scope_repo_id: RepositoryId,
    pub(super) pr_number: u64,
    pub(super) owner: String,
    pub(super) repo: String,
    pub(super) request_id: u64,
}

/// Gather detail-load params from state (returns None if no PR selected).
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-009
/// @pseudocode component-004 lines 139-145
pub(super) fn pr_detail_load_params(app_state: &AppStateHandle) -> Option<PrDetailLoadParams> {
    let state = app_state.read();
    let pr_number = state
        .prs_state
        .selected_pr_index()
        .and_then(|idx| state.prs_state.pull_requests().get(idx))
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

/// Check whether the currently-selected PR still matches `pr_number`.
///
/// Used by `preview_pr_from_list` to close the read-then-write TOCTOU window:
/// after building a preview under a read lock and dropping it, the write lock
/// re-validates that the selection has not changed before applying the
/// (potentially stale) preview.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-003
/// @pseudocode component-004 lines 119-126
pub(super) fn selected_pr_still_matches(
    state: &jefe::state::AppState,
    scope_repo_id: &RepositoryId,
    pr_number: u64,
) -> bool {
    if &current_pr_scope_repo_id(state) != scope_repo_id {
        return false;
    }
    state
        .prs_state
        .selected_pr_index()
        .and_then(|idx| state.prs_state.pull_requests().get(idx))
        .is_some_and(|pr| pr.number == pr_number)
}

/// Build a `(pr_number, PullRequestDetail)` preview from the selected list PR
/// (zero I/O). Used for instant preview while arrowing through the PR list;
/// extracted so `preview_pr_from_list` stays under the per-function line limit.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-003
/// @pseudocode component-004 lines 119-126
fn build_pr_preview_for_selection(
    state: &jefe::state::AppState,
) -> Option<(RepositoryId, u64, jefe::domain::PullRequestDetail)> {
    let scope_repo_id = current_pr_scope_repo_id(state);
    let pr = state
        .prs_state
        .selected_pr_index()
        .and_then(|idx| state.prs_state.pull_requests().get(idx))?;
    let (owner, repo) = resolve_pr_gh_repo(state);
    let repo_owner_name = if owner.is_empty() || repo.is_empty() {
        String::new()
    } else {
        format!("{owner}/{repo}")
    };
    let detail = jefe::domain::PullRequestDetail {
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
        mergeable: None,
        merge_state_status: None,
    };
    Some((scope_repo_id, pr.number, detail))
}

/// Build a lightweight PR detail preview from list data (no I/O).
/// Used for instant preview while arrowing through the PR list.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-003
/// @pseudocode component-004 lines 119-126
pub(super) fn preview_pr_from_list(app_state: &mut AppStateHandle) {
    // Capture (pr_number, preview) under the READ lock, then drop it.
    let preview = {
        let state = app_state.read();
        build_pr_preview_for_selection(&state)
    };

    if let Some((preview_scope_repo_id, preview_pr_number, detail)) = preview {
        let mut state = app_state.write();
        // TOCTOU re-validation: between the read lock above and this write lock,
        // the selection could have changed. Only apply the preview if the
        // selection STILL points at the same repository AND PR number the
        // preview was built for — a different repo with the same PR number must
        // not receive another repo's stale preview.
        if !selected_pr_still_matches(&state, &preview_scope_repo_id, preview_pr_number) {
            return;
        }
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

/// Write an UNTRUSTED content block between BEGIN/END markers, prefixing every
/// line with `> ` so the content cannot emit a literal closing-delimiter line
/// and escape the block to impersonate prompt instructions (MED-7).
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-011
/// @pseudocode component-003 lines 176-187
fn write_untrusted_block(out: &mut String, label: &str, content: &str) {
    use std::fmt::Write;
    let _ = writeln!(out, "----- BEGIN UNTRUSTED {label} -----");
    for line in content.lines() {
        let _ = writeln!(out, "> {line}");
    }
    let _ = writeln!(out, "----- END UNTRUSTED {label} -----");
}

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
    // The PR body is UNTRUSTED (authored by an arbitrary GitHub user). Wrap it
    // in clear BEGIN/END delimiters so a malicious body containing fake
    // `## Instructions` headings or code fences cannot escape into the real
    // Instructions section or impersonate prompt directives (MED-7).
    let _ = writeln!(out, "## Body");
    let _ = writeln!(out);
    write_untrusted_block(&mut out, "PR BODY", &payload.pr_body);

    if let Some(comment) = &payload.focused_comment {
        let _ = writeln!(out);
        if let Some(author) = &payload.focused_comment_author {
            let _ = writeln!(out, "## Focused Comment (by @{author})");
        } else {
            let _ = writeln!(out, "## Focused Comment");
        }
        let _ = writeln!(out);
        // The focused comment is also UNTRUSTED user content — fence it so it
        // cannot inject prompt instructions (MED-7).
        write_untrusted_block(&mut out, "COMMENT", comment);
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
        .selected_pr_index()
        .and_then(|idx| state.prs_state.pull_requests().get(idx))
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
        .selected_pr_index()
        .and_then(|idx| state.prs_state.pull_requests().get(idx))
        .map_or(0, |pr| pr.number);
    (scope, pr_number)
}

// ── In-app merge dispatch (issue #92) ─────────────────────────────────────

/// Resolved context needed to merge a PR (mirrors `PrOpenInBrowserInfo`).
///
/// @requirement REQ-PR-009
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PrMergeInfo {
    pub scope: RepositoryId,
    pub owner: String,
    pub name: String,
    pub number: u64,
    pub mutation_id: u64,
    pub method: jefe::domain::MergeMethod,
}

/// Resolve the merge context from the pending merge mutation in state.
///
/// Returns `Ok(info)` when a merge mutation is pending with a valid repo slug,
/// `Err(RepoContextError::InvalidSlug)` when the slug is malformed, and
/// `Err(RepoContextError::NoSelection)` when no mutation is pending.
///
/// @requirement REQ-PR-009
pub(super) fn pr_merge_info_from_state(
    state: &jefe::state::AppState,
) -> Result<PrMergeInfo, RepoContextError> {
    let pending = state
        .prs_state
        .merge_mutation_pending
        .as_ref()
        .ok_or(RepoContextError::NoSelection)?;
    let (owner, name) = resolve_pr_gh_repo(state);
    if owner.is_empty() || name.is_empty() {
        return Err(RepoContextError::InvalidSlug);
    }
    Ok(PrMergeInfo {
        scope: pending.scope_repo_id.clone(),
        owner,
        name,
        number: pending.pr_number,
        mutation_id: pending.mutation_id,
        method: pending.method,
    })
}

/// Dispatch the merge side effect for a confirmed merge mutation.
///
/// Reads `merge_mutation_pending` from state, resolves the repo/PR/method,
/// and spawns `GhClient::merge_pull_request` OFF the UI thread via
/// `spawn_gh_task_with_panic`, delivering `PrMerged` on success and
/// `PrMergeFailed` on Err/panic.
///
/// @requirement REQ-PR-009
pub(super) fn dispatch_pr_merge(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let info = {
        let state = app_state.read();
        pr_merge_info_from_state(&state)
    };
    match info {
        Ok(info) => spawn_pr_merge(app_state, ctx, info),
        Err(RepoContextError::NoSelection) => {}
        Err(RepoContextError::InvalidSlug) => {
            let (scope, pr_number, mutation_id) = {
                let state = app_state.read();
                let pending = state.prs_state.merge_mutation_pending.as_ref();
                let scope = pending.map_or_else(
                    || current_pr_scope_repo_id(&state),
                    |p| p.scope_repo_id.clone(),
                );
                let pr_number = pending.map_or(0, |p| p.pr_number);
                let mutation_id = pending.map_or(0, |p| p.mutation_id);
                drop(state);
                (scope, pr_number, mutation_id)
            };
            apply_and_persist(
                app_state,
                ctx,
                AppEvent::PrMergeFailed {
                    scope_repo_id: scope,
                    pr_number,
                    mutation_id,
                    error: "Configure repository (owner/name) before merging".to_string(),
                },
            );
        }
    }
}

/// Spawn the off-thread `gh pr merge` task for a valid repo + PR + method.
///
/// @requirement REQ-PR-009
fn spawn_pr_merge(app_state: &AppStateHandle, ctx: &SharedContext, info: PrMergeInfo) {
    let panic_info = info.clone();
    gh_async::spawn_gh_task_with_panic(
        app_state,
        ctx,
        move |mut app_state, ctx| {
            let event = pr_merge_event(&ctx, &info);
            // Route the merge result through the full dispatch chain so that a
            // successful `PrMerged` hits the `PullRequestsMessage::Merged` arm
            // and triggers the post-mutation list + detail reload (issue #128).
            // A `PrMergeFailed` outcome is converted to a message but does NOT
            // trigger a reload (it lacks the `Merged`/`CommentCreated` markers).
            dispatch_app_event(&mut app_state, &ctx, event);
        },
        move |mut app_state, ctx, message| {
            apply_and_persist(
                &mut app_state,
                &ctx,
                AppEvent::PrMergeFailed {
                    scope_repo_id: panic_info.scope.clone(),
                    pr_number: panic_info.number,
                    mutation_id: panic_info.mutation_id,
                    error: format!("GitHub merge task panicked: {message}"),
                },
            );
        },
    );
}

/// Build the merge success/failure event from the gh result.
///
/// @requirement REQ-PR-009
fn pr_merge_event(ctx: &SharedContext, info: &PrMergeInfo) -> AppEvent {
    let result = github_client(ctx)
        .map(|client| client.merge_pull_request(&info.owner, &info.name, info.number, info.method));
    match result {
        Some(Ok(())) => AppEvent::PrMerged {
            scope_repo_id: info.scope.clone(),
            pr_number: info.number,
            method: info.method,
        },
        Some(Err(error)) => AppEvent::PrMergeFailed {
            scope_repo_id: info.scope.clone(),
            pr_number: info.number,
            mutation_id: info.mutation_id,
            error: error.to_string(),
        },
        None => AppEvent::PrMergeFailed {
            scope_repo_id: info.scope.clone(),
            pr_number: info.number,
            mutation_id: info.mutation_id,
            error: "Application context unavailable".to_string(),
        },
    }
}

/// Dispatch the merge-methods fetch when the chooser opens.
///
/// Resolves the repo owner/name from state and spawns
/// `GhClient::get_repo_merge_methods` OFF the UI thread, delivering
/// `PrMergeMethodsLoaded` on success. On failure, nothing is delivered — the
/// chooser treats `allowed_methods: None` as "all available" (graceful
/// degradation).
///
/// @requirement REQ-PR-009
pub(super) fn dispatch_pr_merge_methods_load(app_state: &AppStateHandle, ctx: &SharedContext) {
    let info = {
        let state = app_state.read();
        let pr_number = state.prs_state.pr_detail.as_ref().map_or(0, |d| d.number);
        let (owner, name) = resolve_pr_gh_repo(&state);
        let scope = current_pr_scope_repo_id(&state);
        drop(state);
        if owner.is_empty() || name.is_empty() {
            None
        } else {
            Some((scope, owner, name, pr_number))
        }
    };
    let Some((scope, owner, name, pr_number)) = info else {
        return;
    };
    gh_async::spawn_gh_task_with_panic(
        app_state,
        ctx,
        move |mut app_state, ctx| {
            if let Some(event) = pr_merge_methods_event(&ctx, &scope, &owner, &name, pr_number) {
                apply_and_persist(&mut app_state, &ctx, event);
            }
        },
        // On panic: deliver nothing (chooser stays in "all available" mode).
        move |_app_state, _ctx, _message| {},
    );
}

/// Build the merge-methods-loaded event, returning `None` on failure so the
/// chooser keeps `allowed_methods: None` (meaning "all available") rather than
/// collapsing to an empty list that disables every method.
///
/// @requirement REQ-PR-009
fn pr_merge_methods_event(
    ctx: &SharedContext,
    scope: &RepositoryId,
    owner: &str,
    name: &str,
    pr_number: u64,
) -> Option<AppEvent> {
    let methods = github_client(ctx)?
        .get_repo_merge_methods(owner, name)
        .ok()?;
    Some(AppEvent::PrMergeMethodsLoaded {
        scope_repo_id: scope.clone(),
        pr_number,
        allowed_methods: methods,
    })
}
