//! Non-blocking changed-file loading for the PR Changes drill-down.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use jefe::domain::{PrFileBlob, RepositoryId};
use jefe::state::{
    AppEvent, PrChangesBlobLoadFailedPayload, PrChangesBlobLoadedPayload,
    PrChangesLoadFailedPayload, PrChangesLoadedPayload,
};

use super::prs_dispatch::{current_pr_scope_repo_id, resolve_pr_gh_repo_or_error};
use super::{AppStateHandle, SharedContext, apply_and_persist, gh_async, github_client};

#[derive(Clone)]
struct PrChangesLoadParams {
    scope_repo_id: RepositoryId,
    pr_number: u64,
    head_sha: String,
    owner: String,
    repo: String,
    request_id: u64,
}

#[derive(Clone)]
struct PrBlobLoadParams {
    scope_repo_id: RepositoryId,
    pr_number: u64,
    owner: String,
    repo: String,
    request_id: u64,
    blob_sha: String,
    local_dir: Option<PathBuf>,
}

const MAX_FULL_FILE_BYTES: u64 = 1_048_576;

/// Apply Changes entry and start the correlated changed-files read.
pub(super) fn open_and_load_changes(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    apply_and_persist(app_state, ctx, AppEvent::PrOpenChanges);
    load_pending_changes(app_state, ctx);
}

/// Start a correlated changed-files read when the reducer staged one (used by
/// both initial open and retry after failure).
pub(super) fn load_pending_changes(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let params = match changes_load_params(app_state) {
        Ok(Some(params)) => params,
        Ok(None) => return,
        Err(event) => {
            apply_and_persist(app_state, ctx, *event);
            return;
        }
    };
    let panic_params = params.clone();
    gh_async::spawn_gh_task_with_panic(
        app_state,
        ctx,
        move |mut app_state, ctx| {
            let event = changes_load_event(&ctx, &params);
            // Settle auth failures into the failed state before opening auth
            // remediation (issue #376): apply the failure event first so the
            // reducer records the terminal failure, then offer remediation.
            if let AppEvent::PrChangesLoadFailed(payload) = &event {
                let auth_error = payload.error.clone();
                apply_and_persist(&mut app_state, &ctx, event);
                super::auth_remediation::offer_auth_remediation(&mut app_state, &ctx, &auth_error);
                return;
            }
            apply_and_persist(&mut app_state, &ctx, event);
        },
        move |mut app_state, ctx, message| {
            apply_and_persist(
                &mut app_state,
                &ctx,
                failure_event(
                    &panic_params,
                    format!("GitHub PR files task panicked: {message}"),
                ),
            );
        },
    );
}
/// Start a correlated lazy full-file read when the reducer staged one.
pub(super) fn load_pending_blob(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let params = match blob_load_params(app_state) {
        Ok(Some(params)) => params,
        Ok(None) => return,
        Err(event) => {
            apply_and_persist(app_state, ctx, *event);
            return;
        }
    };
    // Record the dispatched request_id so subsequent calls for the same
    // pending request are skipped (edge-triggered dispatch, issue #376).
    let dispatched_request_id = params.request_id;
    {
        let mut state = app_state.write();
        state.prs_state.changes.blob_dispatched_request_id = Some(dispatched_request_id);
    }
    let panic_params = params.clone();
    gh_async::spawn_gh_task_with_panic(
        app_state,
        ctx,
        move |mut app_state, ctx| {
            let event = blob_load_event(&ctx, &params);
            apply_and_persist(&mut app_state, &ctx, event);
        },
        move |mut app_state, ctx, message| {
            apply_and_persist(
                &mut app_state,
                &ctx,
                blob_failure_event(&panic_params, format!("PR blob task panicked: {message}")),
            );
        },
    );
}

