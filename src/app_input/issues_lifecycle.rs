//! Issue close + delete dispatch helpers (issue #182).
//!
//! Mirrors `prs_dispatch::dispatch_pr_merge` and `issues_mutation::create_issue`.
//! All `gh` I/O runs off the UI thread via `spawn_gh_task_with_panic`.

use jefe::domain::RepositoryId;
use jefe::state::AppEvent;

use super::{
    AppStateHandle, SharedContext, apply_and_persist, dispatch_app_event, gh_async, github_client,
    issues_dispatch,
};

/// Handle a close-issue request (key-layer `CloseIssue` event).
///
/// The reducer has already set `close_mutation_pending` if the close is valid.
/// If no pending was set (e.g. already closed), this is a no-op. Otherwise we
/// spawn `GhClient::close_issue` off-thread and deliver `IssueClosed` /
/// `MutationFailed`. On success we reload list + detail.
pub(super) fn handle_issue_close(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let (pending, repo_target) = match resolve_close_context(app_state) {
        CloseContext::Pending(pending, repo) => (pending, repo),
        CloseContext::NothingToDo => return,
        CloseContext::MissingRepoConfig(pending) => {
            report_missing_github_repo(app_state, ctx, pending);
            return;
        }
    };

    let failure_target = MutationFailureTarget {
        scope_repo_id: pending.scope_repo_id,
        issue_number: Some(pending.issue_number),
    };
    let panic_failure_target = failure_target.clone();
    let mutation_id = pending.mutation_id;
    let issue_number = pending.issue_number;

    gh_async::spawn_gh_task_with_panic(
        app_state,
        ctx,
        move |mut app_state, ctx| {
            let event = close_issue_event(
                &ctx,
                &repo_target,
                issue_number,
                &failure_target.scope_repo_id,
                mutation_id,
            );
            apply_close_outcome(&mut app_state, &ctx, event, failure_target, mutation_id);
        },
        move |mut app_state, ctx, message| {
            apply_mutation_failed(
                &mut app_state,
                &ctx,
                panic_failure_target,
                mutation_id,
                format!("GitHub issue close task panicked: {message}"),
            );
        },
    );
}

/// Build the close success/failure event from the gh result (pure).
fn close_issue_event(
    ctx: &SharedContext,
    repo_target: &GhRepoTarget,
    issue_number: u64,
    scope: &RepositoryId,
    mutation_id: u64,
) -> CloseOutcome {
    match github_client(ctx)
        .map(|client| client.close_issue(&repo_target.owner, &repo_target.repo, issue_number))
    {
        Some(Ok(())) => CloseOutcome::Closed {
            scope_repo_id: scope.clone(),
            issue_number,
            mutation_id,
        },
        Some(Err(error)) => CloseOutcome::Failed(error.to_string()),
        None => CloseOutcome::Failed("Application context unavailable".to_string()),
    }
}

/// Apply a close outcome: persist the event. The reducer's optimistic update
/// already reflects the closed state in both the list row and the detail, so no
/// post-mutation list/detail reload is needed (and reloading would race the
/// async list fetch, resetting selection and overwriting the detail with a
/// lightweight preview). This is the authoritative state for the issue we just
/// closed.
fn apply_close_outcome(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    outcome: CloseOutcome,
    failure_target: MutationFailureTarget,
    mutation_id: u64,
) {
    match outcome {
        CloseOutcome::Closed {
            scope_repo_id,
            issue_number,
            mutation_id,
        } => {
            apply_and_persist(
                app_state,
                ctx,
                AppEvent::IssueClosed {
                    scope_repo_id,
                    issue_number,
                    mutation_id,
                    close_reason: None,
                    duplicate_of: None,
                },
            );
        }
        CloseOutcome::Failed(error) => {
            apply_mutation_failed(app_state, ctx, failure_target, mutation_id, error);
        }
    }
}

/// Outcome of a close-issue gh task (success carries the event payload).
enum CloseOutcome {
    Closed {
        scope_repo_id: RepositoryId,
        issue_number: u64,
        mutation_id: u64,
    },
    Failed(String),
}

