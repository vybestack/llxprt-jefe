//! PR-mode dispatch helpers.
//!
//! Extracted from mod.rs to keep file sizes manageable. Mirrors
//! `issues_dispatch.rs`. All `gh` I/O runs off the UI thread via
//! `gh_async::spawn_gh_work`.
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
    /// A nonblank tracker override is malformed (issue #266). Carries the
    /// typed message so the caller can surface it instead of a generic
    /// "missing GitHub Repo".
    Malformed(String),
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

#[cfg(test)]
pub(super) fn resolve_pr_gh_repo(state: &jefe::state::AppState) -> (String, String) {
    resolve_pr_gh_repo_or_error(state).unwrap_or_default()
}

pub(super) fn resolve_pr_gh_repo_or_error(
    state: &jefe::state::AppState,
) -> Result<(String, String), MalformedPrRepo> {
    let Some(repo) = state
        .selected_repository_index
        .and_then(|idx| state.repositories.get(idx))
    else {
        return Ok((String::new(), String::new()));
    };
    match super::tracker_resolver::resolve_tracker_outcome(repo) {
        super::tracker_resolver::ResolvedTracker::Resolved(target) => {
            Ok((target.owner().to_owned(), target.repo().to_owned()))
        }
        super::tracker_resolver::ResolvedTracker::Absent => Ok((String::new(), String::new())),
        super::tracker_resolver::ResolvedTracker::Malformed(error) => Err(MalformedPrRepo {
            message: error.to_string(),
        }),
    }
}
/// Typed malformed-tracker error for PR dispatch paths (issue #266).
///
/// Carries the user-visible parse-error message so PR detail, list, and
/// preview operations can surface the specific malformed-configuration
/// reason instead of a generic "missing GitHub Repo" message.
pub(super) struct MalformedPrRepo {
    pub message: String,
}

pub(super) fn current_pr_scope_repo_id(state: &jefe::state::AppState) -> RepositoryId {
    state
        .selected_repository_index
        .and_then(|idx| state.repositories.get(idx))
        .map_or_else(|| RepositoryId(String::new()), |r| r.id.clone())
}

// ── PR detail loading ─────────────────────────────────────────────────────

enum PrDetailLoadCompletion {
    None,
    OpenAgentChooser(Vec<jefe::domain::AgentChooserGitMetadata>),
}

/// Load PR detail for the currently selected PR in the list.
/// Used by `PrListEnter` to get the full detail with comments.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-009
/// @pseudocode component-004 lines 138-146
pub(super) fn load_pr_detail_for_selection(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    load_pr_detail(app_state, ctx, PrDetailLoadCompletion::None);
}

pub(super) fn load_pr_detail_then_open_agent_chooser(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    metadata: Vec<jefe::domain::AgentChooserGitMetadata>,
) {
    load_pr_detail(
        app_state,
        ctx,
        PrDetailLoadCompletion::OpenAgentChooser(metadata),
    );
}

fn load_pr_detail(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    completion: PrDetailLoadCompletion,
) {
    let Some(mut params) = pr_detail_load_params(app_state) else {
        return;
    };
    if !begin_pr_detail_loading(app_state, ctx, &mut params, &completion) {
        return;
    }
    if params.owner.is_empty() || params.repo.is_empty() {
        let error = params
            .malformed_message
            .as_deref()
            .unwrap_or(MISSING_PR_DETAIL_REPO_MSG);
        apply_and_persist(app_state, ctx, missing_pr_detail_repo_event(&params, error));
        return;
    }

    let Some(deliveries) =
        gh_async::delivery_handle_or_report(app_state, ctx, detail_load_abandoned(params.clone()))
    else {
        return;
    };
    let panic_params = params.clone();
    let ready_params = params.clone();
    gh_async::spawn_gh_work(
        &deliveries,
        ctx,
        move |ctx| pr_detail_load_event(ctx, &params),
        pr_detail_delivery(completion, ready_params),
        detail_load_abandoned(panic_params),
    );
}

