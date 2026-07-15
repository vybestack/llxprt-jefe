//! Issues-mode mutation dispatch helpers.

use jefe::state::AppEvent;

use super::{
    AppStateHandle, SharedContext, apply_and_persist, gh_async, github_client, issues_dispatch,
};

pub(super) fn handle_inline_submit(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let action = match inline_submit_action(app_state) {
        InlineSubmitResolution::Submit(action) => action,
        InlineSubmitResolution::AlreadyPending => {
            tracing::debug!(
                "ignoring inline submit: a mutation is already in flight (double-submit guard)"
            );
            return;
        }
        InlineSubmitResolution::NoInlineDraft => {
            tracing::warn!(
                "inline submit dispatched with no active composer/editor; ignoring no-op"
            );
            return;
        }
    };
    match gh_repo_target(app_state) {
        Ok(Some(repo)) => dispatch_inline_submit_action(app_state, ctx, repo, action),
        Ok(None) => report_missing_github_repo(app_state, ctx, None),
        Err(message) => report_missing_github_repo(app_state, ctx, Some(message)),
    }
}

fn inline_submit_action(app_state: &AppStateHandle) -> InlineSubmitResolution {
    let state = app_state.read();
    if state.issues_state.mutation_pending.is_some() {
        return InlineSubmitResolution::AlreadyPending;
    }
    match &state.issues_state.inline_state {
        jefe::state::InlineState::Composer { target, text, .. } => {
            InlineSubmitResolution::Submit(InlineSubmitAction::Create {
                pending_target: state.issues_state.inline_state.clone(),
                target: target.clone(),
                text: text.clone(),
            })
        }
        jefe::state::InlineState::Editor { target, text, .. } => {
            InlineSubmitResolution::Submit(InlineSubmitAction::Edit {
                pending_target: state.issues_state.inline_state.clone(),
                target: *target,
                text: text.clone(),
            })
        }
        jefe::state::InlineState::None => InlineSubmitResolution::NoInlineDraft,
    }
}

fn gh_repo_target(app_state: &AppStateHandle) -> Result<Option<GhRepoTarget>, String> {
    let state = app_state.read();
    match issues_dispatch::resolve_gh_repo_or_error(&state) {
        Ok((owner, repo)) if !owner.is_empty() && !repo.is_empty() => {
            Ok(Some(GhRepoTarget { owner, repo }))
        }
        Ok(_) => Ok(None),
        Err(error) => Err(error.message),
    }
}

/// Report a missing or malformed GitHub repo as a mutation failure
/// (synchronous). When `malformed` is `Some`, the typed malformed reason is
/// surfaced instead of the generic "missing GitHub Repo" message (issue #266).
fn report_missing_github_repo(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    malformed: Option<String>,
) {
    let error = malformed.unwrap_or_else(|| {
        "No GitHub repository configured. Set the GitHub Repo field (owner/repo) in repository settings.".to_string()
    });
    let target = mutation_failure_target(app_state);
    apply_mutation_failed(app_state, ctx, target, None, error);
}

fn dispatch_inline_submit_action(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    repo: GhRepoTarget,
    action: InlineSubmitAction,
) {
    match action {
        InlineSubmitAction::Create {
            pending_target,
            target,
            text,
        } => {
            if target == jefe::state::ComposerTarget::NewIssue {
                create_issue(app_state, ctx, repo, pending_target, text);
            } else {
                let mutation_id = begin_mutation(app_state, ctx, pending_target);
                create_comment(app_state, ctx, repo, mutation_id, text);
            }
        }
        InlineSubmitAction::Edit {
            pending_target,
            target,
            text,
        } => dispatch_inline_edit(app_state, ctx, repo, pending_target, target, text),
    }
}

fn dispatch_inline_edit(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    repo: GhRepoTarget,
    pending_target: jefe::state::InlineState,
    target: jefe::state::EditorTarget,
    text: String,
) {
    let mutation_id = begin_mutation(app_state, ctx, pending_target);
    match target {
        jefe::state::EditorTarget::IssueBody => {
            update_issue_body(app_state, ctx, repo, mutation_id, text);
        }
        jefe::state::EditorTarget::Comment { comment_index } => {
            update_comment(app_state, ctx, repo, mutation_id, comment_index, text);
        }
    }
}

