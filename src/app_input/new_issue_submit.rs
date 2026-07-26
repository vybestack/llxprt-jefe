//! New Issue inline form submit pipeline (issue #407).
//!
//! On `NewIssueSubmit`, reads the open `issues_state.new_issue_form` state,
//! validates the title, marks the issue mutation pending, then spawns a gh
//! task that:
//!   1. Creates the issue via `GhClient::create_issue` (title + body).
//!   2. Applies labels, milestone, assignees, and issue type against the
//!      newly-created issue using the existing `edit_properties` machinery.
//!   3. Dispatches `NewIssueCreated` (success) or `NewIssueCreateFailed`
//!      (failure).
//!
//! Properties are applied after create because the GitHub REST issue-create
//! endpoint does not accept labels/milestone/type in a single call. The
//! issue's node id (returned by create) is used for the GraphQL type mutation.

use jefe::domain::RepositoryId;
use jefe::github::{
    CreatedIssue, GhClient, GhError, PropertyEditTarget, compute_assignee_diff, compute_label_diff,
};
use jefe::state::AppEvent;

use super::{AppStateHandle, SharedContext, apply_and_persist, gh_async, github_client};

/// Handle a `NewIssueSubmit`: validate, mark pending, spawn the create + apply
/// task. If the title is empty the form stays open with a validation error
/// (the reducer already surfaces this, but we double-check here to avoid
/// marking a mutation pending for an invalid submit).
pub fn handle_new_issue_submit(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let Some(params) = resolve_submit_params(app_state) else {
        return;
    };
    if params.title.trim().is_empty() {
        // Reducer already set the error; nothing to spawn.
        return;
    }
    let mutation_id = begin_mutation(app_state, ctx, params.scope_repo_id.clone());
    let panic_scope = params.scope_repo_id.clone();
    gh_async::spawn_gh_task_with_panic(
        app_state,
        ctx,
        move |mut app_state, ctx| {
            let event = create_and_apply_event(&ctx, &params, mutation_id);
            apply_and_persist(&mut app_state, &ctx, event);
        },
        move |mut app_state, ctx, message| {
            apply_and_persist(
                &mut app_state,
                &ctx,
                AppEvent::NewIssueCreateFailed {
                    scope_repo_id: panic_scope,
                    mutation_id,
                    issue_number: None,
                    error: format!("New issue task panicked: {message}"),
                },
            );
        },
    );
}

#[derive(Clone)]
struct SubmitParams {
    scope_repo_id: RepositoryId,
    owner: String,
    repo: String,
    title: String,
    body: String,
    labels: Vec<String>,
    milestone: Option<String>,
    assignees: Vec<String>,
    type_id: Option<String>,
}

fn resolve_submit_params(app_state: &AppStateHandle) -> Option<SubmitParams> {
    let state = app_state.read();
    let form = state.issues_state.new_issue_form.as_ref()?;
    let (owner, repo) = super::issues_dispatch::resolve_gh_repo(&state);
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    let scope_repo_id = super::issues_dispatch::current_scope_repo_id(&state);
    let params = SubmitParams {
        scope_repo_id,
        owner,
        repo,
        title: form.title_text.clone(),
        body: form.body_text.clone(),
        labels: form.labels.clone(),
        milestone: form.milestone.clone(),
        assignees: form.assignees.clone(),
        type_id: form.type_id.clone(),
    };
    drop(state);
    Some(params)
}

fn begin_mutation(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    scope_repo_id: RepositoryId,
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
            scope_repo_id,
            mutation_id,
            target: jefe::state::InlineState::None,
        },
    );
    mutation_id
}

fn create_and_apply_event(
    ctx: &SharedContext,
    params: &SubmitParams,
    mutation_id: u64,
) -> AppEvent {
    let Some(client) = github_client(ctx) else {
        return failure_event(params, mutation_id, None, "Application context unavailable");
    };
    match create_and_apply_properties(client, params) {
        Ok(created) => {
            let issue = created.into_list_issue();
            AppEvent::NewIssueCreated {
                scope_repo_id: params.scope_repo_id.clone(),
                mutation_id,
                issue: Box::new(issue),
            }
        }
        Err(NewIssueCreateError::CreateFailed(error)) => {
            failure_event(params, mutation_id, None, &error.to_string())
        }
        Err(NewIssueCreateError::PropertyFailed { number, error }) => {
            failure_event(params, mutation_id, Some(number), &error.to_string())
        }
    }
}

