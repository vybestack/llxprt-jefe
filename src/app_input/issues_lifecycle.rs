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

/// Error message when the issue's node id is unavailable for a GraphQL close.
const NODE_ID_UNAVAILABLE_MSG: &str =
    "Cannot close: issue node id unavailable. Reload the issue list and try again.";

/// Handle a close-issue request (key-layer `CloseIssue` event).
///
/// The reducer has already set `close_mutation_pending` (with the node id) if
/// the close is valid. If no pending was set (e.g. already closed), this is a
/// no-op. Otherwise we spawn the GraphQL `closeIssue` mutation off-thread
/// with `stateReason: COMPLETED` (plain close defaults to completed per issue
/// #204) and deliver `IssueClosed` / `MutationFailed`.
pub(super) fn handle_issue_close(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let pending = match resolve_close_context(app_state) {
        CloseContext::Pending(pending, _repo) => pending,
        CloseContext::NothingToDo => return,
        CloseContext::MissingRepoConfig(pending, malformed) => {
            report_missing_github_repo(app_state, ctx, pending, malformed);
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
    let node_id = pending.node_id.clone();

    gh_async::spawn_gh_task_with_panic(
        app_state,
        ctx,
        move |mut app_state, ctx| {
            let event = close_issue_event(
                &ctx,
                issue_number,
                node_id.as_deref(),
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
///
/// Plain close uses the GraphQL `closeIssue` mutation with
/// `stateReason: COMPLETED` (issue #204).
fn close_issue_event(
    ctx: &SharedContext,
    issue_number: u64,
    node_id: Option<&str>,
    scope: &RepositoryId,
    mutation_id: u64,
) -> CloseOutcome {
    let Some(node_id) = node_id.filter(|id| !id.is_empty()) else {
        return CloseOutcome::Failed(NODE_ID_UNAVAILABLE_MSG.to_string());
    };
    match github_client(ctx).map(|client| {
        client.close_issue_graphql(node_id, jefe::domain::CloseReason::Completed, None)
    }) {
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

/// Handle a close-issue-with-reason request (issue #188 / #204).
///
/// The reducer has already set `close_mutation_pending` with the reason
/// (and `duplicate_of` for Duplicate). If no pending was set, this is a
/// no-op. Otherwise we spawn the GraphQL `closeIssue` mutation off-thread,
/// carrying `stateReason` and (for Duplicate) `duplicateIssueId` as
/// first-class fields, and deliver `IssueClosed` (carrying the reason) /
/// `MutationFailed`.
pub(super) fn handle_issue_close_with_reason(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let (pending, repo_target) = match resolve_close_context(app_state) {
        CloseContext::Pending(pending, repo) => (pending, repo),
        CloseContext::NothingToDo => return,
        CloseContext::MissingRepoConfig(pending, malformed) => {
            report_missing_github_repo(app_state, ctx, pending, malformed);
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
/// Uses the GraphQL `closeIssue` mutation with `stateReason` and (for
/// Duplicate) `duplicateIssueId` as first-class fields (issue #204). For a
/// Duplicate close, the canonical (duplicate-of) issue's node id is resolved
/// before the mutation so it can be passed as `duplicateIssueId` in the same
/// call. If the canonical node id cannot be resolved, the close fails — we do
/// not close as Duplicate without the link.
fn close_with_reason_event(params: CloseWithReasonParams) -> CloseWithReasonOutcome {
    let reason = params
        .close_reason
        .unwrap_or(jefe::domain::CloseReason::Completed);

    let Some(this_node_id) = params.this_node_id.filter(|id| !id.is_empty()) else {
        return CloseWithReasonOutcome::Failed(NODE_ID_UNAVAILABLE_MSG.to_string());
    };

    let client = github_client(params.ctx);

    // For a Duplicate close, resolve the canonical issue's node id so it can be
    // passed as `duplicateIssueId` in the single GraphQL `closeIssue` call.
    let duplicate_node_id = if reason == jefe::domain::CloseReason::Duplicate {
        match params.duplicate_of {
            Some(dup_num) => {
                let Some(c) = client.as_ref() else {
                    return CloseWithReasonOutcome::Failed(
                        "Application context unavailable".to_string(),
                    );
                };
                match c.resolve_issue_node_id(
                    &params.repo_target.owner,
                    &params.repo_target.repo,
                    dup_num,
                ) {
                    Ok(id) => Some(id),
                    Err(e) => {
                        return CloseWithReasonOutcome::Failed(format!(
                            "Failed to resolve duplicate-of issue #{dup_num} node id: {e}"
                        ));
                    }
                }
            }
            None => {
                return CloseWithReasonOutcome::Failed(
                    "Cannot close as duplicate: no duplicate target selected".to_string(),
                );
            }
        }
    } else {
        None
    };

    let close_result = client
        .as_ref()
        .map(|c| c.close_issue_graphql(this_node_id, reason, duplicate_node_id.as_deref()));

    match close_result {
        Some(Ok(())) => CloseWithReasonOutcome::Closed {
            scope_repo_id: params.scope.clone(),
            issue_number: params.issue_number,
            mutation_id: params.mutation_id,
            close_reason: Some(reason),
            duplicate_of: params.duplicate_of,
        },
        Some(Err(error)) => CloseWithReasonOutcome::Failed(error.to_string()),
        None => CloseWithReasonOutcome::Failed("Application context unavailable".to_string()),
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
/// fresh list from GitHub (confirming the deletion and reflowing the list).
/// We do NOT call `load_issue_detail_for_selection` here: that would race the
/// async list fetch (whose completion resets selection), and the successor
/// detail is already previewed from list data when the list reload lands.
/// (Issue create intentionally avoids both `RefocusIssueList` and a post-create
/// detail network fetch — see issue #215.)
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
    MissingRepoConfig(IssueLifecyclePending, Option<String>),
    NothingToDo,
}

fn resolve_close_context(app_state: &AppStateHandle) -> CloseContext {
    let state = app_state.read();
    let Some(pending) = state.issues_state.close_mutation_pending.clone() else {
        return CloseContext::NothingToDo;
    };
    let (owner, repo, malformed) = issues_dispatch::resolve_gh_repo_or_error(&state).map_or_else(
        |error| (String::new(), String::new(), Some(error.message)),
        |(owner, repo)| (owner, repo, None),
    );
    if owner.is_empty() || repo.is_empty() {
        return CloseContext::MissingRepoConfig(pending, malformed);
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
/// the UI is not left in a false in-flight state. When `malformed` is `Some`,
/// the typed malformed reason is surfaced instead of the generic "missing
/// GitHub Repo" message (issue #266).
fn report_missing_github_repo(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    pending: IssueLifecyclePending,
    malformed: Option<String>,
) {
    let error = malformed.unwrap_or_else(|| {
        "No GitHub repository configured. Set the GitHub Repo field (owner/repo) in repository settings.".to_string()
    });
    apply_mutation_failed(
        app_state,
        ctx,
        MutationFailureTarget {
            scope_repo_id: pending.scope_repo_id,
            issue_number: Some(pending.issue_number),
        },
        pending.mutation_id,
        error,
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