/// Handle a close-issue-with-reason request (issue #188).
///
/// The reducer has already set `close_mutation_pending` with the reason
/// (and `duplicate_of` for Duplicate). If no pending was set, this is a
/// no-op. Otherwise we spawn `GhClient::close_issue_with_reason` off-thread
/// and deliver `IssueClosed` (carrying the reason) / `MutationFailed`.
///
/// For `Duplicate`, after the close succeeds we additionally resolve the
/// duplicate-of issue's node id and call `mark_issue_as_duplicate`. Failures
/// in the duplicate-marking step are non-fatal (warning only) — the close
/// itself has already succeeded.
pub(super) fn handle_issue_close_with_reason(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let (pending, repo_target) = match resolve_close_context(app_state) {
        CloseContext::Pending(pending, repo) => (pending, repo),
        CloseContext::NothingToDo => return,
        CloseContext::MissingRepoConfig(pending) => {
            report_missing_github_repo(app_state, ctx, pending);
            return;
        }
    };

    let failure_target = MutationFailureTarget {
        scope_repo_id: pending.scope_repo_id.clone(),
        issue_number: Some(pending.issue_number),
    };
    let panic_failure_target = failure_target.clone();
    let mutation_id = pending.mutation_id;
    let issue_number = pending.issue_number;
    let close_reason = pending.close_reason;
    let duplicate_of = pending.duplicate_of;
    let this_node_id = pending.node_id.clone();

    gh_async::spawn_gh_task_with_panic(
        app_state,
        ctx,
        move |mut app_state, ctx| {
            let outcome = close_with_reason_event(CloseWithReasonParams {
                ctx: &ctx,
                repo_target: &repo_target,
                issue_number,
                close_reason,
                duplicate_of,
                this_node_id: this_node_id.as_deref(),
                scope: &failure_target.scope_repo_id,
                mutation_id,
            });
            apply_close_with_reason_outcome(
                &mut app_state,
                &ctx,
                outcome,
                failure_target,
                mutation_id,
            );
        },
        move |mut app_state, ctx, message| {
            apply_mutation_failed(
                &mut app_state,
                &ctx,
                panic_failure_target,
                mutation_id,
                format!("GitHub issue close-with-reason task panicked: {message}"),
            );
        },
    );
}

/// Build the close-with-reason outcome from the gh result (pure).
///
/// For `Duplicate`, after closing successfully we resolve the duplicate-of
/// issue's node id and call `mark_issue_as_duplicate`. Failures in the
/// duplicate-marking step are logged as a warning but do NOT fail the close
/// — the issue is already closed at this point.
fn close_with_reason_event(params: CloseWithReasonParams) -> CloseWithReasonOutcome {
    let reason = params
        .close_reason
        .unwrap_or(jefe::domain::CloseReason::Completed);
    let client = github_client(params.ctx);

    let close_result = client.as_ref().map(|c| {
        c.close_issue_with_reason(
            &params.repo_target.owner,
            &params.repo_target.repo,
            params.issue_number,
            reason,
        )
    });

    match close_result {
        Some(Ok(())) => {
            if reason == jefe::domain::CloseReason::Duplicate
                && let Some(dup_num) = params.duplicate_of
            {
                try_mark_duplicate(
                    client.as_ref(),
                    params.repo_target,
                    dup_num,
                    params.this_node_id,
                    params.issue_number,
                );
            }
            CloseWithReasonOutcome::Closed {
                scope_repo_id: params.scope.clone(),
                issue_number: params.issue_number,
                mutation_id: params.mutation_id,
                close_reason: Some(reason),
                duplicate_of: params.duplicate_of,
            }
        }
        Some(Err(error)) => CloseWithReasonOutcome::Failed(error.to_string()),
        None => CloseWithReasonOutcome::Failed("Application context unavailable".to_string()),
    }
}

/// Attempt to mark an issue as a duplicate of another (non-fatal on failure).
///
/// Resolves the canonical (duplicate-of) issue's node id, then calls
/// `mark_issue_as_duplicate`. If either step fails, a warning is logged but
/// the close is still considered successful — the issue is already closed.
fn try_mark_duplicate(
    client: Option<&jefe::github::GhClient>,
    repo_target: &GhRepoTarget,
    canonical_number: u64,
    duplicate_node_id: Option<&str>,
    issue_number: u64,
) {
    let Some(c) = client else {
        return;
    };
    let canonical_id = match c.resolve_issue_node_id(
        &repo_target.owner,
        &repo_target.repo,
        canonical_number,
    ) {
        Ok(id) => id,
        Err(e) => {
            tracing::warn!(
                error = %e,
                canonical_number,
                issue_number,
                "failed to resolve canonical issue node id for duplicate marking; close still succeeded",
            );
            return;
        }
    };
    let dup_id = match duplicate_node_id {
        Some(id) if !id.is_empty() => id.to_string(),
        _ => {
            tracing::warn!(
                issue_number,
                "missing node id for duplicate issue; cannot mark as duplicate (close still succeeded)",
            );
            return;
        }
    };
    if let Err(e) = c.mark_issue_as_duplicate(&canonical_id, &dup_id) {
        tracing::warn!(
            error = %e,
            canonical_number,
            issue_number,
            "markIssueAsDuplicate mutation failed; close still succeeded",
        );
    }
}

