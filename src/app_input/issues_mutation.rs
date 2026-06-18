//! Issues-mode mutation dispatch helpers.

use jefe::state::AppEvent;

use super::{AppStateHandle, SharedContext, dispatch_app_event, issues_dispatch};

pub(super) fn handle_inline_submit(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let submit_action = {
        let state = app_state.read();
        match &state.issues_state.inline_state {
            jefe::state::InlineState::Composer { target, text, .. } => {
                Some(InlineSubmitAction::Create {
                    target: target.clone(),
                    text: text.clone(),
                })
            }
            jefe::state::InlineState::Editor { target, text, .. } => {
                Some(InlineSubmitAction::Edit {
                    target: *target,
                    text: text.clone(),
                })
            }
            jefe::state::InlineState::None => None,
        }
    };

    let Some(action) = submit_action else {
        return;
    };

    let (owner, repo) = {
        let state = app_state.read();
        issues_dispatch::resolve_gh_repo(&state)
    };

    if owner.is_empty() || repo.is_empty() {
        apply_mutation_failed(
            app_state,
            "No GitHub repository configured. Set the GitHub Repo field (owner/repo) in repository settings.".to_string(),
        );
        return;
    }

    match action {
        InlineSubmitAction::Create { target, text } => {
            if target == jefe::state::ComposerTarget::NewIssue {
                create_issue(app_state, ctx, &owner, &repo, text);
            } else {
                create_comment(app_state, ctx, &owner, &repo, text);
            }
        }
        InlineSubmitAction::Edit { target, text } => match target {
            jefe::state::EditorTarget::IssueBody => {
                update_issue_body(app_state, ctx, &owner, &repo, text);
            }
            jefe::state::EditorTarget::Comment { comment_index } => {
                update_comment(app_state, ctx, &owner, &repo, comment_index, text);
            }
        },
    }
}

fn create_issue(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    owner: &str,
    repo: &str,
    text: String,
) {
    let (title, body) = if let Some((first, rest)) = text.split_once('\n') {
        (first.trim().to_string(), rest.to_string())
    } else {
        (text.trim().to_string(), String::new())
    };

    if title.is_empty() {
        apply_mutation_failed(app_state, "Issue title cannot be empty".to_string());
        return;
    }

    let result = if let Some(ctx_arc) = ctx
        && let Ok(ctx_guard) = ctx_arc.lock()
    {
        Some(ctx_guard.gh_client.create_issue(owner, repo, &title, &body))
    } else {
        None
    };

    match result {
        Some(Ok(issue)) => {
            {
                let mut state = app_state.write();
                state.issues_state.inline_state = jefe::state::InlineState::None;
                state.issues_state.error = None;
                state.issues_state.draft_notice = Some(format!("Created issue #{}", issue.number));
            }
            dispatch_app_event(app_state, ctx, AppEvent::RefocusIssueList);
        }
        Some(Err(e)) => apply_mutation_failed(app_state, e.to_string()),
        None => report_context_unavailable(app_state),
    }
}

fn create_comment(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    owner: &str,
    repo: &str,
    text: String,
) {
    let issue_number = {
        let state = app_state.read();
        state.issues_state.issue_detail.as_ref().map(|d| d.number)
    };
    let Some(number) = issue_number else { return };

    let result = if let Some(ctx_arc) = ctx
        && let Ok(ctx_guard) = ctx_arc.lock()
    {
        Some(
            ctx_guard
                .gh_client
                .create_comment(owner, repo, number, &text),
        )
    } else {
        None
    };

    match result {
        Some(Ok(comment)) => {
            let mut state = app_state.write();
            *state = std::mem::take(&mut *state).apply(AppEvent::CommentCreated { comment });
        }
        Some(Err(e)) => {
            let mut state = app_state.write();
            *state = std::mem::take(&mut *state).apply(AppEvent::CommentCreateFailed {
                error: e.to_string(),
            });
        }
        None => report_context_unavailable(app_state),
    }
}

fn update_issue_body(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    owner: &str,
    repo: &str,
    text: String,
) {
    let issue_number = {
        let state = app_state.read();
        state.issues_state.issue_detail.as_ref().map(|d| d.number)
    };
    let Some(number) = issue_number else { return };

    let result = if let Some(ctx_arc) = ctx
        && let Ok(ctx_guard) = ctx_arc.lock()
    {
        Some(
            ctx_guard
                .gh_client
                .update_issue_body(owner, repo, number, &text),
        )
    } else {
        None
    };

    match result {
        Some(Ok(())) => {
            let mut state = app_state.write();
            *state = std::mem::take(&mut *state).apply(AppEvent::IssueBodyUpdated { body: text });
        }
        Some(Err(e)) => apply_mutation_failed(app_state, e.to_string()),
        None => report_context_unavailable(app_state),
    }
}

fn update_comment(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    owner: &str,
    repo: &str,
    comment_index: usize,
    text: String,
) {
    let comment_id = {
        let state = app_state.read();
        state
            .issues_state
            .issue_detail
            .as_ref()
            .and_then(|d| d.comments.get(comment_index))
            .map(|c| c.comment_id)
    };
    let Some(cid) = comment_id else { return };

    let result = if let Some(ctx_arc) = ctx
        && let Ok(ctx_guard) = ctx_arc.lock()
    {
        Some(ctx_guard.gh_client.update_comment(owner, repo, cid, &text))
    } else {
        None
    };

    match result {
        Some(Ok(())) => {
            let mut state = app_state.write();
            *state = std::mem::take(&mut *state).apply(AppEvent::CommentUpdated {
                comment_index,
                body: text,
            });
        }
        Some(Err(e)) => apply_mutation_failed(app_state, e.to_string()),
        None => report_context_unavailable(app_state),
    }
}

fn apply_mutation_failed(app_state: &mut AppStateHandle, error: String) {
    let mut state = app_state.write();
    *state = std::mem::take(&mut *state).apply(AppEvent::MutationFailed { error });
}

fn report_context_unavailable(app_state: &mut AppStateHandle) {
    apply_mutation_failed(app_state, "Application context unavailable".to_string());
}

enum InlineSubmitAction {
    Create {
        target: jefe::state::ComposerTarget,
        text: String,
    },
    Edit {
        target: jefe::state::EditorTarget,
        text: String,
    },
}