fn blob_load_params(app_state: &AppStateHandle) -> Result<Option<PrBlobLoadParams>, Box<AppEvent>> {
    let state = app_state.read();
    let Some(pending) = state.prs_state.changes.blob_pending.as_ref() else {
        return Ok(None);
    };
    // Edge-triggered dispatch: skip spawning a task when one is already in
    // flight for this exact request_id, so repeated navigation/content events
    // cannot duplicate work for one pending request (issue #376).
    if state.prs_state.changes.blob_dispatched_request_id == Some(pending.request_id) {
        return Ok(None);
    }
    let (owner, repo) = resolve_pr_gh_repo_or_error(&state)
        .map_err(|error| Box::new(blob_failure_from_pending(pending, error.message)))?;
    let local_dir = state
        .selected_repository()
        .filter(|repo| !repo.remote.enabled)
        .map(|repo| repo.base_dir.clone());
    let params = PrBlobLoadParams {
        scope_repo_id: pending.scope_repo_id.clone(),
        pr_number: pending.pr_number,
        owner,
        repo,
        request_id: pending.request_id,
        blob_sha: pending.blob_sha.clone(),
        local_dir,
    };
    drop(state);
    Ok(Some(params))
}

fn blob_load_event(ctx: &SharedContext, params: &PrBlobLoadParams) -> AppEvent {
    let result = read_local_blob(params).or_else(|| {
        github_client(ctx).map(|client| {
            client
                .get_pr_file_blob(&params.owner, &params.repo, &params.blob_sha)
                .map_err(|error| error.to_string())
        })
    });
    match result {
        Some(Ok(blob)) => AppEvent::PrChangesBlobLoaded(PrChangesBlobLoadedPayload {
            scope_repo_id: params.scope_repo_id.clone(),
            pr_number: params.pr_number,
            request_id: params.request_id,
            blob_sha: params.blob_sha.clone(),
            blob,
        }),
        Some(Err(error)) => blob_failure_event(params, error),
        None => blob_failure_event(params, "Application context unavailable".to_string()),
    }
}
/// Build the read-only `git -C <dir> cat-file -s <oid>` size-probe argv.
///
/// `<dir>` and `<oid>` are passed as single arguments even when they contain
/// spaces or Unicode so Git receives exactly the intended paths.
fn cat_file_size_argv(directory: &Path, oid: &str) -> Vec<OsString> {
    vec![
        OsString::from("-C"),
        directory.as_os_str().to_owned(),
        OsString::from("cat-file"),
        OsString::from("-s"),
        OsString::from(oid),
    ]
}

/// Build the read-only `git -C <dir> cat-file blob <oid>` content argv.
fn cat_file_blob_argv(directory: &Path, oid: &str) -> Vec<OsString> {
    vec![
        OsString::from("-C"),
        directory.as_os_str().to_owned(),
        OsString::from("cat-file"),
        OsString::from("blob"),
        OsString::from(oid),
    ]
}

/// Result of classifying a `git cat-file -s` probe's output.
enum LocalSizeProbe {
    /// The object is absent or the size could not be read; fall back to GitHub.
    Missing,
    /// The object exists with the given byte size.
    Bytes(u64),
}

/// Classify a `git cat-file -s` probe's raw output into a local-miss or a
/// known byte size. A non-zero exit, unparseable stdout, or absent object
/// yields [`LocalSizeProbe::Missing`] so the caller falls back to the
/// authoritative GitHub blob read without surfacing a partial semantic result.
fn classify_local_size_probe(
    stdout: &[u8],
    _stderr: &[u8],
    status: std::process::ExitStatus,
) -> LocalSizeProbe {
    if !status.success() {
        return LocalSizeProbe::Missing;
    }
    let Ok(text) = std::str::from_utf8(stdout) else {
        return LocalSizeProbe::Missing;
    };
    match text.trim().parse::<u64>() {
        Ok(byte_size) => LocalSizeProbe::Bytes(byte_size),
        Err(_) => LocalSizeProbe::Missing,
    }
}

/// Classify raw blob bytes from a successful `git cat-file blob` into the
/// display contract.
///
/// NUL bytes use Git's binary heuristic: a valid-UTF-8 byte stream that still
/// contains a NUL is treated as binary. Non-UTF-8 content yields `None` (a
/// local miss) so the caller falls back to GitHub's authoritative `isBinary`
/// metadata rather than misrepresenting the file as text.
fn classify_local_blob_bytes(bytes: Vec<u8>) -> Option<Result<PrFileBlob, String>> {
    let Ok(text) = String::from_utf8(bytes) else {
        return None;
    };
    if text.as_bytes().contains(&0u8) {
        Some(Ok(PrFileBlob::Binary))
    } else {
        Some(Ok(PrFileBlob::Text(text)))
    }
}

