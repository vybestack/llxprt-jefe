//! Orchestration for the agent-driven new-issue draft rewrite (issue #214).
//!
//! Triggered from the new-issue composer by `Ctrl+R`. Reads the current draft,
//! resolves the configured default agent for the focused repository, runs that
//! agent **non-interactively** (single prompt → stdout → exit) so it can study
//! the repository source and produce a cleaner, plan-like issue, then replaces
//! the composer draft with the rewritten text for review before submission.
//!
//! The deterministic state transitions live in the reducer
//! (`state::issues_rewrite_ops`); this module owns only the boundary I/O
//! (availability probe, agent subprocess, applying the result events).

use jefe::domain::{LaunchSignature, Repository, build_rewrite_instruction};
use jefe::runtime::run_non_interactive;
use jefe::state::{AppEvent, AppState, InlineState};

use super::{
    AppStateHandle, SharedContext, apply_and_persist, gh_async, launch_signature_for_transient,
};

/// Entry point dispatched from `IssuesMessage::RequestIssueRewrite`.
///
/// Resolves the rewrite context from the current state, applies the pending
/// flag, and spawns the non-interactive agent run off the UI thread. Result
/// events (`IssueRewriteSucceeded` / `IssueRewriteFailed`) are applied back via
/// `apply_and_persist`.
pub(super) fn handle_request_issue_rewrite(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let context = match rewrite_context(app_state) {
        Ok(None) => {
            // No actionable rewrite request (not a NewIssue composer, no draft,
            // no repo, or already pending). The reducer still consumes the
            // event as a no-op; nothing to spawn.
            return;
        }
        Err(error) => {
            // A resolvable precondition failed (e.g. the working directory
            // could not be determined). Surface it as a non-fatal failure so
            // the draft is preserved and the user is informed.
            apply_and_persist(app_state, ctx, AppEvent::IssueRewriteFailed { error });
            return;
        }
        Ok(Some(context)) => context,
    };

    // Availability guard BEFORE applying the pending flag: a missing agent
    // runtime must not flip the UI into a waiting state.
    if !super::availability::launch_available_or_error(
        app_state,
        context.signature.agent_kind,
        context.signature.llxprt_version.as_ref(),
        &context.signature.code_puppy_version,
        &context.signature.remote,
    ) {
        return;
    }

    // Record the pending state so the UI shows the rewrite is in flight.
    apply_and_persist(app_state, ctx, AppEvent::RequestIssueRewrite);

    let RewriteContext {
        instruction,
        signature,
    } = context;

    gh_async::spawn_gh_task_with_panic(
        app_state,
        ctx,
        move |mut app_state, ctx| match run_non_interactive(
            &signature,
            signature.work_dir.as_path(),
            &instruction,
        ) {
            Ok(text) => {
                apply_and_persist(
                    &mut app_state,
                    &ctx,
                    AppEvent::IssueRewriteSucceeded { text },
                );
            }
            Err(error) => {
                apply_and_persist(
                    &mut app_state,
                    &ctx,
                    AppEvent::IssueRewriteFailed {
                        error: error.to_string(),
                    },
                );
            }
        },
        move |mut app_state, ctx, message| {
            apply_and_persist(
                &mut app_state,
                &ctx,
                AppEvent::IssueRewriteFailed {
                    error: format!("Agent rewrite task panicked: {message}"),
                },
            );
        },
    );
}

/// Resolved context required to run a non-interactive rewrite.
struct RewriteContext {
    instruction: String,
    signature: LaunchSignature,
}

/// Resolve the rewrite context from the current state.
///
/// - `Ok(None)` when there is no actionable request (no NewIssue composer,
///   empty draft, a rewrite already in flight, or no repository selected).
/// - `Err(msg)` when a resolvable precondition failed (e.g. the working
///   directory cannot be determined) and should be surfaced to the user.
///
/// `resolve_rewrite_context_from_state` is the pure (testable) core; this
/// wrapper only acquires the read lock.
fn rewrite_context(app_state: &AppStateHandle) -> Result<Option<RewriteContext>, String> {
    let state = app_state.read();
    let result = resolve_rewrite_context_from_state(&state);
    drop(state);
    result
}

/// Pure resolver: extract the composer draft and build the launch signature +
/// instruction from the focused repository's configured default agent.
fn resolve_rewrite_context_from_state(state: &AppState) -> Result<Option<RewriteContext>, String> {
    if state.issues_state.rewrite_pending {
        return Ok(None);
    }
    let Some(draft) = new_issue_composer_draft(state) else {
        return Ok(None);
    };
    let Some(repository) = focused_repository(state) else {
        return Ok(None);
    };

    // The agent runs in the repository's local working copy so it can study
    // the source while rewriting the issue text. Fall back to the process
    // working directory only when the repository has no configured base_dir.
    let work_dir = if repository.base_dir.as_os_str().is_empty() {
        std::env::current_dir().map_err(|_| {
            "Could not resolve the working directory for the agent rewrite".to_owned()
        })?
    } else {
        repository.base_dir.clone()
    };
    let signature = launch_signature_for_transient(repository, &work_dir);

    let trimmed_repo = repository.github_repo.trim();
    let github_repo = if trimmed_repo.is_empty() {
        None
    } else {
        Some(trimmed_repo)
    };
    let instruction = build_rewrite_instruction(&draft, github_repo);
    Ok(Some(RewriteContext {
        instruction,
        signature,
    }))
}

/// The current NewIssue composer draft text, or `None` if the composer is not
/// active for a new issue or the draft is empty.
fn new_issue_composer_draft(state: &AppState) -> Option<String> {
    match &state.issues_state.inline_state {
        InlineState::Composer {
            target: jefe::state::ComposerTarget::NewIssue,
            text,
            ..
        } if !text.trim().is_empty() => Some(text.clone()),
        _ => None,
    }
}

/// The focused repository, or `None` when none is selected.
fn focused_repository(state: &AppState) -> Option<&Repository> {
    state
        .selected_repository_index
        .and_then(|idx| state.repositories.get(idx))
}

#[cfg(test)]
#[path = "issues_rewrite_dispatch_tests.rs"]
mod tests;
