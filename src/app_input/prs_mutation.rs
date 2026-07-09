//! PR-mode inline-mutation dispatch helpers.
//!
//! Mirrors `issues_mutation::handle_inline_submit`. Spawns the gh PR
//! comment-create off the UI thread via `spawn_gh_task_with_panic`.
//!
//! @plan PLAN-20260624-PR-MODE.P11
//! @requirement REQ-PR-010
//! @requirement REQ-PR-011
//! @pseudocode component-003 lines 109-119

use jefe::state::{AppEvent, ComposerTarget, InlineState};

use super::{
    AppStateHandle, SharedContext, apply_and_persist, dispatch_app_event, gh_async, github_client,
    prs_dispatch,
};

/// Handle an inline submit for PR Mode.
///
/// Reads the mutation-pending target + composer text, validates the repo, and
/// spawns the gh comment/reply task via `spawn_gh_task_with_panic`,
/// delivering `PrCommentCreated` on success or `PrCommentCreateFailed` on
/// Err/panic.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-010
/// @requirement REQ-PR-011
/// @pseudocode component-003 lines 109-119
pub fn handle_pr_inline_submit(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let Some(action) = resolve_pr_inline_submit(app_state) else {
        tracing::debug!("ignoring PR inline submit: no pending mutation or composer text");
        return;
    };
    let Some(repo) = pr_repo_target(app_state) else {
        report_missing_github_repo(app_state, ctx, &action);
        return;
    };
    if let ComposerTarget::ReplyToReviewThread { thread_index, .. } = &action.target {
        let Some(thread_id) = resolve_thread_id(app_state, *thread_index) else {
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
    } else {
        dispatch_pr_comment_create(app_state, ctx, repo, action);
    }
}

/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-010
/// @pseudocode component-004 lines 146-155
#[derive(Clone)]
struct PrInlineSubmitAction {
    scope_repo_id: jefe::domain::RepositoryId,
    pr_number: u64,
    mutation_id: u64,
    text: String,
    target: ComposerTarget,
}

/// Resolve the inline-submit action from state (mutation_pending + composer text).
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 310-325
fn resolve_pr_inline_submit(app_state: &AppStateHandle) -> Option<PrInlineSubmitAction> {
    let state = app_state.read();
    let pending = state.prs_state.mutation_pending.as_ref()?;
    let pr_number = state.prs_state.pr_detail.as_ref()?.number;
    let (text, target) = match &state.prs_state.inline_state {
        InlineState::Composer { text, target, .. } => (text.clone(), target.clone()),
        InlineState::Editor { text, .. } => (text.clone(), ComposerTarget::NewComment),
        InlineState::None => return None,
    };
    if text.trim().is_empty() {
        return None;
    }
    let action = PrInlineSubmitAction {
        scope_repo_id: pending.scope_repo_id.clone(),
        pr_number,
        mutation_id: pending.mutation_id,
        text,
        target,
    };
    drop(state);
    Some(action)
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
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-010
/// @pseudocode component-004 lines 146-155
fn pr_repo_target(app_state: &AppStateHandle) -> Option<PrRepoTarget> {
    let state = app_state.read();
    let (owner, repo) = prs_dispatch::resolve_pr_gh_repo(&state);
    (!owner.is_empty() && !repo.is_empty()).then_some(PrRepoTarget { owner, repo })
}

/// Report a missing GitHub repo as a mutation failure (synchronous).
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-013
/// @pseudocode component-004 lines 146-155
fn report_missing_github_repo(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    action: &PrInlineSubmitAction,
) {
    apply_and_persist(
        app_state,
        ctx,
        AppEvent::PrCommentCreateFailed {
            scope_repo_id: action.scope_repo_id.clone(),
            pr_number: action.pr_number,
            mutation_id: action.mutation_id,
            error: "No GitHub repository configured. Set the GitHub Repo field (owner/repo) in repository settings.".to_string(),
        },
    );
}

/// Spawn the gh PR comment-create task off the UI thread.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-010
/// @pseudocode component-004 lines 146-155
fn dispatch_pr_comment_create(
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
            let event = pr_comment_create_event(&ctx, &repo, &action);
            // Route through the full dispatch chain so a successful
            // `PrCommentCreated` triggers the post-mutation detail reload
            // (issue #128). A `PrCommentCreateFailed` does not trigger a reload.
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
                    error: format!("GitHub PR comment task panicked: {message}"),
                },
            );
        },
    );
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

/// Spawn the gh review-thread-reply task off the UI thread.
///
/// @requirement REQ-PR-009
fn dispatch_pr_thread_reply(
    app_state: &AppStateHandle,
    ctx: &SharedContext,
    _repo: PrRepoTarget,
    action: PrInlineSubmitAction,
    thread_id: String,
) {
    let panic_action = action.clone();
    gh_async::spawn_gh_task_with_panic(
        app_state,
        ctx,
        move |mut app_state, ctx| {
            let event = pr_thread_reply_event(&ctx, &action, &thread_id);
            // Route through the full dispatch chain so a successful
            // `PrCommentCreated` triggers the post-mutation detail reload
            // (issue #128). A `PrCommentCreateFailed` does not trigger a reload.
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
                    error: format!("GitHub thread reply task panicked: {message}"),
                },
            );
        },
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
pub fn handle_pr_thread_resolve(app_state: &AppStateHandle, ctx: &SharedContext) {
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
    let detail = state.prs_state.pr_detail.as_ref()?;
    let thread = detail
        .reviews
        .iter()
        .flat_map(|r| &r.review_threads)
        .nth(pending.thread_index)?;
    let action = ThreadResolveAction {
        scope_repo_id: pending.scope_repo_id.clone(),
        thread_index: pending.thread_index,
        resolve: pending.resolve,
        request_id: pending.request_id,
        thread_id: thread.thread_id.clone(),
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
    app_state: &AppStateHandle,
    ctx: &SharedContext,
    action: ThreadResolveAction,
) {
    let panic_action = action.clone();
    gh_async::spawn_gh_task_with_panic(
        app_state,
        ctx,
        move |mut app_state, ctx| {
            let event = pr_thread_resolve_result_event(&ctx, &action);
            apply_and_persist(&mut app_state, &ctx, event);
        },
        move |mut app_state, ctx, message| {
            apply_and_persist(
                &mut app_state,
                &ctx,
                AppEvent::PrThreadResolveFailed {
                    scope_repo_id: panic_action.scope_repo_id,
                    thread_index: panic_action.thread_index,
                    request_id: panic_action.request_id,
                    error: format!("GitHub thread resolve task panicked: {message}"),
                },
            );
        },
    );
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
