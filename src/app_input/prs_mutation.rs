//! PR-mode inline-mutation dispatch helpers.
//!
//! Mirrors `issues_mutation::handle_inline_submit`. Spawns the gh PR
//! comment-create off the UI thread via `gh_async::spawn_gh_work`.
//!
//! @plan PLAN-20260624-PR-MODE.P11
//! @requirement REQ-PR-010
//! @requirement REQ-PR-011
//! @pseudocode component-003 lines 109-119

use jefe::state::{AppEvent, AppState, ComposerTarget, InlineState};

use super::{
    AppStateHandle, SharedContext, apply_and_persist, dispatch_app_event, gh_async, github_client,
    prs_dispatch,
};

/// Handle an inline submit for PR Mode.
///
/// Reads the mutation-pending target + composer text, validates the repo, and
/// spawns the gh comment/reply task via `gh_async::spawn_gh_work`,
/// delivering `PrCommentCreated` on success or `PrCommentCreateFailed` on
/// Err/panic.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-010
/// @requirement REQ-PR-011
/// @pseudocode component-003 lines 109-119
pub fn handle_pr_inline_submit(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let action = {
        let state = app_state.read();
        resolve_pr_inline_submit(&state)
    };
    let Some(action) = action else {
        tracing::debug!("ignoring PR inline submit: inconsistent pending mutation or composer");
        return;
    };
    let repo = match pr_repo_target(app_state) {
        Ok(Some(repo)) => repo,
        Ok(None) => {
            report_missing_github_repo(app_state, ctx, &action, None);
            return;
        }
        Err(message) => {
            report_missing_github_repo(app_state, ctx, &action, Some(message));
            return;
        }
    };
    if let ComposerTarget::ReplyToReviewThread {
        thread_index,
        thread_id,
        ..
    } = &action.target
    {
        // Prefer the stable thread_id captured at composer-open time; fall
        // back to positional resolution only if it is somehow empty (issue #238).
        let resolved_id = if thread_id.is_empty() {
            resolve_thread_id(app_state, *thread_index)
        } else {
            Some(thread_id.clone())
        };
        let Some(thread_id) = resolved_id else {
            apply_and_persist(
                app_state,
                ctx,
                AppEvent::PrCommentCreateFailed {
                    scope_repo_id: action.scope_repo_id.clone(),
                    pr_number: action.pr_number,
                    mutation_id: action.mutation_id,
                    error: "Review thread not found (it may have been removed).".to_string(),
                },
            );
            return;
        };
        dispatch_pr_thread_reply(app_state, ctx, repo, action, thread_id);
    } else if matches!(action.target, ComposerTarget::NewReviewThread { .. }) {
        dispatch_pr_review_comment_create(app_state, ctx, repo, action);
    } else {
        dispatch_pr_comment_create(app_state, ctx, repo, action);
    }
}

/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-010
/// @pseudocode component-004 lines 146-155
#[derive(Clone)]
pub(super) struct PrInlineSubmitAction {
    pub(super) scope_repo_id: jefe::domain::RepositoryId,
    pub(super) pr_number: u64,
    pub(super) mutation_id: u64,
    pub(super) text: String,
    pub(super) target: ComposerTarget,
}

/// Resolve the inline-submit action from one committed post-reducer snapshot.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 310-325
pub(super) fn resolve_pr_inline_submit(state: &AppState) -> Option<PrInlineSubmitAction> {
    let pending = state.prs_state.mutation_pending.as_ref()?;
    let pr_number = state.prs_state.pr_detail.as_ref()?.number;
    let InlineState::Composer { text, target, .. } = &state.prs_state.inline_state else {
        return None;
    };
    if text.trim().is_empty() || target != &pending.target {
        return None;
    }
    Some(PrInlineSubmitAction {
        scope_repo_id: pending.scope_repo_id.clone(),
        pr_number,
        mutation_id: pending.mutation_id,
        text: text.clone(),
        target: pending.target.clone(),
    })
}

/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-010
/// @pseudocode component-004 lines 146-155
#[derive(Clone)]
struct PrRepoTarget {
    owner: String,
    repo: String,
}

