//! PR-mode inline-mutation dispatch helpers.
//!
//! Mirrors `issues_mutation::handle_inline_submit`. Spawns the gh PR
//! comment-create off the UI thread via `spawn_gh_task_with_panic`.
//!
//! @plan PLAN-20260624-PR-MODE.P11
//! @requirement REQ-PR-010
//! @requirement REQ-PR-011
//! @pseudocode component-003 lines 109-119

use jefe::state::{AppEvent, InlineState};

use super::{
    AppStateHandle, SharedContext, apply_and_persist, gh_async, github_client, prs_dispatch,
};

/// Handle an inline submit for PR Mode.
///
/// Reads the mutation-pending target + composer text, validates the repo, and
/// spawns `GhClient::create_pr_comment` via `spawn_gh_task_with_panic`,
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
    dispatch_pr_comment_create(app_state, ctx, repo, action);
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
    let text = match &state.prs_state.inline_state {
        InlineState::Composer { text, .. } | InlineState::Editor { text, .. } => text.clone(),
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
            apply_and_persist(&mut app_state, &ctx, event);
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
