//! Issues-mode dispatch helpers.
//!
//! Extracted from mod.rs to keep file sizes manageable.

use jefe::state::AppEvent;

use super::{AppStateHandle, SharedContext, apply_and_persist};

/// Resolve the GitHub owner/repo for the currently selected repository.
/// Reads from the explicit `github_repo` field (format: `"owner/repo"`).
///
/// @plan PLAN-20260329-ISSUES-MODE.P15
/// @requirement REQ-ISS-013
pub(super) fn resolve_gh_repo(state: &jefe::state::AppState) -> (String, String) {
    let repo = state
        .selected_repository_index
        .and_then(|idx| state.repositories.get(idx));

    let Some(repo) = repo else {
        return (String::new(), String::new());
    };

    let gh = repo.github_repo.trim();
    if gh.is_empty() {
        return (String::new(), String::new());
    }

    if let Some((owner, name)) = gh.split_once('/') {
        let owner = owner.trim();
        let name = name.trim();
        if !owner.is_empty() && !name.is_empty() {
            return (owner.to_owned(), name.to_owned());
        }
    }

    (String::new(), String::new())
}

pub(super) fn current_scope_repo_id(state: &jefe::state::AppState) -> jefe::domain::RepositoryId {
    state
        .selected_repository_index
        .and_then(|idx| state.repositories.get(idx))
        .map_or_else(
            || jefe::domain::RepositoryId(String::new()),
            |r| r.id.clone(),
        )
}

/// Build a lightweight issue detail preview from list data (no I/O).
/// Used for instant preview while arrowing through the issue list.
pub(super) fn preview_issue_from_list(app_state: &mut AppStateHandle) {
    let preview = {
        let state = app_state.read();
        state
            .issues_state
            .selected_issue_index
            .and_then(|idx| state.issues_state.issues.get(idx))
            .map(|issue| {
                let gh_repo = resolve_gh_repo(&state);
                jefe::domain::IssueDetail {
                    repo_owner_name: format!("{}/{}", gh_repo.0, gh_repo.1),
                    number: issue.number,
                    title: issue.title.clone(),
                    state: issue.state.clone(),
                    author_login: issue.author_login.clone(),
                    created_at: String::new(),
                    updated_at: issue.updated_at.clone(),
                    labels: issue
                        .labels_summary
                        .split(", ")
                        .filter(|s| !s.is_empty())
                        .map(String::from)
                        .collect(),
                    assignees: issue
                        .assignee_summary
                        .split(", ")
                        .filter(|s| !s.is_empty())
                        .map(String::from)
                        .collect(),
                    milestone: None,
                    body: issue.body.clone(),
                    external_url: String::new(),
                    comments: Vec::new(),
                    has_more_comments: false,
                    comments_cursor: None,
                }
            })
    };

    if let Some(detail) = preview {
        let mut state = app_state.write();
        state.issues_state.issue_detail = Some(detail);
        state.issues_state.detail_loading = false;
    }
}

/// Load issue detail for the currently selected issue in the list.
/// Used by IssuesEnter to get the full detail with comments.
pub(super) fn load_issue_detail_for_selection(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let (issue_number, scope_repo_id, owner, repo) = {
        let state = app_state.read();
        let num = state
            .issues_state
            .selected_issue_index
            .and_then(|idx| state.issues_state.issues.get(idx))
            .map(|issue| issue.number);
        let gh_repo = resolve_gh_repo(&state);
        (num, current_scope_repo_id(&state), gh_repo.0, gh_repo.1)
    };

    let Some(number) = issue_number else { return };
    if owner.is_empty() || repo.is_empty() {
        return;
    }

    // Mark detail as loading
    {
        let mut state = app_state.write();
        state.issues_state.detail_loading = true;
    }

    let result = if let Some(ctx_arc) = &ctx
        && let Ok(ctx_guard) = ctx_arc.lock()
    {
        Some(ctx_guard.gh_client.get_issue_detail(&owner, &repo, number))
    } else {
        None
    };

    match result {
        Some(Ok(detail)) => {
            apply_and_persist(
                app_state,
                ctx,
                AppEvent::IssueDetailLoaded {
                    scope_repo_id,
                    issue_number: number,
                    detail: std::boxed::Box::new(detail),
                },
            );
        }
        Some(Err(e)) => {
            apply_and_persist(
                app_state,
                ctx,
                AppEvent::IssueDetailLoadFailed {
                    scope_repo_id,
                    issue_number: number,
                    error: e.to_string(),
                },
            );
        }
        None => {}
    }
}

/// Format a `SendPayload` into a markdown issue prompt for the agent.
pub(super) fn format_issue_prompt(payload: &jefe::github::SendPayload) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "# GitHub Issue #{}: {}",
        payload.issue_number, payload.issue_title
    );
    let _ = writeln!(out);
    let _ = writeln!(out, "**Repository:** {}", payload.repository);
    let _ = writeln!(out, "**State:** {}", payload.issue_state);
    if !payload.issue_labels.is_empty() {
        let _ = writeln!(out, "**Labels:** {}", payload.issue_labels.join(", "));
    }
    if !payload.issue_assignees.is_empty() {
        let _ = writeln!(out, "**Assignees:** {}", payload.issue_assignees.join(", "));
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "## Body");
    let _ = writeln!(out);
    let _ = writeln!(out, "{}", payload.issue_body);

    if let Some(comment) = &payload.focused_comment {
        let _ = writeln!(out);
        if let Some(author) = &payload.focused_comment_author {
            let _ = writeln!(out, "## Focused Comment (by @{author})");
        } else {
            let _ = writeln!(out, "## Focused Comment");
        }
        let _ = writeln!(out);
        let _ = writeln!(out, "{comment}");
    }

    if !payload.issue_base_prompt.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "## Instructions");
        let _ = writeln!(out);
        let _ = writeln!(out, "{}", payload.issue_base_prompt);
    }

    out
}