fn pr_detail_delivery(
    completion: PrDetailLoadCompletion,
    ready_params: PrDetailLoadParams,
) -> impl FnOnce(&mut AppStateHandle, &SharedContext, AppEvent) {
    move |app_state, ctx, event| {
        let failure_is_current = match &event {
            AppEvent::PrDetailLoadFailed {
                scope_repo_id,
                pr_number,
                request_id,
                ..
            } => {
                let state = app_state.read();
                match &completion {
                    PrDetailLoadCompletion::None => {
                        state.pr_detail_request_is_current(scope_repo_id, *pr_number, *request_id)
                    }
                    PrDetailLoadCompletion::OpenAgentChooser(_) => state
                        .pr_list_send_request_is_current(scope_repo_id, *pr_number, *request_id),
                }
            }
            _ => false,
        };
        let auth_error = match &event {
            AppEvent::PrDetailLoadFailed { error, .. } if failure_is_current => Some(error.clone()),
            _ => None,
        };
        let detail_loaded = matches!(&event, AppEvent::PrDetailLoaded { .. });
        let event = if auth_error.as_ref().is_some_and(|error| {
            super::auth_remediation::should_offer_auth_remediation(error, app_state)
        }) {
            AppEvent::PrDetailAuthRequired(
                ready_params.scope_repo_id.clone(),
                ready_params.pr_number,
                ready_params.request_id,
            )
        } else {
            event
        };
        apply_and_persist(app_state, ctx, event);
        if detail_loaded && matches!(completion, PrDetailLoadCompletion::OpenAgentChooser(_)) {
            apply_and_persist(
                app_state,
                ctx,
                AppEvent::PrListSendDetailReady {
                    scope_repo_id: ready_params.scope_repo_id,
                    pr_number: ready_params.pr_number,
                    request_id: ready_params.request_id,
                },
            );
        }
        if let Some(error) = auth_error {
            super::auth_remediation::offer_auth_remediation(app_state, ctx, &error);
        }
    }
}

/// Report an abandoned PR detail load so the pending marker is cleared.
fn detail_load_abandoned(
    params: PrDetailLoadParams,
) -> impl FnOnce(&mut AppStateHandle, &SharedContext, String) {
    move |app_state, ctx, message| {
        apply_and_persist(app_state, ctx, pr_detail_load_panic_event(&params, message));
    }
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
    pub(super) malformed_message: Option<String>,
}

fn resolve_pr_gh_repo_or_triple(state: &jefe::state::AppState) -> (String, String, Option<String>) {
    match resolve_pr_gh_repo_or_error(state) {
        Ok((owner, repo)) => (owner, repo, None),
        Err(error) => (String::new(), String::new(), Some(error.message)),
    }
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
    let (owner, repo, malformed_message) = resolve_pr_gh_repo_or_triple(&state);
    let params = PrDetailLoadParams {
        scope_repo_id: current_pr_scope_repo_id(&state),
        pr_number,
        owner,
        repo,
        request_id: 0,
        malformed_message,
    };
    drop(state);
    Some(params)
}

