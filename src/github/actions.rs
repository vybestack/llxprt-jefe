use super::{GhClient, GhError, categorize_error};
use crate::domain::{
    ActionsFilter, Workflow, WorkflowRun, WorkflowRunConclusion, WorkflowRunDetail, WorkflowRunJob,
    WorkflowRunStatus, WorkflowRunStep,
};
use std::fmt::Write;

/// Percent-encode a SINGLE URL path segment (RFC 3986). Keeps unreserved
/// characters (`A-Za-z0-9-._~`) verbatim; encodes everything else, including
/// `/` (as `%2F`) so a stray full path can never silently leak slashes into
/// one segment and reproduce the #206 404.
///
/// This is intentionally segment-only: it does NOT preserve `/` as a path
/// separator. Callers building multi-segment paths must join already-encoded
/// segments with literal `/` themselves; do not pass a composite path here.
fn percent_encode_path(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for &b in value.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char);
            }
            _ => {
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}

/// Return the bare workflow filename (last path segment) of a workflow
/// file path. The GitHub REST API
/// `repos/{owner}/{repo}/actions/workflows/{workflow_id_or_filename}/runs`
/// endpoint accepts the workflow's filename (e.g. `ci.yml`) but NOT the
/// full `.github/workflows/ci.yml` path — literal slashes in the path
/// segment make the API route to a different resource and return 404.
///
/// Trailing slashes are trimmed first so a non-canonical path like
/// `.github/workflows/ci.yml/` resolves to `ci.yml` rather than an empty
/// segment (which would produce a malformed `/workflows//runs` URL).
#[must_use]
fn workflow_filename(path: &str) -> &str {
    let trimmed = path.trim_end_matches('/');
    match trimmed.rsplit_once('/') {
        Some((_, name)) => name,
        None => trimmed,
    }
}

/// Percent-encode a value for use in a URL query component (RFC 3986). Keeps
/// unreserved characters verbatim and encodes reserved/sub-delims that would
/// alter the query structure (`& = # +` etc.).
fn percent_encode_query(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for &b in value.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char);
            }
            _ => {
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}