fn begin_mutation(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    target: jefe::state::InlineState,
) -> u64 {
    let mutation_id = {
        let mut state = app_state.write();
        state.issues_state.next_mutation_id = state.issues_state.next_mutation_id.saturating_add(1);
        state.issues_state.next_mutation_id
    };
    apply_and_persist(
        app_state,
        ctx,
        AppEvent::MutationSubmitted {
            scope_repo_id: mutation_failure_target(app_state).scope_repo_id,
            mutation_id,
            target,
        },
    );
    mutation_id
}

/// Split composer text into an issue title (first line, trimmed) and body
/// (remaining lines verbatim).
fn split_issue_title_body(text: &str) -> (String, String) {
    if let Some((first, rest)) = text.split_once('\n') {
        (first.trim().to_string(), rest.to_string())
    } else {
        (text.trim().to_string(), String::new())
    }
}

fn create_issue(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    repo_target: GhRepoTarget,
    pending_target: jefe::state::InlineState,
    text: String,
) {
    let (title, body) = split_issue_title_body(&text);

    if title.is_empty() {
        apply_mutation_failed(
            app_state,
            ctx,
            mutation_failure_target(app_state),
            None,
            "Issue title cannot be empty".to_string(),
        );
        return;
    }

    let mutation_id = begin_mutation(app_state, ctx, pending_target);
    let failure_target = mutation_failure_target(app_state);
    let panic_failure_target = failure_target.clone();
    // Capture the originally-submitted scope so a late-arriving success is
    // attributed to the repository that was active at submission time, not to
    // whatever the user has since selected.
    let created_scope = failure_target.scope_repo_id.clone();
    gh_async::spawn_gh_task_with_panic(
        app_state,
        ctx,
        move |mut app_state, ctx| {
            let result = github_client(&ctx).map(|client| {
                client.create_issue(&repo_target.owner, &repo_target.repo, &title, &body)
            });

            match result {
                Some(Ok(created)) => {
                    let issue = created.into_list_issue();
                    let created_number = issue.number;
                    apply_and_persist(
                        &mut app_state,
                        &ctx,
                        AppEvent::IssueCreated {
                            scope_repo_id: created_scope,
                            mutation_id,
                            issue,
                        },
                    );
                    // Optimistic list insert already selected the new issue.
                    // Avoid RefocusIssueList (clears notice + races search indexing)
                    // and avoid load_issue_detail_for_selection (late detail events
                    // can race the create selection / notice). Seed a sync preview
                    // from the list row instead; Enter still loads full detail.
                    let preview_created = {
                        let state = app_state.read();
                        state
                            .issues_state
                            .selected_issue_index()
                            .and_then(|idx| state.issues_state.issues().get(idx))
                            .is_some_and(|issue| issue.number == created_number)
                    };
                    if preview_created {
                        issues_dispatch::preview_issue_from_list(&mut app_state);
                    }
                }
                Some(Err(e)) => {
                    apply_mutation_failed(
                        &mut app_state,
                        &ctx,
                        failure_target,
                        Some(mutation_id),
                        e.to_string(),
                    );
                }
                None => {
                    report_context_unavailable(&mut app_state, &ctx, failure_target, mutation_id);
                }
            }
        },
        move |mut app_state, ctx, message| {
            apply_mutation_failed(
                &mut app_state,
                &ctx,
                panic_failure_target,
                Some(mutation_id),
                format!("GitHub issue create task panicked: {message}"),
            );
        },
    );
}

fn create_comment(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    repo_target: GhRepoTarget,
    mutation_id: u64,
    text: String,
) {
    let Some(target) = current_issue_target(app_state) else {
        let failure_target = mutation_failure_target(app_state);
        apply_mutation_failed(
            app_state,
            ctx,
            failure_target,
            Some(mutation_id),
            "No issue loaded for this comment".to_string(),
        );
        return;
    };
    let failure_target = target.failure_target();
    let panic_failure_target = failure_target.clone();

    gh_async::spawn_gh_task_with_panic(
        app_state,
        ctx,
        move |mut app_state, ctx| {
            let event = create_comment_event(
                &ctx,
                &repo_target.owner,
                &repo_target.repo,
                &text,
                &target,
                mutation_id,
            );
            match event {
                Some(event) => apply_and_persist(&mut app_state, &ctx, event),
                None => {
                    report_context_unavailable(&mut app_state, &ctx, failure_target, mutation_id);
                }
            }
        },
        move |mut app_state, ctx, message| {
            apply_mutation_failed(
                &mut app_state,
                &ctx,
                panic_failure_target,
                Some(mutation_id),
                format!("GitHub comment create task panicked: {message}"),
            );
        },
    );
}