/// Resolve the GitHub owner/repo for the currently selected repository.
///
/// Returns `Ok(Some(target))` when resolved, `Ok(None)` when genuinely absent,
/// and `Err(message)` when a nonblank override is malformed (so the malformed
/// reason can be surfaced instead of a misleading "missing GitHub Repo").
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-010
/// @pseudocode component-004 lines 146-155
fn pr_repo_target(app_state: &AppStateHandle) -> Result<Option<PrRepoTarget>, String> {
    let state = app_state.read();
    match prs_dispatch::resolve_pr_gh_repo_or_error(&state) {
        Ok((owner, repo)) if !owner.is_empty() && !repo.is_empty() => {
            Ok(Some(PrRepoTarget { owner, repo }))
        }
        Ok(_) => Ok(None),
        Err(error) => Err(error.message),
    }
}

/// Report a missing or malformed GitHub repo as a mutation failure
/// (synchronous). When `malformed` is `Some`, the malformed reason is
/// surfaced instead of the generic "missing GitHub Repo" message.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-013
/// @pseudocode component-004 lines 146-155
fn report_missing_github_repo(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    action: &PrInlineSubmitAction,
    malformed: Option<String>,
) {
    let error = malformed.unwrap_or_else(|| {
        "No GitHub repository configured. Set the GitHub Repo field (owner/repo) in repository settings.".to_string()
    });
    apply_and_persist(
        app_state,
        ctx,
        AppEvent::PrCommentCreateFailed {
            scope_repo_id: action.scope_repo_id.clone(),
            pr_number: action.pr_number,
            mutation_id: action.mutation_id,
            error,
        },
    );
}

/// Spawn the gh PR comment-create task off the UI thread.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-010
/// @pseudocode component-004 lines 146-155
fn dispatch_pr_comment_create(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    repo: PrRepoTarget,
    action: PrInlineSubmitAction,
) {
    let Some(deliveries) = gh_async::delivery_handle_or_report(
        app_state,
        ctx,
        pr_comment_abandoned(action.clone(), "GitHub PR comment"),
    ) else {
        return;
    };
    let panic_action = action.clone();
    gh_async::spawn_gh_work(
        &deliveries,
        ctx,
        move |ctx| pr_comment_create_event(ctx, &repo, &action),
        // Route through the full dispatch chain so a successful
        // `PrCommentCreated` triggers the post-mutation detail reload
        // (issue #128). A `PrCommentCreateFailed` does not trigger a reload.
        dispatch_app_event,
        pr_comment_abandoned(panic_action, "GitHub PR comment"),
    );
}

/// Report an abandoned PR comment/reply request so the composer never stays
/// stuck in-flight.
fn pr_comment_abandoned(
    action: PrInlineSubmitAction,
    task_label: &'static str,
) -> impl FnOnce(&mut AppStateHandle, &SharedContext, String) {
    move |app_state, ctx, message| {
        apply_and_persist(
            app_state,
            ctx,
            AppEvent::PrCommentCreateFailed {
                scope_repo_id: action.scope_repo_id,
                pr_number: action.pr_number,
                mutation_id: action.mutation_id,
                error: format!("{task_label} abandoned: {message}"),
            },
        );
    }
}

/// Build the comment-created/failed event from the gh result (background thread).
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-010
/// @pseudocode component-004 lines 146-155
fn pr_comment_create_event(
    ctx: &SharedContext,
    repo: &PrRepoTarget,
    action: &PrInlineSubmitAction,
) -> AppEvent {
    let result = github_client(ctx).map(|client| {
        client.create_pr_comment(&repo.owner, &repo.repo, action.pr_number, &action.text)
    });
    match result {
        Some(Ok(comment)) => AppEvent::PrCommentCreated {
            scope_repo_id: action.scope_repo_id.clone(),
            pr_number: action.pr_number,
            mutation_id: action.mutation_id,
            comment,
        },
        Some(Err(error)) => AppEvent::PrCommentCreateFailed {
            scope_repo_id: action.scope_repo_id.clone(),
            pr_number: action.pr_number,
            mutation_id: action.mutation_id,
            error: error.to_string(),
        },
        None => AppEvent::PrCommentCreateFailed {
            scope_repo_id: action.scope_repo_id.clone(),
            pr_number: action.pr_number,
            mutation_id: action.mutation_id,
            error: "Application context unavailable".to_string(),
        },
    }
}
fn dispatch_pr_review_comment_create(
    app_state: &AppStateHandle,
    ctx: &SharedContext,
    repo: PrRepoTarget,
    action: PrInlineSubmitAction,
) {
    let panic_action = action.clone();
    gh_async::spawn_gh_task_with_panic(
        app_state,
        ctx,
        move |mut app_state, ctx| {
            let event = pr_review_comment_event(&ctx, &repo, &action);
            dispatch_app_event(&mut app_state, &ctx, event);
        },
        move |mut app_state, ctx, message| {
            apply_and_persist(
                &mut app_state,
                &ctx,
                AppEvent::PrCommentCreateFailed {
                    scope_repo_id: panic_action.scope_repo_id,
                    pr_number: panic_action.pr_number,
                    mutation_id: panic_action.mutation_id,
                    error: format!("GitHub review-comment task panicked: {message}"),
                },
            );
        },
    );
}