// Response from listing runs
pub struct WorkflowRunListResponse {
    pub runs: Vec<WorkflowRun>,
    pub total_count: u64,
    pub has_more: bool,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhWorkflowJson {
    id: u64,
    name: String,
    path: String,
    state: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhWorkflowRunJson {
    #[serde(rename = "databaseId")]
    id: u64,
    name: String,
    head_branch: String,
    head_sha: String,
    #[serde(rename = "number")]
    number: u32,
    event: String,
    status: String,
    conclusion: Option<String>,
    workflow_name: String,
    created_at: String,
    updated_at: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
struct GhApiWorkflowRunJson {
    id: u64,
    name: Option<String>,
    display_title: Option<String>,
    head_branch: Option<String>,
    head_sha: String,
    run_number: u32,
    event: String,
    status: String,
    conclusion: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhJobJson {
    #[serde(rename = "databaseId")]
    id: u64,
    name: String,
    status: String,
    conclusion: Option<String>,
    steps: Option<Vec<GhStepJson>>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhStepJson {
    name: String,
    status: String,
    conclusion: Option<String>,
    number: u32,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
struct GhApiRunsResponse {
    total_count: Option<u64>,
    workflow_runs: Option<Vec<GhApiWorkflowRunJson>>,
}

fn map_status(status: &str) -> WorkflowRunStatus {
    match status.to_ascii_lowercase().as_str() {
        "completed" => WorkflowRunStatus::Completed,
        "in_progress" | "in-progress" => WorkflowRunStatus::InProgress,
        "queued" => WorkflowRunStatus::Queued,
        "requested" => WorkflowRunStatus::Requested,
        "waiting" => WorkflowRunStatus::Waiting,
        "pending" => WorkflowRunStatus::Pending,
        _ => WorkflowRunStatus::Unknown,
    }
}

fn map_conclusion(conclusion: &str) -> WorkflowRunConclusion {
    match conclusion.to_ascii_lowercase().as_str() {
        "success" => WorkflowRunConclusion::Success,
        "failure" => WorkflowRunConclusion::Failure,
        "cancelled" => WorkflowRunConclusion::Cancelled,
        "skipped" => WorkflowRunConclusion::Skipped,
        "timed_out" | "timed-out" => WorkflowRunConclusion::TimedOut,
        "action_required" | "action-required" => WorkflowRunConclusion::ActionRequired,
        "stale" => WorkflowRunConclusion::Stale,
        "neutral" => WorkflowRunConclusion::Neutral,
        "startup_failure" | "startup-failure" => WorkflowRunConclusion::StartupFailure,
        _ => WorkflowRunConclusion::Unknown,
    }
}

pub fn parse_workflows_json(json: &str) -> Result<Vec<Workflow>, GhError> {
    let raw: Vec<GhWorkflowJson> =
        serde_json::from_str(json).map_err(|e| GhError::ParseError(e.to_string()))?;
    Ok(raw
        .into_iter()
        .map(|w| Workflow {
            id: w.id,
            name: w.name,
            path: w.path,
            state: w.state,
        })
        .collect())
}

pub fn parse_runs_json(json: &str) -> Result<Vec<WorkflowRun>, GhError> {
    let raw: Vec<GhWorkflowRunJson> =
        serde_json::from_str(json).map_err(|e| GhError::ParseError(e.to_string()))?;
    Ok(raw.into_iter().map(map_run).collect())
}

pub fn parse_single_run_json(json: &str) -> Result<WorkflowRun, GhError> {
    let raw: GhWorkflowRunJson =
        serde_json::from_str(json).map_err(|e| GhError::ParseError(e.to_string()))?;
    Ok(map_run(raw))
}

pub fn parse_api_runs_json(json: &str) -> Result<(Vec<WorkflowRun>, u64), GhError> {
    let response: GhApiRunsResponse =
        serde_json::from_str(json).map_err(|e| GhError::ParseError(e.to_string()))?;
    let runs = response
        .workflow_runs
        .unwrap_or_default()
        .into_iter()
        .map(map_api_run)
        .collect();
    let total = response.total_count.unwrap_or(0);
    Ok((runs, total))
}

fn map_run(r: GhWorkflowRunJson) -> WorkflowRun {
    WorkflowRun {
        id: r.id,
        name: r.name,
        head_branch: r.head_branch,
        head_sha: r.head_sha,
        run_number: r.number,
        event: r.event,
        status: map_status(&r.status),
        conclusion: r.conclusion.as_deref().map(map_conclusion),
        workflow_name: r.workflow_name,
        created_at: r.created_at,
        updated_at: r.updated_at,
    }
}

fn map_api_run(r: GhApiWorkflowRunJson) -> WorkflowRun {
    let workflow_name = r.name.unwrap_or_default();
    let name = r.display_title.unwrap_or_else(|| workflow_name.clone());
    WorkflowRun {
        id: r.id,
        name,
        head_branch: r.head_branch.unwrap_or_default(),
        head_sha: r.head_sha,
        run_number: r.run_number,
        event: r.event,
        status: map_status(&r.status),
        conclusion: r.conclusion.as_deref().map(map_conclusion),
        workflow_name,
        created_at: r.created_at,
        updated_at: r.updated_at,
    }
}

pub fn parse_jobs_json(json: &str) -> Result<Vec<WorkflowRunJob>, GhError> {
    let raw: Vec<GhJobJson> =
        serde_json::from_str(json).map_err(|e| GhError::ParseError(e.to_string()))?;
    Ok(raw
        .into_iter()
        .map(|j| WorkflowRunJob {
            id: j.id,
            name: j.name,
            status: map_status(&j.status),
            conclusion: j.conclusion.as_deref().map(map_conclusion),
            steps: j
                .steps
                .unwrap_or_default()
                .into_iter()
                .map(|s| WorkflowRunStep {
                    name: s.name,
                    status: map_status(&s.status),
                    conclusion: s.conclusion.as_deref().map(map_conclusion),
                    number: s.number,
                })
                .collect(),
        })
        .collect())
}

fn run_gh<S: AsRef<std::ffi::OsStr>>(args: &[S]) -> Result<String, GhError> {
    let output = super::gh_command()?.args(args).output().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            GhError::NotInstalled
        } else {
            GhError::NetworkError(e.to_string())
        }
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(categorize_error(output.status.code().unwrap_or(1), &stderr));
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Build the GitHub API path for listing workflow runs with filters.
///
/// Pure function extracted from `list_runs` for unit testability. When a
/// workflow filter is set, only the bare **filename** of `workflow_path`
/// (its last `/`-separated segment, via [`workflow_filename`]) is sent as
/// the `actions/workflows/{filename}/runs` endpoint segment — the GitHub
/// REST API rejects the full `.github/workflows/...` path (literal slashes
/// route to a different resource, returning HTTP 404) and also rejects the
/// `workflow` display name with HTTP 404.
///
/// The sentinel ("all") and emptiness checks run against the normalized
/// filename, so non-canonical inputs (`"all/"`, `"/"`, `"///"`, `""`) all
/// fall through to the generic `actions/runs` endpoint rather than producing
/// a malformed workflow-specific URL.
#[must_use]
pub fn build_runs_api_path(
    owner: &str,
    repo: &str,
    filter: &ActionsFilter,
    page: u32,
    per_page: u32,
) -> String {
    // Normalize once: the sentinel ("all") and emptiness checks run against
    // the SAME value used in the path segment, so "all/", "/", "///", and ""
    // all route to the generic runs endpoint instead of producing malformed
    // workflow-specific URLs like /workflows/all/runs or /workflows//runs.
    let workflow_id = workflow_filename(&filter.workflow_path);
    let workflow_enc = percent_encode_path(workflow_id);
    let status_enc = percent_encode_query(&filter.status);
    let mut api_path = if workflow_id != "all" && !workflow_id.is_empty() {
        format!(
            "repos/{owner}/{repo}/actions/workflows/{workflow_enc}/runs?page={page}&per_page={per_page}"
        )
    } else {
        format!("repos/{owner}/{repo}/actions/runs?page={page}&per_page={per_page}")
    };

    if filter.status != "all" && !filter.status.is_empty() {
        let _ = write!(api_path, "&status={status_enc}");
    }

    if let Some(ref sha) = filter.head_sha {
        let sha_enc = percent_encode_query(sha);
        let _ = write!(api_path, "&event=pull_request&head_sha={sha_enc}");
    }

    api_path
}

impl GhClient {
    /// List active/running/completed workflow runs for a repository with pagination/filters.
    pub fn list_runs(
        &self,
        owner: &str,
        repo: &str,
        filter: &ActionsFilter,
        page: u32,
        per_page: u32,
    ) -> Result<WorkflowRunListResponse, GhError> {
        let api_path = build_runs_api_path(owner, repo, filter, page, per_page);

        let stdout = run_gh(&["api", &api_path])?;
        let (runs, total_count) = parse_api_runs_json(&stdout)?;
        let has_more = u64::from(page) * u64::from(per_page) < total_count;

        Ok(WorkflowRunListResponse {
            runs,
            total_count,
            has_more,
        })
    }

    /// Retrieve detailed workflow run information, including jobs and steps.
    pub fn get_run_detail(
        &self,
        owner: &str,
        repo: &str,
        run_id: u64,
    ) -> Result<WorkflowRunDetail, GhError> {
        let repo_arg = format!("{owner}/{repo}");
        let run_id_arg = run_id.to_string();

        // Fetch run basic details
        let run_stdout = run_gh(&[
            "run",
            "view",
            "--repo",
            &repo_arg,
            &run_id_arg,
            "--json",
            "attempt,conclusion,createdAt,databaseId,displayTitle,event,headBranch,headSha,name,number,startedAt,status,updatedAt,url,workflowDatabaseId,workflowName",
        ])?;
        let run = parse_single_run_json(&run_stdout)?;

        // Fetch jobs and steps details
        let jobs_stdout = run_gh(&[
            "run",
            "view",
            "--repo",
            &repo_arg,
            &run_id_arg,
            "--json",
            "jobs",
            "--jq",
            ".jobs",
        ])?;
        let jobs = parse_jobs_json(&jobs_stdout)?;

        Ok(WorkflowRunDetail { run, jobs })
    }

    /// List all workflows in the repository.
    pub fn list_workflows(&self, owner: &str, repo: &str) -> Result<Vec<Workflow>, GhError> {
        let api_path = format!("repos/{owner}/{repo}/actions/workflows");
        let stdout = run_gh(&["api", &api_path, "--jq", ".workflows"])?;
        parse_workflows_json(&stdout)
    }

    /// Trigger a workflow run manually with dispatch parameters.
    pub fn dispatch_workflow(
        &self,
        owner: &str,
        repo: &str,
        workflow_id: &str,
        ref_name: &str,
        inputs: &[(String, String)],
    ) -> Result<(), GhError> {
        let mut args = vec![
            "workflow".to_string(),
            "run".to_string(),
            "--repo".to_string(),
            format!("{owner}/{repo}"),
            workflow_id.to_string(),
            "--ref".to_string(),
            ref_name.to_string(),
            // End of options — every subsequent `-f KEY=VALUE` arg is a
            // positional input, never parsed as a flag. Without this, a
            // user-supplied ref/inputs value starting with `-` (e.g. a branch
            // named `-rf`) would be misread by `gh` as an option flag.
            "--".to_string(),
        ];

        for (k, v) in inputs {
            args.push("-f".to_string());
            args.push(format!("{k}={v}"));
        }

        run_gh(&args)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ActionsFilter;

    /// `percent_encode_path` must encode `/` as `%2F` so a stray full path
    /// passed without going through `workflow_filename` can never silently
    /// leak slashes into a single path segment (the #206 404 regression).
    #[test]
    fn percent_encode_path_encodes_slash() {
        let encoded = percent_encode_path(".github/workflows/ci.yml");
        assert!(
            encoded.contains("%2F"),
            "slash must be percent-encoded, got: {encoded}"
        );
        assert!(
            !encoded.contains('/'),
            "no literal slash may remain, got: {encoded}"
        );
    }

    /// A non-canonical trailing slash must not collapse the filename to an
    /// empty segment (which would yield a malformed `/workflows//runs` URL).
    #[test]
    fn build_runs_api_path_trailing_slash_still_uses_filename() {
        let filter = ActionsFilter {
            workflow: "CI".to_string(),
            workflow_path: ".github/workflows/ci.yml/".to_string(),
            ..ActionsFilter::default()
        };
        let path = build_runs_api_path("owner", "repo", &filter, 1, 30);
        assert_eq!(
            path, "repos/owner/repo/actions/workflows/ci.yml/runs?page=1&per_page=30",
            "trailing slash must be trimmed before extracting the filename, got: {path}"
        );
    }

    /// The API path must use the workflow FILENAME (last path segment), not
    /// the full `.github/workflows/ci.yml` path. The GitHub REST endpoint
    /// `actions/workflows/{workflow_id_or_filename}/runs` rejects the full
    /// path with HTTP 404 because the literal slashes split the path into
    /// wrong segments.
    #[test]
    fn build_runs_api_path_uses_workflow_filename_not_full_path() {
        let filter = ActionsFilter {
            workflow: "CI".to_string(),
            workflow_path: ".github/workflows/ci.yml".to_string(),
            ..ActionsFilter::default()
        };
        let path = build_runs_api_path("owner", "repo", &filter, 1, 30);
        assert!(
            path.contains("/workflows/ci.yml/runs"),
            "API path must contain the bare workflow filename, got: {path}"
        );
        assert!(
            !path.contains(".github/"),
            "API path must NOT leak the full directory path, got: {path}"
        );
        assert!(
            !path.contains("%2F"),
            "API path must NOT contain encoded slashes, got: {path}"
        );
        assert!(
            !path.contains("/workflows/CI/"),
            "API path must NOT contain the display name, got: {path}"
        );
    }

    /// A nested workflow path like `.github/workflows/ocr-review.yml` must
    /// resolve to the bare filename `ocr-review.yml` in the API path segment.
    #[test]
    fn build_runs_api_path_uses_filename_for_nested_workflow_path() {
        let filter = ActionsFilter {
            workflow: "OCR Review".to_string(),
            workflow_path: ".github/workflows/ocr-review.yml".to_string(),
            ..ActionsFilter::default()
        };
        let path = build_runs_api_path("owner", "repo", &filter, 1, 30);
        // Exact contract: only the bare filename appears as the workflow
        // segment; the `.github/workflows/` prefix must not leak into the URL.
        assert_eq!(
            path, "repos/owner/repo/actions/workflows/ocr-review.yml/runs?page=1&per_page=30",
            "API path must use the bare workflow filename, got: {path}"
        );
    }

    /// A workflow path that is already just a filename (no directory
    /// separator) must be used unchanged. Defensive: guards against future
    /// state changes where the path may be normalized to a bare filename.
    #[test]
    fn build_runs_api_path_filename_without_directory_separator() {
        let filter = ActionsFilter {
            workflow: "CI".to_string(),
            workflow_path: "ci.yml".to_string(),
            ..ActionsFilter::default()
        };
        let path = build_runs_api_path("owner", "repo", &filter, 1, 30);
        assert!(
            path.contains("/workflows/ci.yml/runs"),
            "API path must contain the bare filename, got: {path}"
        );
    }

    /// When workflow_path is empty (all workflows), the path uses the generic
    /// actions/runs endpoint.
    #[test]
    fn build_runs_api_path_no_workflow_filter() {
        let filter = ActionsFilter::default();
        let path = build_runs_api_path("owner", "repo", &filter, 1, 30);
        assert!(
            path.ends_with("/actions/runs?page=1&per_page=30"),
            "expected generic runs endpoint, got: {path}"
        );
    }

    /// Non-canonical "all" / empty inputs must fall through to the generic
    /// runs endpoint, not a malformed workflow-specific URL. The sentinel is
    /// checked against the normalized filename, so "all/" and "///" resolve
    /// to "all" / "" respectively.
    #[test]
    fn build_runs_api_path_non_canonical_all_routes_to_generic_endpoint() {
        for raw in ["all/", "/", "///", ""] {
            let filter = ActionsFilter {
                workflow_path: raw.to_string(),
                ..ActionsFilter::default()
            };
            let path = build_runs_api_path("owner", "repo", &filter, 1, 30);
            assert!(
                path.ends_with("/actions/runs?page=1&per_page=30"),
                "raw {raw:?} must route to the generic runs endpoint, got: {path}"
            );
            assert!(
                !path.contains("/workflows/"),
                "raw {raw:?} must not produce a workflow-specific URL, got: {path}"
            );
        }
    }

    /// Status filter is appended as a query parameter.
    #[test]
    fn build_runs_api_path_with_status_filter() {
        let filter = ActionsFilter {
            status: "failed".to_string(),
            ..ActionsFilter::default()
        };
        let path = build_runs_api_path("owner", "repo", &filter, 1, 30);
        assert!(
            path.contains("&status=failed"),
            "expected status query param, got: {path}"
        );
    }

    /// A PR filter with `pr_number`/`head_sha` set must append
    /// `&event=pull_request&head_sha=<sha>` so the API returns only runs for
    /// that PR's head commit (issue #205).
    #[test]
    fn build_runs_api_path_with_pr_filter() {
        let filter = ActionsFilter {
            pr_number: Some(42),
            head_sha: Some("abc123".to_string()),
            ..ActionsFilter::default()
        };
        let path = build_runs_api_path("owner", "repo", &filter, 1, 30);
        assert!(
            path.contains("&event=pull_request"),
            "expected event=pull_request query param, got: {path}"
        );
        assert!(
            path.contains("&head_sha=abc123"),
            "expected head_sha=abc123 query param, got: {path}"
        );
    }

    /// Without a PR filter, the path must NOT contain `event=` or `head_sha=`
    /// params (issue #205).
    #[test]
    fn build_runs_api_path_without_pr_filter() {
        let filter = ActionsFilter::default();
        let path = build_runs_api_path("owner", "repo", &filter, 1, 30);
        assert!(
            !path.contains("event="),
            "default filter must not add event= param, got: {path}"
        );
        assert!(
            !path.contains("head_sha="),
            "default filter must not add head_sha= param, got: {path}"
        );
    }
}