fn update_issue_body(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    repo_target: GhRepoTarget,
    mutation_id: u64,
    text: String,
) {
    let (title, body) = split_issue_title_body(&text);
    if title.is_empty() {
        apply_mutation_failed(
            app_state,
            ctx,
            mutation_failure_target(app_state),
            Some(mutation_id),
            "Issue title cannot be empty".to_string(),
        );
        return;
    }
    let Some(target) = current_issue_target(app_state) else {
        let failure_target = mutation_failure_target(app_state);
        apply_mutation_failed(
            app_state,
            ctx,
            failure_target,
            Some(mutation_id),
            "No issue loaded to update".to_string(),
        );
        return;
    };
    let failure_target = target.failure_target();
    let panic_failure_target = failure_target.clone();

    gh_async::spawn_gh_task_with_panic(
        app_state,
        ctx,
        move |mut app_state, ctx| {
            let event =
                update_issue_body_event(&ctx, &repo_target, &title, &body, &target, mutation_id);
            match event {
                Some(event) => apply_and_persist(&mut app_state, &ctx, event),
                None => {
                    report_context_unavailable(&mut app_state, &ctx, failure_target, mutation_id);
                }
            }
        },
        move |mut app_state, ctx, message| {
            apply_mutation_failed(
                &mut app_state,
                &ctx,
                panic_failure_target,
                Some(mutation_id),
                format!("GitHub issue body update task panicked: {message}"),
            );
        },
    );
}

fn update_comment(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    repo_target: GhRepoTarget,
    mutation_id: u64,
    comment_index: usize,
    text: String,
) {
    let Some(target) = comment_update_target(app_state, comment_index) else {
        let failure_target = mutation_failure_target(app_state);
        apply_mutation_failed(
            app_state,
            ctx,
            failure_target,
            Some(mutation_id),
            "Comment no longer exists".to_string(),
        );
        return;
    };
    let failure_target = target.failure_target();
    let panic_failure_target = failure_target.clone();

    gh_async::spawn_gh_task_with_panic(
        app_state,
        ctx,
        move |mut app_state, ctx| {
            let event = update_comment_event(
                &ctx,
                &repo_target.owner,
                &repo_target.repo,
                &text,
                &target,
                mutation_id,
            );
            match event {
                Some(event) => apply_and_persist(&mut app_state, &ctx, event),
                None => {
                    report_context_unavailable(&mut app_state, &ctx, failure_target, mutation_id);
                }
            }
        },
        move |mut app_state, ctx, message| {
            apply_mutation_failed(
                &mut app_state,
                &ctx,
                panic_failure_target,
                Some(mutation_id),
                format!("GitHub comment update task panicked: {message}"),
            );
        },
    );
}

fn create_comment_event(
    ctx: &SharedContext,
    owner: &str,
    repo: &str,
    text: &str,
    target: &IssueMutationTarget,
    mutation_id: u64,
) -> Option<AppEvent> {
    github_client(ctx)
        .map(|client| client.create_comment(owner, repo, target.issue_number, text))
        .map(|result| match result {
            Ok(comment) => AppEvent::CommentCreated {
                scope_repo_id: target.scope_repo_id.clone(),
                issue_number: target.issue_number,
                mutation_id,
                comment,
            },
            Err(error) => AppEvent::CommentCreateFailed {
                scope_repo_id: target.scope_repo_id.clone(),
                issue_number: target.issue_number,
                mutation_id,
                error: error.to_string(),
            },
        })
}

fn update_issue_body_event(
    ctx: &SharedContext,
    repo: &GhRepoTarget,
    title: &str,
    body: &str,
    target: &IssueMutationTarget,
    mutation_id: u64,
) -> Option<AppEvent> {
    github_client(ctx)
        .map(|client| {
            client.update_issue(&repo.owner, &repo.repo, target.issue_number, title, body)
        })
        .map(|result| match result {
            Ok(()) => AppEvent::IssueBodyUpdated {
                scope_repo_id: target.scope_repo_id.clone(),
                issue_number: target.issue_number,
                mutation_id,
                title: title.to_string(),
                body: body.to_string(),
            },
            Err(error) => AppEvent::MutationFailed {
                scope_repo_id: target.scope_repo_id.clone(),
                issue_number: Some(target.issue_number),
                mutation_id: Some(mutation_id),
                error: error.to_string(),
            },
        })
}