fn pr_review_comment_event(
    ctx: &SharedContext,
    repo: &PrRepoTarget,
    action: &PrInlineSubmitAction,
) -> AppEvent {
    let target = match &action.target {
        ComposerTarget::NewReviewThread { target } => Some(target),
        _ => None,
    };
    let result = github_client(ctx).zip(target).map(|(client, target)| {
        client.create_pr_review_comment(
            &repo.owner,
            &repo.repo,
            action.pr_number,
            target,
            &action.text,
        )
    });
    comment_result_event(action, result)
}

fn comment_result_event(
    action: &PrInlineSubmitAction,
    result: Option<Result<jefe::domain::IssueComment, jefe::github::GhError>>,
) -> AppEvent {
    match result {
        Some(Ok(comment)) => AppEvent::PrCommentCreated {
            scope_repo_id: action.scope_repo_id.clone(),
            pr_number: action.pr_number,
            mutation_id: action.mutation_id,
            comment,
        },
        Some(Err(error)) => AppEvent::PrCommentCreateFailed {
            scope_repo_id: action.scope_repo_id.clone(),
            pr_number: action.pr_number,
            mutation_id: action.mutation_id,
            error: error.to_string(),
        },
        None => AppEvent::PrCommentCreateFailed {
            scope_repo_id: action.scope_repo_id.clone(),
            pr_number: action.pr_number,
            mutation_id: action.mutation_id,
            error: "Application context unavailable".to_string(),
        },
    }
}

/// Spawn the gh review-thread-reply task off the UI thread.
///
/// @requirement REQ-PR-009
fn dispatch_pr_thread_reply(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    _repo: PrRepoTarget,
    action: PrInlineSubmitAction,
    thread_id: String,
) {
    let Some(deliveries) = gh_async::delivery_handle_or_report(
        app_state,
        ctx,
        pr_comment_abandoned(action.clone(), "GitHub thread reply"),
    ) else {
        return;
    };
    let panic_action = action.clone();
    gh_async::spawn_gh_work(
        &deliveries,
        ctx,
        move |ctx| pr_thread_reply_event(ctx, &action, &thread_id),
        // Route through the full dispatch chain so a successful
        // `PrCommentCreated` triggers the post-mutation detail reload
        // (issue #128). A `PrCommentCreateFailed` does not trigger a reload.
        dispatch_app_event,
        pr_comment_abandoned(panic_action, "GitHub thread reply"),
    );
}

/// Build the thread-reply-created/failed event from the gh result.
///
/// @requirement REQ-PR-009
fn pr_thread_reply_event(
    ctx: &SharedContext,
    action: &PrInlineSubmitAction,
    thread_id: &str,
) -> AppEvent {
    let result = github_client(ctx)
        .map(|client| client.create_pr_review_thread_reply(thread_id, &action.text));
    match result {
        Some(Ok(comment)) => AppEvent::PrCommentCreated {
            scope_repo_id: action.scope_repo_id.clone(),
            pr_number: action.pr_number,
            mutation_id: action.mutation_id,
            comment,
        },
        Some(Err(error)) => AppEvent::PrCommentCreateFailed {
            scope_repo_id: action.scope_repo_id.clone(),
            pr_number: action.pr_number,
            mutation_id: action.mutation_id,
            error: error.to_string(),
        },
        None => AppEvent::PrCommentCreateFailed {
            scope_repo_id: action.scope_repo_id.clone(),
            pr_number: action.pr_number,
            mutation_id: action.mutation_id,
            error: "Application context unavailable".to_string(),
        },
    }
}

