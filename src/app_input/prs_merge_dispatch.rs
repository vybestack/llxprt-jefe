//! PR merge dispatch helpers (issue #92), extracted from `prs_dispatch.rs`
//! to keep that file within the architecture boundary line limit.
//!
//! Resolves the merge context from the pending merge mutation, spawns
//! `GhClient::merge_pull_request` off the UI thread, and loads the allowed
//! merge methods when the chooser opens. All `gh` I/O runs off the UI thread
//! via `spawn_gh_task_with_panic`.
//!
//! Malformed tracker errors are preserved: a malformed nonblank
//! `github_issue_pr_repo` override surfaces the typed reason rather than
//! collapsing to a generic "missing GitHub Repo" (issue #266).

use jefe::domain::RepositoryId;
use jefe::state::AppEvent;

use super::prs_dispatch::{
    RepoContextError, current_pr_scope_repo_id, resolve_pr_gh_repo_or_error,
};
use super::{AppStateHandle, SharedContext, apply_and_persist, dispatch_app_event, gh_async};

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
/// `Err(RepoContextError::InvalidSlug)` when the slug is malformed (carrying
/// the typed malformed reason), and `Err(RepoContextError::NoSelection)`
/// when no mutation is pending.
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
    let (owner, name, malformed) = resolve_pr_merge_target(state);
    if !malformed.is_empty() {
        return Err(RepoContextError::Malformed(malformed));
    }
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

/// Resolve the merge context from the pending merge mutation in state.
///
/// Returns `Ok(info)` when a merge mutation is pending with a valid repo slug,
/// `Err(RepoContextError::InvalidSlug)` when the slug is malformed, and
/// `Err(RepoContextError::NoSelection)` when no mutation is pending.
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
        Err(RepoContextError::Malformed(message)) => {
            let (scope, pr_number, mutation_id) = pr_merge_failure_context(app_state);
            apply_and_persist(
                app_state,
                ctx,
                AppEvent::PrMergeFailed {
                    scope_repo_id: scope,
                    pr_number,
                    mutation_id,
                    error: message,
                },
            );
        }
        Err(RepoContextError::InvalidSlug) => {
            let (scope, pr_number, mutation_id) = pr_merge_failure_context(app_state);
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
    let panic_scope = info.scope.clone();
    let panic_pr_number = info.number;
    let panic_mutation_id = info.mutation_id;
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
                    scope_repo_id: panic_scope,
                    pr_number: panic_pr_number,
                    mutation_id: panic_mutation_id,
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
    let result = super::github_client(ctx)
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
            error: "GitHub client unavailable from application context".to_string(),
        },
    }
}

/// Resolve the scope, PR number, and mutation id for a merge failure event.
fn pr_merge_failure_context(app_state: &AppStateHandle) -> (RepositoryId, u64, u64) {
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
}

/// Dispatch the merge-methods fetch when the chooser opens.
///
/// Resolves the repo owner/name from state and spawns
/// `GhClient::get_repo_merge_methods` OFF the UI thread, delivering
/// `PrMergeMethodsLoaded` on success. On failure, nothing is delivered — the
/// chooser treats `allowed_methods: None` as "all available" (graceful
/// degradation). A malformed nonblank override surfaces the typed error
/// as a `PrMergeMethodsLoadFailed` event rather than collapsing to empty
/// (issue #266).
///
/// @requirement REQ-PR-009
pub(super) fn dispatch_pr_merge_methods_load(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let info = {
        let state = app_state.read();
        let pr_number = state.prs_state.pr_detail.as_ref().map_or(0, |d| d.number);
        let (owner, name, malformed) = resolve_pr_merge_target(&state);
        let scope = current_pr_scope_repo_id(&state);
        drop(state);
        if !malformed.is_empty() {
            Err((scope, pr_number, malformed))
        } else if owner.is_empty() || name.is_empty() {
            Ok(None)
        } else {
            Ok(Some((scope, owner, name, pr_number)))
        }
    };
    match info {
        Err((scope, pr_number, error)) => {
            apply_and_persist(
                app_state,
                ctx,
                AppEvent::PrMergeMethodsLoadFailed {
                    scope_repo_id: scope,
                    pr_number,
                    error,
                },
            );
        }
        Ok(Some((scope, owner, name, pr_number))) => {
            gh_async::spawn_gh_task_with_panic(
                app_state,
                ctx,
                move |mut app_state, ctx| {
                    if let Some(event) =
                        pr_merge_methods_event(&ctx, &scope, &owner, &name, pr_number)
                    {
                        apply_and_persist(&mut app_state, &ctx, event);
                    }
                },
                // The shared task wrapper logs panics; the chooser keeps its
                // graceful "all available" fallback instead of surfacing one.
                move |_app_state, _ctx, _message| {},
            );
        }
        Ok(None) => {}
    }
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
    let methods = super::github_client(ctx)?
        .get_repo_merge_methods(owner, name)
        .ok()?;
    Some(AppEvent::PrMergeMethodsLoaded {
        scope_repo_id: scope.clone(),
        pr_number,
        allowed_methods: methods,
    })
}

/// Resolve `(owner, name, malformed_message)` from the effective tracker.
///
/// When the tracker resolves cleanly, returns `(owner, name, empty)`. When a
/// nonblank override is malformed, returns `(empty, empty, message)` so the
/// caller can surface the typed reason instead of a misleading "missing
/// GitHub Repo" (issue #266).
fn resolve_pr_merge_target(state: &jefe::state::AppState) -> (String, String, String) {
    match resolve_pr_gh_repo_or_error(state) {
        Ok((owner, repo)) => (owner, repo, String::new()),
        Err(error) => (String::new(), String::new(), error.message),
    }
}