fn update_comment_event(
    ctx: &SharedContext,
    owner: &str,
    repo: &str,
    text: &str,
    target: &CommentMutationTarget,
    mutation_id: u64,
) -> Option<AppEvent> {
    github_client(ctx)
        .map(|client| client.update_comment(owner, repo, target.comment_id, text))
        .map(|result| match result {
            Ok(()) => AppEvent::CommentUpdated {
                scope_repo_id: target.scope_repo_id.clone(),
                issue_number: target.issue_number,
                mutation_id,
                comment_id: target.comment_id,
                comment_index: target.comment_index,
                body: text.to_string(),
            },
            Err(error) => AppEvent::MutationFailed {
                scope_repo_id: target.scope_repo_id.clone(),
                issue_number: Some(target.issue_number),
                mutation_id: Some(mutation_id),
                error: error.to_string(),
            },
        })
}

fn apply_mutation_failed(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    target: MutationFailureTarget,
    mutation_id: Option<u64>,
    error: String,
) {
    apply_and_persist(
        app_state,
        ctx,
        AppEvent::MutationFailed {
            scope_repo_id: target.scope_repo_id,
            issue_number: target.issue_number,
            mutation_id,
            error,
        },
    );
}

fn report_context_unavailable(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    target: MutationFailureTarget,
    mutation_id: u64,
) {
    apply_mutation_failed(
        app_state,
        ctx,
        target,
        Some(mutation_id),
        "Application context unavailable".to_string(),
    );
}

fn current_issue_target(app_state: &AppStateHandle) -> Option<IssueMutationTarget> {
    let state = app_state.read();
    let issue_number = state.issues_state.issue_detail.as_ref()?.number;
    let target = IssueMutationTarget {
        scope_repo_id: issues_dispatch::current_scope_repo_id(&state),
        issue_number,
    };
    drop(state);
    Some(target)
}

fn comment_update_target(
    app_state: &AppStateHandle,
    comment_index: usize,
) -> Option<CommentMutationTarget> {
    let state = app_state.read();
    let detail = state.issues_state.issue_detail.as_ref()?;
    let comment = detail.comments.get(comment_index)?;
    let target = CommentMutationTarget {
        scope_repo_id: issues_dispatch::current_scope_repo_id(&state),
        issue_number: detail.number,
        comment_id: comment.comment_id,
        comment_index,
    };
    drop(state);
    Some(target)
}

fn mutation_failure_target(app_state: &AppStateHandle) -> MutationFailureTarget {
    let state = app_state.read();
    let target = MutationFailureTarget {
        scope_repo_id: issues_dispatch::current_scope_repo_id(&state),
        issue_number: state.issues_state.issue_detail.as_ref().map(|d| d.number),
    };
    drop(state);
    target
}

#[derive(Clone)]
struct IssueMutationTarget {
    scope_repo_id: jefe::domain::RepositoryId,
    issue_number: u64,
}

impl IssueMutationTarget {
    fn failure_target(&self) -> MutationFailureTarget {
        MutationFailureTarget {
            scope_repo_id: self.scope_repo_id.clone(),
            issue_number: Some(self.issue_number),
        }
    }
}

#[derive(Clone)]
struct CommentMutationTarget {
    scope_repo_id: jefe::domain::RepositoryId,
    issue_number: u64,
    comment_id: u64,
    comment_index: usize,
}

impl CommentMutationTarget {
    fn failure_target(&self) -> MutationFailureTarget {
        MutationFailureTarget {
            scope_repo_id: self.scope_repo_id.clone(),
            issue_number: Some(self.issue_number),
        }
    }
}

#[derive(Clone)]
struct GhRepoTarget {
    owner: String,
    repo: String,
}

#[derive(Clone)]
struct MutationFailureTarget {
    scope_repo_id: jefe::domain::RepositoryId,
    issue_number: Option<u64>,
}

enum InlineSubmitAction {
    Create {
        pending_target: jefe::state::InlineState,
        target: jefe::state::ComposerTarget,
        text: String,
    },
    Edit {
        pending_target: jefe::state::InlineState,
        target: jefe::state::EditorTarget,
        text: String,
    },
}

/// Outcome of resolving an inline-submit request.
///
/// Distinguishes an actionable submission from the two quiet no-op cases so the
/// dispatcher can record why nothing happened instead of silently returning.
enum InlineSubmitResolution {
    /// There is a composer/editor draft ready to submit.
    Submit(InlineSubmitAction),
    /// A mutation is already in flight; the submit is debounced.
    AlreadyPending,
    /// No composer/editor is active (no draft to submit).
    NoInlineDraft,
}