/// Handle a review-thread resolve/unresolve action by spawning the gh task.
///
/// Reads the `thread_resolve_pending` state, resolves the thread_id and current
/// resolve state, and spawns the gh resolve/unresolve mutation.
///
/// @requirement REQ-PR-009
pub fn handle_pr_thread_resolve(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let Some(pending) = pr_thread_resolve_action(app_state) else {
        tracing::debug!("ignoring PR thread resolve: no pending resolve or detail");
        return;
    };
    dispatch_pr_thread_resolve(app_state, ctx, pending);
}

/// Resolve the GitHub thread node ID from a flat thread index.
fn resolve_thread_id(app_state: &AppStateHandle, thread_index: usize) -> Option<String> {
    let state = app_state.read();
    let detail = state.prs_state.pr_detail.as_ref()?;
    let thread_id = detail
        .reviews
        .iter()
        .flat_map(|r| &r.review_threads)
        .nth(thread_index)
        .map(|t| t.thread_id.clone());
    drop(state);
    thread_id
}

/// Resolve the thread resolve action from state.
fn pr_thread_resolve_action(app_state: &AppStateHandle) -> Option<ThreadResolveAction> {
    let state = app_state.read();
    let pending = state.prs_state.thread_resolve_pending.as_ref()?;
    // The pending action carries the stable thread_id captured at dispatch
    // time, so the gh mutation targets the correct thread even if a background
    // refresh reordered detail.reviews (issue #238).
    let action = ThreadResolveAction {
        scope_repo_id: pending.scope_repo_id.clone(),
        thread_index: pending.thread_index,
        resolve: pending.resolve,
        request_id: pending.request_id,
        thread_id: pending.thread_id.clone(),
    };
    drop(state);
    Some(action)
}

#[derive(Clone)]
struct ThreadResolveAction {
    scope_repo_id: jefe::domain::RepositoryId,
    thread_index: usize,
    resolve: bool,
    request_id: u64,
    thread_id: String,
}

/// Spawn the gh thread resolve/unresolve task off the UI thread.
fn dispatch_pr_thread_resolve(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    action: ThreadResolveAction,
) {
    let Some(deliveries) = gh_async::delivery_handle_or_report(
        app_state,
        ctx,
        pr_thread_resolve_abandoned(action.clone()),
    ) else {
        return;
    };
    let panic_action = action.clone();
    gh_async::spawn_gh_work(
        &deliveries,
        ctx,
        move |ctx| pr_thread_resolve_result_event(ctx, &action),
        apply_and_persist,
        pr_thread_resolve_abandoned(panic_action),
    );
}

/// Report an abandoned thread resolve so the request never stays in-flight.
fn pr_thread_resolve_abandoned(
    action: ThreadResolveAction,
) -> impl FnOnce(&mut AppStateHandle, &SharedContext, String) {
    move |app_state, ctx, message| {
        apply_and_persist(
            app_state,
            ctx,
            AppEvent::PrThreadResolveFailed {
                scope_repo_id: action.scope_repo_id,
                thread_index: action.thread_index,
                request_id: action.request_id,
                error: format!("GitHub thread resolve abandoned: {message}"),
            },
        );
    }
}

/// Build the thread-resolve result event from the gh result.
fn pr_thread_resolve_result_event(ctx: &SharedContext, action: &ThreadResolveAction) -> AppEvent {
    let result = github_client(ctx).map(|client| {
        if action.resolve {
            client.resolve_review_thread(&action.thread_id)
        } else {
            client.unresolve_review_thread(&action.thread_id)
        }
    });
    match result {
        Some(Ok(is_resolved)) => AppEvent::PrThreadResolveSucceeded {
            scope_repo_id: action.scope_repo_id.clone(),
            thread_index: action.thread_index,
            is_resolved,
            request_id: action.request_id,
        },
        Some(Err(error)) => AppEvent::PrThreadResolveFailed {
            scope_repo_id: action.scope_repo_id.clone(),
            thread_index: action.thread_index,
            request_id: action.request_id,
            error: error.to_string(),
        },
        None => AppEvent::PrThreadResolveFailed {
            scope_repo_id: action.scope_repo_id.clone(),
            thread_index: action.thread_index,
            request_id: action.request_id,
            error: "Application context unavailable".to_string(),
        },
    }
}
