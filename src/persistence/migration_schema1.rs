//! Private schema-1 DTOs used only by the one-way migration reader.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

fn default_code_puppy_yolo() -> bool {
    true
}

fn default_llxprt_mode_flags() -> Vec<String> {
    vec!["--yolo".to_owned()]
}

fn default_pass_continue() -> bool {
    true
}

fn default_sandbox_engine() -> String {
    "podman".to_owned()
}

#[derive(Debug, Deserialize)]
pub(super) struct Schema1State {
    pub schema_version: u64,
    #[serde(default)]
    pub repositories: Vec<Schema1Repository>,
    #[serde(default)]
    pub agents: Vec<Schema1Agent>,
    #[serde(default)]
    pub selected_repository_index: Option<usize>,
    #[serde(default)]
    pub selected_agent_index: Option<usize>,
    #[serde(default)]
    pub hide_idle_repositories: bool,
    #[serde(default)]
    pub last_selected_agent_by_repo: Vec<(String, String)>,
    #[serde(default)]
    pub pane_focus: String,
    #[serde(default)]
    pub terminal_focused: bool,
    #[serde(default)]
    pub user_preferences: Schema1Preferences,
    #[serde(flatten)]
    pub unknown: BTreeMap<String, Value>,
}

#[derive(Debug, Deserialize)]
pub(super) struct Schema1Repository {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub slug: String,
    pub base_dir: PathBuf,
    #[serde(default)]
    pub default_profile: String,
    #[serde(default)]
    pub default_code_puppy_model: String,
    #[serde(default)]
    pub default_code_puppy_version: String,
    #[serde(default)]
    pub github_repo: String,
    #[serde(default)]
    pub github_issue_pr_repo: String,
    #[serde(default)]
    pub remote: Schema1Remote,
    #[serde(default)]
    pub issue_base_prompt: String,
    #[serde(default)]
    pub default_agent_kind: Option<String>,
    #[serde(default)]
    pub transient_agent_dir: PathBuf,
    #[serde(default = "default_code_puppy_yolo")]
    pub default_code_puppy_yolo: bool,
    #[serde(default = "default_llxprt_mode_flags")]
    pub default_llxprt_mode_flags: Vec<String>,
    #[serde(default)]
    pub transient_max_concurrent: u32,
    #[serde(default)]
    pub default_llxprt_version: Option<String>,
    #[serde(default)]
    pub agent_ids: Vec<String>,
    #[serde(flatten)]
    pub unknown: BTreeMap<String, Value>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub(super) struct Schema1Remote {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub login_user: String,
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub identity_file: PathBuf,
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(default)]
    pub run_as_user: String,
    #[serde(default)]
    pub setup_env_default: bool,
    #[serde(flatten)]
    pub unknown: BTreeMap<String, Value>,
}

#[derive(Debug, Deserialize)]
pub(super) struct Schema1Agent {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub display_id: String,
    pub repository_id: String,
    #[serde(default)]
    pub shortcut_slot: Option<u8>,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub work_dir: PathBuf,
    #[serde(default)]
    pub profile: String,
    #[serde(default)]
    pub code_puppy_model: String,
    #[serde(default)]
    pub code_puppy_version: String,
    #[serde(default)]
    pub code_puppy_yolo: Option<bool>,
    #[serde(default)]
    pub code_puppy_quick_resume: bool,
    #[serde(default)]
    pub mode_flags: Vec<String>,
    #[serde(default)]
    pub llxprt_debug: String,
    #[serde(default = "default_pass_continue")]
    pub pass_continue: bool,
    #[serde(default)]
    pub sandbox_enabled: bool,
    #[serde(default = "default_sandbox_engine")]
    pub sandbox_engine: String,
    #[serde(default)]
    pub sandbox_flags: String,
    #[serde(default)]
    pub agent_kind: Option<String>,
    #[serde(default)]
    pub llxprt_version: Option<String>,
    #[serde(default)]
    pub status: Option<Value>,
    #[serde(default)]
    pub runtime_binding: Option<Schema1RuntimeBinding>,
    #[serde(default)]
    pub origin: Option<String>,
    #[serde(flatten)]
    pub unknown: BTreeMap<String, Value>,
}

#[derive(Debug, Deserialize)]
pub(super) struct Schema1RuntimeBinding {
    pub session_name: String,
    #[serde(default)]
    pub lifecycle_generation: u64,
    #[serde(flatten)]
    pub evidence: BTreeMap<String, Value>,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct Schema1Preferences {
    #[serde(default)]
    pub by_repo: Vec<(String, Schema1RepoPreferences)>,
    #[serde(flatten)]
    pub unknown: BTreeMap<String, Value>,
}

#[derive(Debug, Deserialize, Serialize)]
pub(super) struct Schema1RepoPreferences {
    #[serde(default = "default_issue_filter")]
    pub issue_filter: Schema1IssueFilter,
    #[serde(default = "default_pr_filter")]
    pub pr_filter: Schema1PrFilter,
    #[serde(default)]
    pub issue_search_query: String,
    #[serde(default)]
    pub pr_search_query: String,
    #[serde(default)]
    pub issue_filter_field_index: usize,
    #[serde(default)]
    pub pr_filter_field_index: usize,
    #[serde(default)]
    pub last_merge_method: Option<String>,
    #[serde(flatten)]
    pub unknown: BTreeMap<String, Value>,
}

fn default_issue_filter() -> Schema1IssueFilter {
    Schema1IssueFilter {
        state: Some("Open".to_owned()),
        ..Schema1IssueFilter::default()
    }
}

fn default_pr_filter() -> Schema1PrFilter {
    Schema1PrFilter {
        state: Some("Open".to_owned()),
        ..Schema1PrFilter::default()
    }
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub(super) struct Schema1IssueFilter {
    #[serde(default)]
    pub query_text: String,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub assignee: String,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub issue_type: String,
    #[serde(default)]
    pub milestone: String,
    #[serde(default)]
    pub module: String,
    #[serde(default)]
    pub mentioned: String,
    #[serde(default)]
    pub updated_before: String,
    #[serde(default)]
    pub updated_after: String,
    #[serde(flatten)]
    pub unknown: BTreeMap<String, Value>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub(super) struct Schema1PrFilter {
    #[serde(default)]
    pub query_text: String,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub assignee: String,
    #[serde(default)]
    pub reviewer: String,
    #[serde(default)]
    pub is_draft: Option<bool>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub review_decision: Option<String>,
    #[serde(default)]
    pub checks_status: Option<String>,
    #[serde(flatten)]
    pub unknown: BTreeMap<String, Value>,
}
