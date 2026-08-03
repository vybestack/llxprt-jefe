//! Pull-request delete dispatch (issue #183).
//!
//! Mirrors `prs_merge_dispatch`: the reducer has already decided what to do, so
//! this module only executes it off the UI thread. Deleting means closing the
//! pull request when it is still open — through the same `close_item` call the
//! state property editor uses — and then removing its head branch.

use jefe::domain::RepositoryId;
use jefe::github::GhClient;
use jefe::state::{AppEvent, PrLifecycleEvent};

use super::prs_dispatch::resolve_pr_gh_repo_or_error;
use super::{AppStateHandle, SharedContext, apply_and_persist, gh_async, github_client};

/// Everything the delete needs, resolved from state before leaving the thread.
#[derive(Clone)]
struct PrDeleteTask {
    scope_repo_id: RepositoryId,
    mutation_id: u64,
    pr_number: u64,
    head_ref: String,
    close_first: bool,
    owner: String,
    repo: String,
}

/// Handle a confirmed pull-request delete.
///
/// The reducer records a pending only for a delete it has already validated, so
/// an absent pending here means the confirmation merely armed the overlay.
pub(super) fn handle_pr_delete_confirm(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let task = match resolve_pr_delete_task(app_state) {
        Ok(Some(task)) => task,
        Ok(None) => return,
        Err((identity, message)) => {
            apply_and_persist(app_state, ctx, delete_failed(&identity, false, message));
            return;
        }
    };

    let abandoned_identity = delete_identity(&task);
    let Some(deliveries) =
        gh_async::delivery_handle_or_report(app_state, ctx, abandoned(abandoned_identity.clone()))
    else {
        return;
    };
    let work_task = task.clone();
    gh_async::spawn_gh_work(
        &deliveries,
        ctx,
        move |ctx| pr_delete_event(ctx, &work_task),
        apply_mutation_outcome,
        abandoned(abandoned_identity),
    );
}

/// Apply a lifecycle-mutation result, then let the coalesced refresh reconcile
/// with GitHub. The reducer has already applied whatever it can optimistically,
/// so the refresh confirms rather than reveals it.
fn apply_mutation_outcome(app_state: &mut AppStateHandle, ctx: &SharedContext, event: AppEvent) {
    apply_and_persist(app_state, ctx, event);
    super::prs_orchestration::resume_pr_post_mutation_refresh(app_state, ctx);
}

/// Identity of the delete whose failure must be reported.
#[derive(Clone)]
struct PrDeleteIdentity {
    scope_repo_id: RepositoryId,
    mutation_id: u64,
    pr_number: u64,
}

fn delete_identity(task: &PrDeleteTask) -> PrDeleteIdentity {
    PrDeleteIdentity {
        scope_repo_id: task.scope_repo_id.clone(),
        mutation_id: task.mutation_id,
        pr_number: task.pr_number,
    }
}

fn delete_failed(identity: &PrDeleteIdentity, closed: bool, error: String) -> AppEvent {
    PrLifecycleEvent::DeleteFailed {
        scope_repo_id: identity.scope_repo_id.clone(),
        pr_number: identity.pr_number,
        mutation_id: identity.mutation_id,
        closed,
        error,
    }
    .into()
}

/// Report an abandoned delete so the pending never stays in flight.
fn abandoned(
    identity: PrDeleteIdentity,
) -> impl FnOnce(&mut AppStateHandle, &SharedContext, String) {
    move |app_state, ctx, message| {
        apply_and_persist(
            app_state,
            ctx,
            delete_failed(
                &identity,
                false,
                format!("GitHub delete abandoned: {message}"),
            ),
        );
    }
}

/// Resolve the pending delete and its repository, or the failure that stops it.
fn resolve_pr_delete_task(
    app_state: &AppStateHandle,
) -> Result<Option<PrDeleteTask>, (PrDeleteIdentity, String)> {
    let state = app_state.read();
    let Some(pending) = state.prs_state.delete_mutation_pending.clone() else {
        return Ok(None);
    };
    let repo = resolve_pr_gh_repo_or_error(&state);
    drop(state);

    let identity = PrDeleteIdentity {
        scope_repo_id: pending.scope_repo_id.clone(),
        mutation_id: pending.mutation_id,
        pr_number: pending.pr_number,
    };
    let (owner, repo) = match repo {
        Ok(pair) => pair,
        Err(malformed) => return Err((identity, malformed.message)),
    };
    if owner.is_empty() || repo.is_empty() {
        return Err((
            identity,
            "Configure repository (owner/name) before deleting".to_string(),
        ));
    }
    Ok(Some(PrDeleteTask {
        scope_repo_id: pending.scope_repo_id,
        mutation_id: pending.mutation_id,
        pr_number: pending.pr_number,
        head_ref: pending.head_ref,
        close_first: pending.close_first,
        owner,
        repo,
    }))
}

