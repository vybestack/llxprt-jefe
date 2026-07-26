//! New Issue inline form-field types (issue #407).
//!
//! Extracted from `types.rs` (like `form_types.rs`/`issues_types.rs`) to keep
//! `types.rs` under the source-file-size hard limit. These are the draft
//! state carried by [`crate::state::IssuesState::new_issue_form`] and the
//! focus/cursor helpers for the inline new-issue composer.

/// A selectable issue type in the New Issue form (issue #407).
///
/// Carries the GraphQL node `id` (used for the `updateIssue` mutation) and
/// the display `name`. Replaces the unlabeled `(String, String)` tuple so
/// call sites read as `t.id` / `t.name` instead of `.0` / `.1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IssueType {
    /// Opaque GraphQL node id submitted to `updateIssue.issueTypeId`.
    pub id: String,
    /// Human-readable type name shown in the picker.
    pub name: String,
}

impl IssueType {
    /// Construct an issue type from its GraphQL id and display name.
    #[must_use]
    pub fn new(id: String, name: String) -> Self {
        Self { id, name }
    }
}

/// Built-in templates synthesized client-side (issue #407). Repo-defined
/// issue templates are listed separately via the `issueTemplates` GraphQL
/// connection; Slice A ships only the built-in presets.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum NewIssueTemplate {
    #[default]
    Blank,
    Bug,
    Feature,
    Task,
}

impl NewIssueTemplate {
    /// Cycle to the next built-in template (issue #407 A2).
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Blank => Self::Bug,
            Self::Bug => Self::Feature,
            Self::Feature => Self::Task,
            Self::Task => Self::Blank,
        }
    }

    /// Short display label for the template.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Blank => "Blank",
            Self::Bug => "Bug",
            Self::Feature => "Feature",
            Self::Task => "Task",
        }
    }

    /// The body scaffold to prefill when this template is selected. Blank
    /// returns an empty body (the user types from scratch). Returns a static
    /// slice to avoid allocating on every call (issue #407).
    #[must_use]
    pub fn body_scaffold(self) -> &'static str {
        match self {
            Self::Blank => "",
            Self::Bug => {
                "\
## What happened?

## Steps to reproduce
1.
2.

## Expected
"
            }
            Self::Feature => {
                "\
## Motivation

## Proposal

## Non-goals
"
            }
            Self::Task => {
                "\
## Goal

## Acceptance

## Non-goals
"
            }
        }
    }
}

/// Which field is focused in the New Issue form (issue #407).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum NewIssueFormFocus {
    #[default]
    Template,
    Type,
    Title,
    Body,
    Labels,
    Milestone,
    Project,
    Assignees,
}

impl NewIssueFormFocus {
    /// Move to the next focusable field (issue #407). Cycle at the end.
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Template => Self::Type,
            Self::Type => Self::Title,
            Self::Title => Self::Body,
            Self::Body => Self::Labels,
            Self::Labels => Self::Milestone,
            Self::Milestone => Self::Project,
            Self::Project => Self::Assignees,
            Self::Assignees => Self::Template,
        }
    }

    /// Move to the previous focusable field (issue #407). Cycle at the start.
    #[must_use]
    pub fn prev(self) -> Self {
        match self {
            Self::Template => Self::Assignees,
            Self::Type => Self::Template,
            Self::Title => Self::Type,
            Self::Body => Self::Title,
            Self::Labels => Self::Body,
            Self::Milestone => Self::Labels,
            Self::Project => Self::Milestone,
            Self::Assignees => Self::Project,
        }
    }
}

/// Draft state for the inline New Issue form (issue #407).
///
/// Pure reducer state: no I/O. The `app_input` layer reads this on submit
/// and drives the `create_issue` + post-create property-edit pipeline.
/// Sticky milestone/project defaults are restored from `RepoPreferences`
/// when the form opens and remembered back on a successful submit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewIssueFormState {
    /// Currently-selected built-in template (Blank/Bug/Feature/Task).
    pub template: NewIssueTemplate,
    /// Selected issue type name (`None` = no type). Resolved against the
    /// repo's available issue types via the `issueTypes` GraphQL query.
    pub type_name: Option<String>,
    /// Opaque node id for the selected issue type, resolved when the type
    /// picker confirms (issue #407). `None` when no type is selected.
    pub type_id: Option<String>,
    /// Available issue types for the current repo. Populated async when the
    /// form opens; empty while loading.
    pub available_types: Vec<IssueType>,
    /// Title draft (single line).
    pub title_text: String,
    /// Title cursor (char offset).
    pub title_cursor: usize,
    /// Body draft (multiline).
    pub body_text: String,
    /// Body cursor (char offset).
    pub body_cursor: usize,
    /// Selected labels (multi). Applied via `--add-label` after create.
    pub labels: Vec<String>,
    /// Available labels for the repo (populated async).
    pub available_labels: Vec<String>,
    /// Selected milestone (`None` = blankable). Sticky across opens.
    pub milestone: Option<String>,
    /// Available milestones for the repo (populated async).
    pub available_milestones: Vec<String>,
    /// Selected Projects V2 node ids (multi). Sticky across opens.
    pub project_ids: Vec<String>,
    /// Selected assignee logins (multi). Applied via `--add-assignee` after
    /// create.
    pub assignees: Vec<String>,
    /// Available assignee logins for the repo (populated async).
    pub available_assignees: Vec<String>,
    /// Which field is focused.
    pub focus: NewIssueFormFocus,
    /// Footer error (e.g. empty-title, options-load failure). Blankable.
    pub error: Option<String>,
    /// Whether the async options load (labels/milestones/types/assignees)
    /// is still in flight. Submit is blocked while true.
    pub options_loading: bool,
}
