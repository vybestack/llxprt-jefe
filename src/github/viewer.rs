//! Viewer (authenticated user) resolution and issue self-assignment helpers
//! for send-to-agent (issue #186).
//!
//! All functions here are pure so the exact `gh` command shape can be unit
//! tested without a process boundary; the [`super::GhClient`] methods wrap
//! them with the actual `gh` subprocess invocation.

use super::GhError;
use serde_json::Value;

/// Parse the `gh api user` response into the authenticated viewer's login.
///
/// Accepts either the REST shape (`{"login": "..."}`) or a `--jq .login`
/// bare-string output. A bare string is validated as a GitHub login component
/// via [`crate::domain::is_valid_github_component`] so a garbled multiline `gh`
/// output is rejected rather than forwarded to the assignment request.
pub fn parse_viewer_login(output: &str) -> Result<String, GhError> {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return Err(GhError::ParseError(
            "Missing viewer login in user response".to_string(),
        ));
    }
    if !trimmed.starts_with('{') {
        let line = trimmed.lines().next().unwrap_or_default().trim_matches('"');
        return validate_login(line);
    }
    let value: Value = serde_json::from_str(trimmed)
        .map_err(|e| GhError::ParseError(format!("Invalid JSON: {e}")))?;
    validate_login(
        value
            .get("login")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    )
}

/// Validate a resolved login as a nonempty GitHub component, returning it
/// owned on success. Centralizes the plausible-login check shared by the bare
/// and JSON parse paths.
fn validate_login(login: &str) -> Result<String, GhError> {
    if !login.is_empty() && crate::domain::is_valid_github_component(login) {
        Ok(login.to_string())
    } else {
        Err(GhError::ParseError(
            "Missing or invalid viewer login".to_string(),
        ))
    }
}

/// Build the `gh api user --jq .login` argument vector for viewer resolution.
/// Pure so the exact command shape is unit-testable.
#[must_use]
pub fn build_viewer_login_args() -> Vec<String> {
    vec![
        "api".to_string(),
        "user".to_string(),
        "--jq".to_string(),
        ".login".to_string(),
    ]
}

/// Build the `gh api` argument vector for assigning an issue to `assignee`.
/// Pure so the exact endpoint and array-field shape are unit-testable.
#[must_use]
pub fn build_assign_issue_args(
    owner: &str,
    repo: &str,
    number: u64,
    assignee: &str,
) -> Vec<String> {
    vec![
        "api".to_string(),
        "--method".to_string(),
        "POST".to_string(),
        format!("/repos/{owner}/{repo}/issues/{number}/assignees"),
        "-f".to_string(),
        format!("assignees[]={assignee}"),
    ]
}