/// Run the delete and build its result event (off the UI thread).
fn pr_delete_event(ctx: &SharedContext, task: &PrDeleteTask) -> AppEvent {
    let identity = delete_identity(task);
    let Some(client) = github_client(ctx) else {
        return delete_failed(
            &identity,
            false,
            "GitHub client unavailable from application context".to_string(),
        );
    };
    match execute_pr_delete(client, task) {
        PrDeleteOutcome::Done => PrLifecycleEvent::Deleted {
            scope_repo_id: task.scope_repo_id.clone(),
            pr_number: task.pr_number,
            mutation_id: task.mutation_id,
            branch: task.head_ref.clone(),
            closed: task.close_first,
        }
        .into(),
        PrDeleteOutcome::Failed { closed, error } => delete_failed(&identity, closed, error),
    }
}

/// What a delete attempt actually accomplished.
///
/// A delete is two calls, so failure is not all-or-nothing: `closed` records
/// whether the pull request was already closed on GitHub when the branch
/// removal failed, which the reducer needs so the screen does not contradict
/// the server.
enum PrDeleteOutcome {
    Done,
    Failed { closed: bool, error: String },
}

/// Close the pull request when it is still open, then remove its head branch.
///
/// The close comes first: a branch removed while the pull request is open would
/// leave it open with no head, which is a worse state than either end.
fn execute_pr_delete(client: GhClient, task: &PrDeleteTask) -> PrDeleteOutcome {
    if task.close_first
        && let Err(error) = client.close_item(&task.owner, &task.repo, task.pr_number, true)
    {
        return PrDeleteOutcome::Failed {
            closed: false,
            error: format!("could not close the pull request: {error}"),
        };
    }
    let closed = task.close_first;
    let ref_id = match client.resolve_branch_ref_id(&task.owner, &task.repo, &task.head_ref) {
        Ok(ref_id) => ref_id,
        Err(error) => {
            return PrDeleteOutcome::Failed {
                closed,
                error: format!("could not resolve branch {}: {error}", task.head_ref),
            };
        }
    };
    match client.delete_branch_ref(&ref_id) {
        Ok(()) => PrDeleteOutcome::Done,
        Err(error) => PrDeleteOutcome::Failed {
            closed,
            error: format!("could not delete branch {}: {error}", task.head_ref),
        },
    }
}

// ── New PR composer ────────────────────────────────────────────────────────

/// Everything the branch load needs, resolved before leaving the thread.
#[derive(Clone)]
struct BranchLoadTask {
    scope_repo_id: RepositoryId,
    request_id: u64,
    owner: String,
    repo: String,
}

/// Fetch the repository's branches for a freshly opened composer.
pub(super) fn handle_pr_branches_load(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let task = match resolve_branch_load_task(app_state) {
        Ok(Some(task)) => task,
        Ok(None) => return,
        Err((scope_repo_id, request_id, message)) => {
            apply_and_persist(
                app_state,
                ctx,
                branches_failed(&scope_repo_id, request_id, message),
            );
            return;
        }
    };

    let abandoned_task = task.clone();
    let Some(deliveries) =
        gh_async::delivery_handle_or_report(app_state, ctx, branches_abandoned(abandoned_task))
    else {
        return;
    };
    let work_task = task.clone();
    gh_async::spawn_gh_work(
        &deliveries,
        ctx,
        move |ctx| branches_event(ctx, &work_task),
        apply_and_persist,
        branches_abandoned(task),
    );
}

fn branches_failed(scope_repo_id: &RepositoryId, request_id: u64, error: String) -> AppEvent {
    PrLifecycleEvent::BranchesFailed {
        scope_repo_id: scope_repo_id.clone(),
        request_id,
        error,
    }
    .into()
}

/// An abandoned branch load must still unblock the composer.
fn branches_abandoned(
    task: BranchLoadTask,
) -> impl FnOnce(&mut AppStateHandle, &SharedContext, String) {
    move |app_state, ctx, message| {
        apply_and_persist(
            app_state,
            ctx,
            branches_failed(
                &task.scope_repo_id,
                task.request_id,
                format!("branch listing abandoned: {message}"),
            ),
        );
    }
}

type BranchLoadFailure = (RepositoryId, u64, String);

fn resolve_branch_load_task(
    app_state: &AppStateHandle,
) -> Result<Option<BranchLoadTask>, BranchLoadFailure> {
    let state = app_state.read();
    let Some(form) = state.prs_state.new_pr_form.as_ref() else {
        return Ok(None);
    };
    let request_id = form.load_request_id;
    let scope_repo_id = super::prs_dispatch::current_pr_scope_repo_id(&state);
    let resolved = resolve_pr_gh_repo_or_error(&state);
    drop(state);

    let (owner, repo) = match resolved {
        Ok(pair) => pair,
        Err(malformed) => return Err((scope_repo_id, request_id, malformed.message)),
    };
    if owner.is_empty() || repo.is_empty() {
        return Err((
            scope_repo_id,
            request_id,
            "no GitHub repository is configured (owner/name)".to_string(),
        ));
    }
    Ok(Some(BranchLoadTask {
        scope_repo_id,
        request_id,
        owner,
        repo,
    }))
}