fn failure_event(
    params: &SubmitParams,
    mutation_id: u64,
    issue_number: Option<u64>,
    error: &str,
) -> AppEvent {
    AppEvent::NewIssueCreateFailed {
        scope_repo_id: params.scope_repo_id.clone(),
        mutation_id,
        issue_number,
        error: error.to_string(),
    }
}

/// Create the issue, then apply labels/milestone/assignees/type. Returns the
/// `CreatedIssue` on success. Property-apply failures are surfaced via
/// [`PropertyApplyError`] so the caller can report a partial-failure event
/// that includes the created issue number (the issue exists on GitHub even
/// though the property writes failed).
fn create_and_apply_properties(
    client: GhClient,
    params: &SubmitParams,
) -> Result<CreatedIssue, NewIssueCreateError> {
    let created = client
        .create_issue(&params.owner, &params.repo, &params.title, &params.body)
        .map_err(NewIssueCreateError::CreateFailed)?;
    let number = created.number;
    let target = PropertyEditTarget {
        owner: &params.owner,
        repo: &params.repo,
        number,
        is_pr: false,
    };
    // Apply each property independently so a single failure does not hide the
    // issue number from the user. The first property error short-circuits; the
    // created issue still exists on GitHub and is reported in the event.
    apply_labels(client, target, &params.labels)
        .map_err(|e| NewIssueCreateError::PropertyFailed { number, error: e })?;
    apply_assignees(client, target, &params.assignees)
        .map_err(|e| NewIssueCreateError::PropertyFailed { number, error: e })?;
    apply_milestone(client, target, params.milestone.as_deref())
        .map_err(|e| NewIssueCreateError::PropertyFailed { number, error: e })?;
    apply_issue_type(client, &created.node_id, params.type_id.as_deref())
        .map_err(|e| NewIssueCreateError::PropertyFailed { number, error: e })?;
    Ok(created)
}

/// Distinguishes a create failure (no issue was created) from a
/// property-apply failure (the issue exists on GitHub; the user should be told
/// its number so they can finish the properties by hand).
#[derive(Debug)]
enum NewIssueCreateError {
    CreateFailed(GhError),
    PropertyFailed { number: u64, error: GhError },
}

impl std::fmt::Display for NewIssueCreateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CreateFailed(e) => write!(f, "{e}"),
            Self::PropertyFailed { number, error } => {
                write!(f, "Issue #{number} created but a property failed: {error}")
            }
        }
    }
}

fn apply_labels(
    client: GhClient,
    target: PropertyEditTarget,
    labels: &[String],
) -> Result<(), GhError> {
    // Filter empty/whitespace-only labels in one pass so an accidental blank
    // entry in the multi-select does not produce a gh error (issue #407).
    let desired: Vec<String> = labels
        .iter()
        .filter(|l| !l.trim().is_empty())
        .cloned()
        .collect();
    if desired.is_empty() {
        return Ok(());
    }
    let (to_add, _to_remove) = compute_label_diff(&[], &desired);
    client.edit_labels(target, &to_add, &[])
}

fn apply_assignees(
    client: GhClient,
    target: PropertyEditTarget,
    assignees: &[String],
) -> Result<(), GhError> {
    let desired: Vec<String> = assignees
        .iter()
        .filter(|a| !a.trim().is_empty())
        .cloned()
        .collect();
    if desired.is_empty() {
        return Ok(());
    }
    let (to_add, _to_remove) = compute_assignee_diff(&[], &desired);
    client.edit_assignees(target, &to_add, &[])
}

fn apply_milestone(
    client: GhClient,
    target: PropertyEditTarget,
    milestone: Option<&str>,
) -> Result<(), GhError> {
    let Some(milestone) = milestone else {
        return Ok(());
    };
    if milestone.trim().is_empty() {
        return Ok(());
    }
    client.set_milestone(
        target.owner,
        target.repo,
        target.number,
        target.is_pr,
        milestone,
    )
}

/// Apply the issue type using the create-response `node_id` directly,
/// avoiding an extra `fetch_issue_node_info` round-trip (issue #407).
fn apply_issue_type(client: GhClient, node_id: &str, type_id: Option<&str>) -> Result<(), GhError> {
    let Some(type_id) = type_id else {
        return Ok(());
    };
    if type_id.trim().is_empty() {
        return Ok(());
    }
    client.set_issue_type(node_id, Some(type_id))
}
