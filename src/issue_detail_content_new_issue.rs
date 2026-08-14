//! The New Issue document projection (issue #693).
//!
//! The pane used to render a fixed four-line prompt plus one embedded `TextBox`
//! that only ever showed the *focused* field, so a typed subject vanished the
//! moment focus advanced to the body. Summarising every field here keeps what
//! the user typed on screen.

use super::{ContentBuilder, DetailContent};
use crate::state::{NewIssueFormFocus, NewIssueFormState};

/// Placeholder for an optional New Issue field the user has not set yet.
const NEW_ISSUE_UNSET: &str = "(none)";

/// Build a full-screen content block for creating a new issue.
///
/// Every field of the form is summarised here, so text the user already typed
/// stays on screen once focus moves on (issue #693): the embedded `TextBox`
/// only ever shows the *focused* field, which made a typed subject appear to
/// vanish the moment Enter advanced focus to the body.
///
/// The editable text itself is still rendered by that wrapping `TextBox`
/// (issue #212); these rows are a read-only summary and own no cursor.
#[must_use]
pub fn build_new_issue_content(form: Option<&NewIssueFormState>) -> DetailContent {
    let mut builder = ContentBuilder::new();

    builder.lines.push("New Issue".to_string());
    if let Some(form) = form {
        push_new_issue_form_rows(&mut builder.lines, form);
        if let Some(error) = &form.error {
            builder.lines.push(error.clone());
        }
    }
    builder.lines.push(String::new());

    builder.lines.push("[Composer input]".to_string());
    builder.finish()
}

/// One row per New Issue field, in Tab order, with the focused row marked.
fn push_new_issue_form_rows(rows: &mut Vec<String>, form: &NewIssueFormState) {
    push_new_issue_field(
        rows,
        form,
        NewIssueFormFocus::Template,
        "Template",
        form.template.label(),
    );
    push_new_issue_field(
        rows,
        form,
        NewIssueFormFocus::Type,
        "Type",
        &optional_new_issue_value(form.type_name.as_deref()),
    );
    push_new_issue_field(
        rows,
        form,
        NewIssueFormFocus::Title,
        "Title",
        &form.title_text,
    );
    push_new_issue_body_rows(rows, form);
    push_new_issue_field(
        rows,
        form,
        NewIssueFormFocus::Labels,
        "Labels",
        &joined_new_issue_values(&form.labels),
    );
    push_new_issue_field(
        rows,
        form,
        NewIssueFormFocus::Milestone,
        "Milestone",
        &optional_new_issue_value(form.milestone.as_deref()),
    );
    push_new_issue_field(
        rows,
        form,
        NewIssueFormFocus::Project,
        "Project",
        &joined_new_issue_values(&form.project_ids),
    );
    push_new_issue_field(
        rows,
        form,
        NewIssueFormFocus::Assignees,
        "Assignees",
        &joined_new_issue_values(&form.assignees),
    );
}

/// Push one labelled single-line field row.
fn push_new_issue_field(
    rows: &mut Vec<String>,
    form: &NewIssueFormState,
    focus: NewIssueFormFocus,
    label: &str,
    value: &str,
) {
    let marker = new_issue_focus_marker(form, focus);
    rows.push(format!("{marker}{label}: {value}").trim_end().to_string());
}

/// Push the body field, one row per logical line, continuations aligned under
/// the label so a multi-line draft still reads as one field.
fn push_new_issue_body_rows(rows: &mut Vec<String>, form: &NewIssueFormState) {
    let marker = new_issue_focus_marker(form, NewIssueFormFocus::Body);
    let label = "Body: ";
    let indent = " ".repeat(marker.len() + label.len());
    let mut lines = form.body_text.split('\n');
    let first = lines.next().unwrap_or_default();
    rows.push(format!("{marker}{label}{first}").trim_end().to_string());
    for line in lines {
        rows.push(format!("{indent}{line}").trim_end().to_string());
    }
}

/// `"> "` for the focused field, `"  "` otherwise — the marker convention the
/// parent module already uses for issue-detail subfocus.
fn new_issue_focus_marker(form: &NewIssueFormState, focus: NewIssueFormFocus) -> &'static str {
    if form.focus == focus { "> " } else { "  " }
}

/// Render an optional field value, or the unset placeholder.
fn optional_new_issue_value(value: Option<&str>) -> String {
    value
        .filter(|value| !value.is_empty())
        .unwrap_or(NEW_ISSUE_UNSET)
        .to_string()
}

/// Render a multi-value field, or the unset placeholder when it is empty.
fn joined_new_issue_values(values: &[String]) -> String {
    if values.is_empty() {
        NEW_ISSUE_UNSET.to_string()
    } else {
        values.join(", ")
    }
}

#[cfg(test)]
#[path = "issue_detail_content_new_issue_tests.rs"]
mod tests;