/// Mark the PR detail as loading and assign a monotonic request id.
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-009
/// @pseudocode component-004 lines 139-145
fn begin_pr_detail_loading(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    params: &mut PrDetailLoadParams,
    completion: &PrDetailLoadCompletion,
) -> bool {
    match completion {
        PrDetailLoadCompletion::None => {
            let mut state = app_state.write();
            let request_id = state.next_pr_detail_request_id();
            state.mark_pr_detail_loading(
                params.scope_repo_id.clone(),
                params.pr_number,
                request_id,
            );
            drop(state);
            params.request_id = request_id;
            true
        }
        PrDetailLoadCompletion::OpenAgentChooser(metadata) => {
            apply_and_persist(
                app_state,
                ctx,
                AppEvent::BeginPrListSendDetail(metadata.clone()),
            );
            let request_id = app_state
                .read()
                .prs_state
                .list_send_pending
                .as_ref()
                .filter(|pending| {
                    pending.scope_repo_id == params.scope_repo_id
                        && pending.pr_number == params.pr_number
                })
                .map(|pending| pending.request_id);
            let Some(request_id) = request_id else {
                return false;
            };
            params.request_id = request_id;
            true
        }
    }
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
fn missing_pr_detail_repo_event(params: &PrDetailLoadParams, error: &str) -> AppEvent {
    AppEvent::PrDetailLoadFailed {
        scope_repo_id: params.scope_repo_id.clone(),
        pr_number: params.pr_number,
        request_id: params.request_id,
        error: error.to_string(),
    }
}

const MISSING_PR_DETAIL_REPO_MSG: &str = "No GitHub repository configured. Set the GitHub Repo field (owner/repo) in repository settings.";

/// Build the panic failure event (clears loading + delivers error).
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-NFR-001
/// @pseudocode component-004 lines 139-145
fn pr_detail_load_panic_event(params: &PrDetailLoadParams, message: String) -> AppEvent {
    AppEvent::PrDetailLoadFailed {
        scope_repo_id: params.scope_repo_id.clone(),
        pr_number: params.pr_number,
        request_id: params.request_id,
        error: format!("GitHub PR detail request abandoned: {message}"),
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
pub(super) fn build_pr_preview_for_selection(
    state: &jefe::state::AppState,
) -> Result<Option<(RepositoryId, u64, jefe::domain::PullRequestDetail)>, MalformedPrRepo> {
    let scope_repo_id = current_pr_scope_repo_id(state);
    let Some(pr) = state
        .prs_state
        .selected_pr_index()
        .and_then(|idx| state.prs_state.pull_requests().get(idx))
    else {
        return Ok(None);
    };
    let (owner, repo) = resolve_pr_gh_repo_or_error(state)?;
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
        head_sha: pr.head_sha.clone(),
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
        comments: jefe::domain::PaginatedList::from_loaded(
            jefe::domain::CommentDetailIdentity {
                scope_repo_id: jefe::domain::RepositoryId::default(),
                number: pr.number,
            },
            Vec::new(),
            jefe::domain::PageToken::from_cursor(None, false),
        ),
        mergeable: pr.mergeable,
        merge_state_status: None,
    };
    Ok(Some((scope_repo_id, pr.number, detail)))
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

    let (preview_scope_repo_id, preview_pr_number, detail) = match preview {
        Ok(Some(preview)) => preview,
        Ok(None) => return,
        Err(error) => {
            let mut state = app_state.write();
            jefe::state::transition::commit_pure_site(
                &mut state,
                AppEvent::CancelPrListSendDetail.into(),
            );
            state.prs_state.error = Some(error.message);
            state.prs_state.loading.detail = false;
            state.prs_state.loading.comments = false;
            state.prs_state.pr_detail = None;
            state.prs_state.detail_pending = None;
            drop(state);
            return;
        }
    };
    {
        let mut state = app_state.write();
        // TOCTOU re-validation: between the read lock above and this write lock,
        // the selection could have changed. Only apply the preview if the
        // selection STILL points at the same repository AND PR number the
        // preview was built for — a different repo with the same PR number must
        // not receive another repo's stale preview.
        if !selected_pr_still_matches(&state, &preview_scope_repo_id, preview_pr_number) {
            return;
        }
        jefe::state::transition::commit_pure_site(
            &mut state,
            AppEvent::CancelPrListSendDetail.into(),
        );
        if let Some(previous_detail) = &mut state.prs_state.pr_detail {
            previous_detail.comments.cancel_pending();
        }
        state.prs_state.pr_detail = Some(detail);
        state.prs_state.error = None;
        state.prs_state.loading.detail = false;
        state.prs_state.loading.comments = false;
        state.prs_state.detail_pending = None;
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

    // Build the gh fetch command once for compaction of large bodies/comments
    // (issue #409): the agent runs in the checked-out repo with gh available.
    let fetch_cmd = format!(
        "gh pr view {} --repo {} --comments",
        payload.pr_number, payload.repository
    );

    // The PR body is UNTRUSTED (authored by an arbitrary GitHub user). Wrap it
    // in clear BEGIN/END delimiters so a malicious body containing fake
    // `## Instructions` headings or code fences cannot escape into the real
    // Instructions section or impersonate prompt directives (MED-7).
    let _ = writeln!(out, "## Body");
    let _ = writeln!(out);
    let compacted_body = super::fresh_prompt::compact_prompt_content(&payload.pr_body, &fetch_cmd);
    write_untrusted_block(&mut out, "PR BODY", &compacted_body);

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
        let compacted_comment = super::fresh_prompt::compact_prompt_content(comment, &fetch_cmd);
        write_untrusted_block(&mut out, "COMMENT", &compacted_comment);
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
/// `GhClient::open_pull_request_in_browser` via `gh_async::spawn_gh_work`
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
        Err(RepoContextError::Malformed(message)) => {
            // Typed malformed reason surfaced visibly (issue #266).
            let (scope, pr_number) = pr_open_in_browser_failure_context(app_state);
            apply_and_persist(
                app_state,
                ctx,
                AppEvent::PrOpenInBrowserFailed {
                    scope_repo_id: scope,
                    pr_number,
                    error: message,
                },
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
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    info: PrOpenInBrowserInfo,
) {
    let Some(deliveries) = gh_async::delivery_handle_or_report(
        app_state,
        ctx,
        open_in_browser_abandoned(info.clone()),
    ) else {
        return;
    };
    let panic_info = info.clone();
    gh_async::spawn_gh_work(
        &deliveries,
        ctx,
        move |ctx| pr_open_in_browser_event(ctx, &info),
        apply_and_persist,
        open_in_browser_abandoned(panic_info),
    );
}

/// Report an abandoned open-in-browser request so it never stays in-flight.
fn open_in_browser_abandoned(
    info: PrOpenInBrowserInfo,
) -> impl FnOnce(&mut AppStateHandle, &SharedContext, String) {
    move |app_state, ctx, message| {
        apply_and_persist(
            app_state,
            ctx,
            AppEvent::PrOpenInBrowserFailed {
                scope_repo_id: info.scope.clone(),
                pr_number: info.number,
                error: format!("GitHub open-in-browser abandoned: {message}"),
            },
        );
    }
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
    let (owner, name, malformed) = resolve_pr_gh_repo_or_triple(state);
    let scope = current_pr_scope_repo_id(state);
    if let Some(message) = malformed {
        return Err(RepoContextError::Malformed(message));
    }
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

// In-app merge dispatch (issue #92) lives in `prs_merge_dispatch.rs`
// (re-exported here for the dispatch chain) to keep this file under the
// architecture boundary line limit.
//
// @requirement REQ-PR-009
pub(super) use super::prs_merge_dispatch::{dispatch_pr_merge, dispatch_pr_merge_methods_load};