fn read_local_blob(params: &PrBlobLoadParams) -> Option<Result<PrFileBlob, String>> {
    let directory = params.local_dir.as_ref()?;
    let mut size = jefe::local_command::command(jefe::local_command::LocalTool::Git).ok()?;
    let output = size
        .args(cat_file_size_argv(directory, &params.blob_sha))
        .output()
        .ok()?;
    let size_probe = classify_local_size_probe(&output.stdout, &output.stderr, output.status);
    let byte_size = match size_probe {
        LocalSizeProbe::Missing => return None,
        LocalSizeProbe::Bytes(byte_size) => byte_size,
    };
    if byte_size > MAX_FULL_FILE_BYTES {
        return Some(Ok(PrFileBlob::Truncated { byte_size }));
    }
    let mut content = jefe::local_command::command(jefe::local_command::LocalTool::Git).ok()?;
    let output = content
        .args(cat_file_blob_argv(directory, &params.blob_sha))
        .output()
        .ok()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Some(Err(format!("git cat-file blob failed: {stderr}")));
    }
    classify_local_blob_bytes(output.stdout)
}

fn blob_failure_event(params: &PrBlobLoadParams, error: String) -> AppEvent {
    AppEvent::PrChangesBlobLoadFailed(PrChangesBlobLoadFailedPayload {
        scope_repo_id: params.scope_repo_id.clone(),
        pr_number: params.pr_number,
        request_id: params.request_id,
        blob_sha: params.blob_sha.clone(),
        error,
    })
}

fn blob_failure_from_pending(
    pending: &jefe::state::PrChangesBlobPending,
    error: String,
) -> AppEvent {
    AppEvent::PrChangesBlobLoadFailed(PrChangesBlobLoadFailedPayload {
        scope_repo_id: pending.scope_repo_id.clone(),
        pr_number: pending.pr_number,
        request_id: pending.request_id,
        blob_sha: pending.blob_sha.clone(),
        error,
    })
}

fn changes_load_params(
    app_state: &AppStateHandle,
) -> Result<Option<PrChangesLoadParams>, Box<AppEvent>> {
    let state = app_state.read();
    let Some(pending) = state.prs_state.changes.pending.as_ref() else {
        return Ok(None);
    };
    let (owner, repo) = resolve_pr_gh_repo_or_error(&state)
        .map_err(|error| Box::new(failure_event_from_pending(pending, error.message)))?;
    if owner.is_empty() || repo.is_empty() {
        return Err(Box::new(failure_event_from_pending(
            pending,
            "No GitHub repository configured for PR changes".to_string(),
        )));
    }
    let params = PrChangesLoadParams {
        scope_repo_id: current_pr_scope_repo_id(&state),
        pr_number: pending.pr_number,
        head_sha: pending.head_sha.clone(),
        owner,
        repo,
        request_id: pending.request_id,
    };
    drop(state);
    Ok(Some(params))
}

fn changes_load_event(ctx: &SharedContext, params: &PrChangesLoadParams) -> AppEvent {
    match github_client(ctx)
        .map(|client| client.list_pr_files(&params.owner, &params.repo, params.pr_number))
    {
        Some(Ok(response)) => AppEvent::PrChangesLoaded(PrChangesLoadedPayload {
            scope_repo_id: params.scope_repo_id.clone(),
            pr_number: params.pr_number,
            request_id: params.request_id,
            head_sha: params.head_sha.clone(),
            files: response.files,
            truncated: response.truncated,
        }),
        Some(Err(error)) => failure_event(params, error.to_string()),
        None => failure_event(params, "Application context unavailable".to_string()),
    }
}

fn failure_event(params: &PrChangesLoadParams, error: String) -> AppEvent {
    AppEvent::PrChangesLoadFailed(PrChangesLoadFailedPayload {
        scope_repo_id: params.scope_repo_id.clone(),
        pr_number: params.pr_number,
        request_id: params.request_id,
        head_sha: params.head_sha.clone(),
        error,
    })
}

fn failure_event_from_pending(pending: &jefe::state::PrChangesPending, error: String) -> AppEvent {
    AppEvent::PrChangesLoadFailed(PrChangesLoadFailedPayload {
        scope_repo_id: pending.scope_repo_id.clone(),
        pr_number: pending.pr_number,
        request_id: pending.request_id,
        head_sha: pending.head_sha.clone(),
        error,
    })
}

#[cfg(test)]
#[path = "prs_diff_dispatch_tests.rs"]
mod tests;
