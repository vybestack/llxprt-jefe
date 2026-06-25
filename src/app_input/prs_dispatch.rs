//! PR-mode dispatch helpers (stub surface).
//!
//! Compiling, panic-free stubs mirroring `issues_dispatch.rs`. Every loader
//! is a TOTAL NO-OP (returns without spawning any I/O); real behavior is
//! filled in by the P10 RED -> P11 GREEN cycle. The small types
//! (`RepoContextError`, `PrOpenInBrowserInfo`) are defined here so the
//! `pr_open_in_browser_info` signature compiles.
//!
//! @plan PLAN-20260624-PR-MODE.P09
//! @requirement REQ-PR-009
//! @requirement REQ-PR-012
//! @requirement REQ-PR-013
//! @pseudocode component-004 lines 138-175
//! @pseudocode component-003 lines 176-228

use jefe::domain::RepositoryId;
use jefe::github::PrSendPayload;

use super::{AppStateHandle, SharedContext};

/// Typed unavailable-context result for PR open-in-browser (REQ-PR-013).
///
/// @plan PLAN-20260624-PR-MODE.P09
/// @requirement REQ-PR-012
/// @requirement REQ-PR-013
/// @pseudocode component-003 lines 216-228
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum RepoContextError {
    /// No PR is selected/loaded.
    NoSelection,
    /// The repository slug (`owner/name`) is missing or malformed.
    InvalidSlug,
}

/// Resolved context needed to open a PR in the browser (REQ-PR-012).
///
/// @plan PLAN-20260624-PR-MODE.P09
/// @requirement REQ-PR-012
/// @requirement REQ-PR-013
/// @pseudocode component-003 lines 217-228
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PrOpenInBrowserInfo {
    pub scope: RepositoryId,
    pub owner: String,
    pub name: String,
    pub number: u64,
}

/// Load PR detail for the currently selected PR in the list (stub — no I/O).
///
/// @plan PLAN-20260624-PR-MODE.P09
/// @requirement REQ-PR-009
/// @pseudocode component-004 lines 138-146
pub(super) fn load_pr_detail_for_selection(_app_state: &mut AppStateHandle, _ctx: &SharedContext) {
    // P11 spawns the gh detail fetch; stub returns without spawning.
}

/// Load the next comments page when the detail view is scrolled (stub — no I/O).
///
/// @plan PLAN-20260624-PR-MODE.P09
/// @requirement REQ-PR-009
/// @pseudocode component-004 lines 147-155
pub(super) fn load_more_pr_comments(_app_state: &mut AppStateHandle, _ctx: &SharedContext) {
    // P11 spawns the gh comments fetch; stub returns without spawning.
}

/// Build a lightweight PR detail preview from list data (stub — no I/O).
///
/// @plan PLAN-20260624-PR-MODE.P09
/// @requirement REQ-PR-003
/// @pseudocode component-004 lines 119-126
pub(super) fn preview_pr_from_list(_app_state: &mut AppStateHandle) {
    // P11 builds the instant preview; stub returns without mutating.
}

/// Format a `PrSendPayload` into a markdown PR prompt for the agent (stub).
///
/// @plan PLAN-20260624-PR-MODE.P09
/// @requirement REQ-PR-011
/// @pseudocode component-003 lines 176-187
#[must_use]
pub(super) fn format_pr_prompt(_payload: &PrSendPayload) -> String {
    // P11 renders the structured payload into markdown; stub returns empty.
    String::new()
}

/// Dispatch the open-in-browser side effect for the selected PR (stub — no I/O).
///
/// @plan PLAN-20260624-PR-MODE.P09
/// @requirement REQ-PR-012
/// @requirement REQ-PR-013
/// @pseudocode component-003 lines 190-215
/// @pseudocode component-004 lines 160-175
pub(super) fn dispatch_pr_open_in_browser(app_state: &AppStateHandle, _ctx: &SharedContext) {
    // P11 resolves context and spawns `gh pr view --web`; stub calls the info
    // resolver and handles the result so the symbols are reachable (no dead
    // code), then returns without spawning.
    match pr_open_in_browser_info(app_state) {
        Ok(_info) => {
            // P11 spawns `gh pr view --web` here.
        }
        Err(RepoContextError::NoSelection | RepoContextError::InvalidSlug) => {
            // P11 delivers PrShowNotice { NoSelectionToOpen } or
            // PrOpenInBrowserFailed (categorized config error).
        }
    }
}

/// Resolve the repo/owner/name/number needed to open a PR in the browser (stub).
///
/// Reads the selected PR number + repo slug. Returns `NoSelection` when no PR
/// is selected, `InvalidSlug` when the repo slug is missing/malformed, and
/// `Ok(info)` when both are well-formed (the actual `gh pr view --web` spawn
/// happens in `dispatch_pr_open_in_browser`, which is a no-op stub until P11).
///
/// @plan PLAN-20260624-PR-MODE.P09
/// @requirement REQ-PR-012
/// @requirement REQ-PR-013
/// @pseudocode component-003 lines 217-228
pub(super) fn pr_open_in_browser_info(
    app_state: &AppStateHandle,
) -> Result<PrOpenInBrowserInfo, RepoContextError> {
    let state = app_state.read();
    let number = state
        .prs_state
        .selected_pr_index
        .and_then(|idx| state.prs_state.pull_requests.get(idx))
        .map(|pr| pr.number)
        .ok_or(RepoContextError::NoSelection)?;
    let (owner, name) = resolve_pr_gh_repo(&state);
    let scope = state
        .selected_repository_index
        .and_then(|idx| state.repositories.get(idx))
        .map_or_else(
            || jefe::domain::RepositoryId(String::new()),
            |repo| repo.id.clone(),
        );
    drop(state);
    if owner.is_empty() || name.is_empty() {
        return Err(RepoContextError::InvalidSlug);
    }
    Ok(PrOpenInBrowserInfo {
        scope,
        owner,
        name,
        number,
    })
}

/// Resolve the GitHub owner/repo for the currently selected repository.
///
/// Mirrors `issues_dispatch::resolve_gh_repo`. Reads from the explicit
/// `github_repo` field (format: `"owner/repo"`).
///
/// @plan PLAN-20260624-PR-MODE.P09
/// @requirement REQ-PR-013
/// @pseudocode component-003 lines 217-228
fn resolve_pr_gh_repo(state: &jefe::state::AppState) -> (String, String) {
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
    let mut parts = gh.split('/');
    let owner = parts.next().map(str::trim).unwrap_or_default();
    let name = parts.next().map(str::trim).unwrap_or_default();
    if parts.next().is_none() && !owner.is_empty() && !name.is_empty() {
        return (owner.to_owned(), name.to_owned());
    }
    (String::new(), String::new())
}
