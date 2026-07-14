//! GitHub Actions domain types: workflows, runs, jobs, steps, and filters.
//!
//! Extracted from `mod.rs` to keep that file under the source-file-size limit.
//! All types are plain data records with no behavior beyond `as_str` accessors.

use serde::{Deserialize, Serialize};

/// GitHub Actions workflow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workflow {
    pub id: u64,
    pub name: String,
    pub path: String,
    pub state: String,
}

/// GitHub Actions workflow run status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum WorkflowRunStatus {
    Completed,
    InProgress,
    Queued,
    Requested,
    Waiting,
    Pending,
    #[default]
    Unknown,
}

impl WorkflowRunStatus {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::InProgress => "in_progress",
            Self::Queued => "queued",
            Self::Requested => "requested",
            Self::Waiting => "waiting",
            Self::Pending => "pending",
            Self::Unknown => "unknown",
        }
    }
}

/// GitHub Actions workflow run conclusion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkflowRunConclusion {
    Success,
    Failure,
    Cancelled,
    Skipped,
    TimedOut,
    ActionRequired,
    Stale,
    Neutral,
    StartupFailure,
    Unknown,
}

impl WorkflowRunConclusion {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failure => "failure",
            Self::Cancelled => "cancelled",
            Self::Skipped => "skipped",
            Self::TimedOut => "timed_out",
            Self::ActionRequired => "action_required",
            Self::Stale => "stale",
            Self::Neutral => "neutral",
            Self::StartupFailure => "startup_failure",
            Self::Unknown => "unknown",
        }
    }
}

/// GitHub Actions workflow run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowRun {
    pub id: u64,
    pub name: String,
    pub head_branch: String,
    pub head_sha: String,
    pub run_number: u32,
    pub event: String,
    pub status: WorkflowRunStatus,
    pub conclusion: Option<WorkflowRunConclusion>,
    pub workflow_name: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A job in a workflow run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowRunJob {
    pub id: u64,
    pub name: String,
    pub status: WorkflowRunStatus,
    pub conclusion: Option<WorkflowRunConclusion>,
    pub steps: Vec<WorkflowRunStep>,
}

/// A step in a workflow job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowRunStep {
    pub name: String,
    pub status: WorkflowRunStatus,
    pub conclusion: Option<WorkflowRunConclusion>,
    pub number: u32,
}

/// Detailed workflow run containing run info and jobs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowRunDetail {
    pub run: WorkflowRun,
    pub jobs: Vec<WorkflowRunJob>,
}

/// GitHub Actions run list filter criteria.
///
/// `workflow` holds the human-readable workflow display name (e.g. "CI") for
/// the UI; `workflow_path` holds the workflow file path (e.g.
/// ".github/workflows/ci.yml") used for the GitHub API call. The API rejects
/// display names with HTTP 404, so the path is the authoritative selector.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionsFilter {
    /// Display name of the selected workflow (for UI rendering).
    pub workflow: String,
    /// Workflow file path for the API call (empty = "all" / no filter).
    pub workflow_path: String,
    pub status: String,
    /// Committed search query for client-side filtering of workflow runs.
    pub search: String,
    /// PR number filter (None = no PR filter).
    pub pr_number: Option<u64>,
    /// Resolved head SHA for the PR filter (used for the GitHub API head_sha= param).
    pub head_sha: Option<String>,
}
