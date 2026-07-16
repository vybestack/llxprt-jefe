//! Issues Mode domain entities (extracted from `mod.rs` to keep that file
//! under the source-file-size limit).
//!
//! @plan PLAN-20260329-ISSUES-MODE.P03

use serde::{Deserialize, Serialize};

use super::{CommentDetailIdentity, PaginatedList};

/// @requirement REQ-ISS-006
/// @pseudocode component-001 lines 83-96
/// Issue state for list display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IssueState {
    Open,
    Closed,
}

/// The GitHub-native reason an issue was closed (issue #204 read side).
///
/// Mirrors the GraphQL `IssueStateReason` values that are meaningful for a
/// closed issue. `REOPENED` is out of scope (no reopen-with-reason) and is
/// therefore not represented. Parsed from the GraphQL `stateReason` field and
/// the REST `state_reason` field; unknown/missing values yield `None` on the
/// domain types so legacy fixtures and partial data degrade gracefully.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueStateReason {
    Completed,
    NotPlanned,
    Duplicate,
}

impl IssueStateReason {
    /// Parse a GraphQL/REST state-reason string into a domain value.
    ///
    /// Accepts both the GraphQL enum spelling (`COMPLETED`, `NOT_PLANNED`,
    /// `DUPLICATE`) and the REST spelling (`completed`, `not_planned`,
    /// `duplicate`). Returns `None` for `REOPENED`, unknown, or missing values
    /// so callers can store an `Option` and let legacy data degrade to "no
    /// reason".
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "COMPLETED" | "completed" => Some(Self::Completed),
            "NOT_PLANNED" | "not_planned" => Some(Self::NotPlanned),
            "DUPLICATE" | "duplicate" => Some(Self::Duplicate),
            _ => None,
        }
    }

    /// User-facing, emoji-free label for display.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::NotPlanned => "not planned",
            Self::Duplicate => "duplicate",
        }
    }
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
    /// The GitHub-native close reason, if any (issue #204). `None` for open
    /// issues or closed issues whose reason is unknown/missing.
    pub state_reason: Option<IssueStateReason>,
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
    pub comments: PaginatedList<IssueComment, CommentDetailIdentity>,
    /// The issue's type name (GitHub issue types), if any (issue #175).
    pub issue_type_name: Option<String>,
    /// The GitHub-native close reason, if any (issue #204). `None` for open
    /// issues or closed issues whose reason is unknown/missing.
    pub state_reason: Option<IssueStateReason>,
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

/// Close reason for an issue (issue #188).
///
/// Mirrors GitHub's close-reason UX. `gh issue close --reason` supports only
/// `completed` and `not planned`; `Duplicate` and `Invalid` both map to
/// `not planned` at the API layer, with `Duplicate` additionally running the
/// `markIssueAsDuplicate` GraphQL mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseReason {
    Completed,
    NotPlanned,
    Duplicate,
    Invalid,
}

/// All close reasons in canonical display order (mirrors `MERGE_METHODS`).
pub const CLOSE_REASONS: [CloseReason; 4] = [
    CloseReason::Completed,
    CloseReason::NotPlanned,
    CloseReason::Duplicate,
    CloseReason::Invalid,
];

impl CloseReason {
    /// User-facing display label (emoji-free, plain text).
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Completed => "Completed",
            Self::NotPlanned => "Not planned",
            Self::Duplicate => "Duplicate",
            Self::Invalid => "Invalid",
        }
    }

    /// The `--reason` value passed to `gh issue close`.
    ///
    /// `gh` supports only `completed` and `not planned`. `Duplicate` and
    /// `Invalid` both map to `not planned` — `Duplicate` additionally runs the
    /// `markIssueAsDuplicate` mutation; `Invalid` has no dedicated API reason.
    #[must_use]
    pub const fn gh_reason_flag(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::NotPlanned | Self::Duplicate | Self::Invalid => "not planned",
        }
    }
}

#[cfg(test)]
mod close_reason_tests {
    use super::*;

    #[test]
    fn completed_label_and_flag() {
        assert_eq!(CloseReason::Completed.label(), "Completed");
        assert_eq!(CloseReason::Completed.gh_reason_flag(), "completed");
    }

    #[test]
    fn not_planned_label_and_flag() {
        assert_eq!(CloseReason::NotPlanned.label(), "Not planned");
        assert_eq!(CloseReason::NotPlanned.gh_reason_flag(), "not planned");
    }

    #[test]
    fn duplicate_label_and_flag() {
        assert_eq!(CloseReason::Duplicate.label(), "Duplicate");
        assert_eq!(CloseReason::Duplicate.gh_reason_flag(), "not planned");
    }

    #[test]
    fn invalid_label_and_flag() {
        assert_eq!(CloseReason::Invalid.label(), "Invalid");
        assert_eq!(CloseReason::Invalid.gh_reason_flag(), "not planned");
    }

    #[test]
    fn close_reasons_has_four_variants_in_order() {
        assert_eq!(CLOSE_REASONS.len(), 4);
        assert_eq!(CLOSE_REASONS[0], CloseReason::Completed);
        assert_eq!(CLOSE_REASONS[1], CloseReason::NotPlanned);
        assert_eq!(CLOSE_REASONS[2], CloseReason::Duplicate);
        assert_eq!(CLOSE_REASONS[3], CloseReason::Invalid);
    }
}

#[cfg(test)]
mod state_reason_tests {
    use super::IssueStateReason;

    #[test]
    fn parse_graphql_spellings() {
        assert_eq!(
            IssueStateReason::parse("COMPLETED"),
            Some(IssueStateReason::Completed)
        );
        assert_eq!(
            IssueStateReason::parse("NOT_PLANNED"),
            Some(IssueStateReason::NotPlanned)
        );
        assert_eq!(
            IssueStateReason::parse("DUPLICATE"),
            Some(IssueStateReason::Duplicate)
        );
    }

    #[test]
    fn parse_rest_spellings() {
        assert_eq!(
            IssueStateReason::parse("completed"),
            Some(IssueStateReason::Completed)
        );
        assert_eq!(
            IssueStateReason::parse("not_planned"),
            Some(IssueStateReason::NotPlanned)
        );
        assert_eq!(
            IssueStateReason::parse("duplicate"),
            Some(IssueStateReason::Duplicate)
        );
    }

    #[test]
    fn parse_reopened_returns_none() {
        assert_eq!(IssueStateReason::parse("REOPENED"), None);
        assert_eq!(IssueStateReason::parse("reopened"), None);
    }

    #[test]
    fn parse_unknown_returns_none() {
        assert_eq!(IssueStateReason::parse("WHATEVER"), None);
        assert_eq!(IssueStateReason::parse(""), None);
    }

    #[test]
    fn parse_trims_whitespace() {
        assert_eq!(
            IssueStateReason::parse("  COMPLETED  "),
            Some(IssueStateReason::Completed)
        );
    }

    #[test]
    fn labels_are_human_readable() {
        assert_eq!(IssueStateReason::Completed.label(), "completed");
        assert_eq!(IssueStateReason::NotPlanned.label(), "not planned");
        assert_eq!(IssueStateReason::Duplicate.label(), "duplicate");
    }
}