fn branches_event(ctx: &SharedContext, task: &BranchLoadTask) -> AppEvent {
    let Some(client) = github_client(ctx) else {
        return branches_failed(
            &task.scope_repo_id,
            task.request_id,
            "GitHub client unavailable from application context".to_string(),
        );
    };
    match client.fetch_repository_branches(&task.owner, &task.repo) {
        Ok(branches) => PrLifecycleEvent::BranchesLoaded {
            scope_repo_id: task.scope_repo_id.clone(),
            request_id: task.request_id,
            branches: branches.names,
            default_branch: branches.default_branch,
        }
        .into(),
        Err(error) => branches_failed(&task.scope_repo_id, task.request_id, error.to_string()),
    }
}

/// Everything the create needs, resolved before leaving the thread.
#[derive(Clone)]
struct PrCreateTask {
    scope_repo_id: RepositoryId,
    mutation_id: u64,
    owner: String,
    repo: String,
    head: String,
    base: String,
    title: String,
    body: String,
}

/// Open the pull request the composer describes.
pub(super) fn handle_pr_create(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let task = match resolve_pr_create_task(app_state) {
        Ok(Some(task)) => task,
        Ok(None) => return,
        Err((scope_repo_id, mutation_id, message)) => {
            apply_and_persist(
                app_state,
                ctx,
                create_failed(&scope_repo_id, mutation_id, message),
            );
            return;
        }
    };

    let abandoned_task = task.clone();
    let Some(deliveries) =
        gh_async::delivery_handle_or_report(app_state, ctx, create_abandoned(abandoned_task))
    else {
        return;
    };
    let work_task = task.clone();
    gh_async::spawn_gh_work(
        &deliveries,
        ctx,
        move |ctx| create_event(ctx, &work_task),
        apply_mutation_outcome,
        create_abandoned(task),
    );
}

fn create_failed(scope_repo_id: &RepositoryId, mutation_id: u64, error: String) -> AppEvent {
    PrLifecycleEvent::CreateFailed {
        scope_repo_id: scope_repo_id.clone(),
        mutation_id,
        error,
    }
    .into()
}

/// An abandoned create must still clear the pending so the composer recovers.
fn create_abandoned(
    task: PrCreateTask,
) -> impl FnOnce(&mut AppStateHandle, &SharedContext, String) {
    move |app_state, ctx, message| {
        apply_and_persist(
            app_state,
            ctx,
            create_failed(
                &task.scope_repo_id,
                task.mutation_id,
                format!("pull-request creation abandoned: {message}"),
            ),
        );
    }
}

type PrCreateFailure = (RepositoryId, u64, String);

fn resolve_pr_create_task(
    app_state: &AppStateHandle,
) -> Result<Option<PrCreateTask>, PrCreateFailure> {
    let state = app_state.read();
    let Some(pending) = state.prs_state.create_mutation_pending.clone() else {
        return Ok(None);
    };
    let Some(form) = state.prs_state.new_pr_form.as_ref() else {
        return Ok(None);
    };
    // The reducer refuses to record a pending unless both branches resolve.
    let (Some(head), Some(base)) = (form.head_branch(), form.base_branch()) else {
        return Ok(None);
    };
    let head = head.to_string();
    let base = base.to_string();
    let title = form.title_text.trim().to_string();
    let body = form.body_text.clone();
    let resolved = resolve_pr_gh_repo_or_error(&state);
    drop(state);

    let (owner, repo) = match resolved {
        Ok(pair) => pair,
        Err(malformed) => {
            return Err((
                pending.scope_repo_id,
                pending.mutation_id,
                malformed.message,
            ));
        }
    };
    if owner.is_empty() || repo.is_empty() {
        return Err((
            pending.scope_repo_id,
            pending.mutation_id,
            "no GitHub repository is configured (owner/name)".to_string(),
        ));
    }
    Ok(Some(PrCreateTask {
        scope_repo_id: pending.scope_repo_id,
        mutation_id: pending.mutation_id,
        owner,
        repo,
        head,
        base,
        title,
        body,
    }))
}

fn create_event(ctx: &SharedContext, task: &PrCreateTask) -> AppEvent {
    let Some(client) = github_client(ctx) else {
        return create_failed(
            &task.scope_repo_id,
            task.mutation_id,
            "GitHub client unavailable from application context".to_string(),
        );
    };
    let request = jefe::github::CreatePullRequest {
        owner: &task.owner,
        repo: &task.repo,
        head: &task.head,
        base: &task.base,
        title: &task.title,
        body: &task.body,
    };
    match client.create_pull_request(request) {
        Ok(pr_number) => PrLifecycleEvent::Created {
            scope_repo_id: task.scope_repo_id.clone(),
            mutation_id: task.mutation_id,
            pr_number,
        }
        .into(),
        Err(error) => create_failed(&task.scope_repo_id, task.mutation_id, error.to_string()),
    }
}