/// Apply a close-with-reason outcome: persist the event with the reason.
fn apply_close_with_reason_outcome(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    outcome: CloseWithReasonOutcome,
    failure_target: MutationFailureTarget,
    mutation_id: u64,
) {
    match outcome {
        CloseWithReasonOutcome::Closed {
            scope_repo_id,
            issue_number,
            mutation_id,
            close_reason,
            duplicate_of,
        } => {
            apply_and_persist(
                app_state,
                ctx,
                AppEvent::IssueClosed {
                    scope_repo_id,
                    issue_number,
                    mutation_id,
                    close_reason,
                    duplicate_of,
                },
            );
        }
        CloseWithReasonOutcome::Failed(error) => {
            apply_mutation_failed(app_state, ctx, failure_target, mutation_id, error);
        }
    }
}

/// Outcome of a close-with-reason gh task (success carries reason + dup info).
enum CloseWithReasonOutcome {
    Closed {
        scope_repo_id: RepositoryId,
        issue_number: u64,
        mutation_id: u64,
        close_reason: Option<jefe::domain::CloseReason>,
        duplicate_of: Option<u64>,
    },
    Failed(String),
}

/// Handle a delete-issue confirm (armed overlay → dispatch).
///
/// The reducer has already set `delete_mutation_pending` with the node id
/// resolved from state. If no pending was set (e.g. missing node id), this is
/// a no-op. Otherwise we spawn `GhClient::delete_issue` off-thread and deliver
/// `IssueDeleted` / `MutationFailed`. On success we reload list + detail.
pub(super) fn handle_issue_delete_confirm(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    // The reducer captured the issue's node id into the pending record at
    // confirm time (and errored if it was unavailable), so the dispatch layer
    // reads it once from the pending rather than re-resolving from mutable
    // state — eliminating a time-of-check/time-of-use seam.
    let (pending, node_id) = match resolve_delete_context(app_state) {
        DeleteContext::Pending(pending, node_id) => (pending, node_id),
        DeleteContext::NothingToDo => return,
        DeleteContext::MissingNodeId(pending) => {
            // Structurally unreachable (the reducer validates node_id before
            // setting a delete pending), but defended so a malformed pending
            // can never leave the UI stuck in-flight.
            report_missing_node_id(app_state, ctx, pending);
            return;
        }
    };

    let failure_target = MutationFailureTarget {
        scope_repo_id: pending.scope_repo_id,
        issue_number: Some(pending.issue_number),
    };
    let panic_failure_target = failure_target.clone();
    let mutation_id = pending.mutation_id;
    let issue_number = pending.issue_number;

    gh_async::spawn_gh_task_with_panic(
        app_state,
        ctx,
        move |mut app_state, ctx| {
            let event = delete_issue_event(
                &ctx,
                &node_id,
                issue_number,
                &failure_target.scope_repo_id,
                mutation_id,
            );
            apply_delete_outcome(&mut app_state, &ctx, event, failure_target, mutation_id);
        },
        move |mut app_state, ctx, message| {
            apply_mutation_failed(
                &mut app_state,
                &ctx,
                panic_failure_target,
                mutation_id,
                format!("GitHub issue delete task panicked: {message}"),
            );
        },
    );
}

/// Build the delete success/failure event from the gh result (pure).
fn delete_issue_event(
    ctx: &SharedContext,
    node_id: &str,
    issue_number: u64,
    scope: &RepositoryId,
    mutation_id: u64,
) -> DeleteOutcome {
    match github_client(ctx).map(|client| client.delete_issue(node_id)) {
        Some(Ok(())) => DeleteOutcome::Deleted {
            scope_repo_id: scope.clone(),
            issue_number,
            mutation_id,
        },
        Some(Err(error)) => DeleteOutcome::Failed(error.to_string()),
        None => DeleteOutcome::Failed("Application context unavailable".to_string()),
    }
}

/// Apply a delete outcome: persist the event, then reload the list on success.
///
/// The reducer's optimistic update already removes the issue from the local
/// list and clears the detail. We dispatch `RefocusIssueList` to fetch a
/// fresh list from GitHub (confirming the deletion and reflowing the list),
/// mirroring `create_issue`'s post-mutation reload. We do NOT call
/// `load_issue_detail_for_selection` here: that would race the async list
/// fetch (whose completion resets selection), and the successor detail is
/// already previewed from list data when the list reload lands.
fn apply_delete_outcome(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    outcome: DeleteOutcome,
    failure_target: MutationFailureTarget,
    mutation_id: u64,
) {
    match outcome {
        DeleteOutcome::Deleted {
            scope_repo_id,
            issue_number,
            mutation_id,
        } => {
            apply_and_persist(
                app_state,
                ctx,
                AppEvent::IssueDeleted {
                    scope_repo_id,
                    issue_number,
                    mutation_id,
                },
            );
            dispatch_app_event(app_state, ctx, AppEvent::RefocusIssueList);
        }
        DeleteOutcome::Failed(error) => {
            apply_mutation_failed(app_state, ctx, failure_target, mutation_id, error);
        }
    }
}

