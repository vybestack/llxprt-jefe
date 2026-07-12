//! Issues Mode domain entities (extracted from `mod.rs` to keep that file
//! under the source-file-size limit).
//!
//! @plan PLAN-20260329-ISSUES-MODE.P03

use serde::{Deserialize, Serialize};

/// @requirement REQ-ISS-006
/// @pseudocode component-001 lines 83-96
/// Issue state for list display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IssueState {
    Open,
    Closed,
}

/// @plan PLAN-20260329-ISSUES-MODE.P03
/// @requirement REQ-ISS-006
/// Issue list representation.
#[derive(Debug, Clone)]
pub struct Issue {
    pub number: u64,
    /// GraphQL node id (e.g. `I_kwDO...`); required for `deleteIssue`.
    pub node_id: String,
    pub title: String,
    pub state: IssueState,
    pub author_login: String,
    pub updated_at: String,
    pub assignee_summary: String,
    pub labels_summary: String,
    pub assignees: Vec<String>,
    pub labels: Vec<String>,
    pub issue_type: String,
    pub milestone: String,
    pub module: String,
    pub comment_count: u64,
    /// Optional lightweight preview body; list/search fetches may leave this empty
    /// so full body content is loaded through `IssueDetail` instead.
    pub body: String,
}

/// @plan PLAN-20260329-ISSUES-MODE.P03
/// @requirement REQ-ISS-009
/// Full issue detail with comments.
#[derive(Debug, Clone)]
pub struct IssueDetail {
    pub repo_owner_name: String,
    pub number: u64,
    /// GraphQL node id (e.g. `I_kwDO...`); required for `deleteIssue`.
    pub node_id: String,
    pub title: String,
    pub state: IssueState,
    pub author_login: String,
    pub created_at: String,
    pub updated_at: String,
    pub labels: Vec<String>,
    pub assignees: Vec<String>,
    pub milestone: Option<String>,
    pub body: String,
    pub external_url: String,
    pub comments: Vec<IssueComment>,
    pub has_more_comments: bool,
    pub comments_cursor: Option<String>,
}

/// @plan PLAN-20260329-ISSUES-MODE.P03
/// @requirement REQ-ISS-009
/// Single issue comment.
#[derive(Debug, Clone)]
pub struct IssueComment {
    pub comment_id: u64,
    pub author_login: String,
    pub created_at: String,
    pub edited_at: Option<String>,
    pub body: String,
}

/// @plan PLAN-20260329-ISSUES-MODE.P03
/// @requirement REQ-ISS-008
/// Filter state options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum IssueFilterState {
    #[default]
    Open,
    Closed,
    All,
}

pub const FILTER_CHOICE_ANY: &str = "any";
pub const FILTER_CHOICE_NONE: &str = "none";

/// @plan PLAN-20260329-ISSUES-MODE.P03
/// @requirement REQ-ISS-008
/// Issue list filter criteria.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssueFilter {
    #[serde(default)]
    pub query_text: String,
    #[serde(default)]
    pub state: Option<IssueFilterState>,
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
}

impl IssueFilter {
    /// @plan PLAN-20260329-ISSUES-MODE.P03
    /// @requirement REQ-ISS-008
    /// @pseudocode component-011 lines 1-9
    #[must_use]
    pub fn has_active_non_default_filters(&self) -> bool {
        matches!(
            self.state,
            Some(IssueFilterState::Closed | IssueFilterState::All)
        ) || !self.query_text.trim().is_empty()
            || sentinel_filter_is_active(&self.author)
            || sentinel_filter_is_active(&self.assignee)
            || !self.labels.is_empty()
            || sentinel_filter_is_active(&self.issue_type)
            || sentinel_filter_is_active(&self.milestone)
            || sentinel_filter_is_active(&self.module)
            || sentinel_filter_is_active(&self.mentioned)
            || sentinel_filter_is_active(&self.updated_before)
            || sentinel_filter_is_active(&self.updated_after)
    }
}

fn sentinel_filter_is_active(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty() && !trimmed.eq_ignore_ascii_case(FILTER_CHOICE_ANY)
}