/// Outcome of a delete-issue gh task (success carries the event payload).
enum DeleteOutcome {
    Deleted {
        scope_repo_id: RepositoryId,
        issue_number: u64,
        mutation_id: u64,
    },
    Failed(String),
}

/// Resolve the close context from state (pending + repo target).
///
/// When a pending close exists but the GitHub repo is not configured, returns
/// `MissingRepoConfig` so the caller can deliver a `MutationFailed` (clearing
/// the pending and surfacing an error) rather than leaving the mutation stuck.
enum CloseContext {
    Pending(IssueLifecyclePending, GhRepoTarget),
    MissingRepoConfig(IssueLifecyclePending),
    NothingToDo,
}

fn resolve_close_context(app_state: &AppStateHandle) -> CloseContext {
    let state = app_state.read();
    let Some(pending) = state.issues_state.close_mutation_pending.clone() else {
        return CloseContext::NothingToDo;
    };
    let (owner, repo) = issues_dispatch::resolve_gh_repo(&state);
    if owner.is_empty() || repo.is_empty() {
        return CloseContext::MissingRepoConfig(pending);
    }
    drop(state);
    CloseContext::Pending(pending, GhRepoTarget { owner, repo })
}

/// Resolve the delete context from state (pending + node id).
///
/// The node id is captured by the reducer at confirm time and carried on the
/// pending record. If no delete is pending, this is a no-op.
enum DeleteContext {
    Pending(IssueLifecyclePending, String),
    MissingNodeId(IssueLifecyclePending),
    NothingToDo,
}

fn resolve_delete_context(app_state: &AppStateHandle) -> DeleteContext {
    let state = app_state.read();
    let Some(pending) = state.issues_state.delete_mutation_pending.clone() else {
        return DeleteContext::NothingToDo;
    };
    drop(state);
    // The reducer guarantees `Some(non-empty)` for a delete pending. If a
    // malformed pending (None or empty node id) ever reaches dispatch, surface
    // a failure rather than leaving the mutation stuck in-flight.
    match pending.node_id.clone().filter(|id| !id.is_empty()) {
        Some(node_id) => DeleteContext::Pending(pending, node_id),
        None => DeleteContext::MissingNodeId(pending),
    }
}

fn apply_mutation_failed(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    target: MutationFailureTarget,
    mutation_id: u64,
    error: String,
) {
    apply_and_persist(
        app_state,
        ctx,
        AppEvent::MutationFailed {
            scope_repo_id: target.scope_repo_id,
            issue_number: target.issue_number,
            mutation_id: Some(mutation_id),
            error,
        },
    );
}

/// Deliver a `MutationFailed` for a close that cannot proceed because no GitHub
/// repository is configured. Clears the stuck pending and surfaces an error so
/// the UI is not left in a false in-flight state.
fn report_missing_github_repo(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    pending: IssueLifecyclePending,
) {
    apply_mutation_failed(
        app_state,
        ctx,
        MutationFailureTarget {
            scope_repo_id: pending.scope_repo_id,
            issue_number: Some(pending.issue_number),
        },
        pending.mutation_id,
        "No GitHub repository configured. Set the GitHub Repo field (owner/repo) in repository settings.".to_string(),
    );
}

/// Deliver a `MutationFailed` for a delete that cannot proceed because the
/// issue's node id is unavailable. Clears the stuck pending and surfaces an
/// error. Structurally unreachable (the reducer validates node_id before
/// setting a delete pending) but defended for safety.
fn report_missing_node_id(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    pending: IssueLifecyclePending,
) {
    apply_mutation_failed(
        app_state,
        ctx,
        MutationFailureTarget {
            scope_repo_id: pending.scope_repo_id,
            issue_number: Some(pending.issue_number),
        },
        pending.mutation_id,
        "Cannot delete: issue node id unavailable. Reload the issue list and try again."
            .to_string(),
    );
}

#[derive(Clone)]
struct GhRepoTarget {
    owner: String,
    repo: String,
}

#[derive(Clone)]
struct MutationFailureTarget {
    scope_repo_id: RepositoryId,
    issue_number: Option<u64>,
}

/// Borrowed parameters for `close_with_reason_event` (issue #188).
///
/// Groups the close-with-reason inputs so the pure event builder stays under
/// the argument-count limit while remaining self-documenting.
struct CloseWithReasonParams<'a> {
    ctx: &'a SharedContext,
    repo_target: &'a GhRepoTarget,
    issue_number: u64,
    close_reason: Option<jefe::domain::CloseReason>,
    duplicate_of: Option<u64>,
    this_node_id: Option<&'a str>,
    scope: &'a RepositoryId,
    mutation_id: u64,
}

/// Re-export the lifecycle pending type for local convenience.
type IssueLifecyclePending = jefe::state::IssueLifecycleMutationPending;
