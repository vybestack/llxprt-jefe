//! New PR composer overlay (issue #183).
//!
//! Four fields — head branch, base branch, title, body — rendered as one
//! overlay so the composer sits alongside the merge chooser and the property
//! editor rather than inventing a second layout idiom for the PR screen.
//!
//! The row projection is pure so what the user reads can be asserted without
//! rendering, and the branch list is windowed so a repository with hundreds of
//! branches cannot push the fields off the screen.

use iocraft::prelude::*;

use crate::state::{NewPrFormFocus, NewPrFormState};
use crate::theme::{ResolvedColors, ThemeColors};

/// How many branches are visible at once in a branch field.
pub const BRANCH_WINDOW_ROWS: usize = 5;

/// One projected line of the composer, with whether it is the focused field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewPrFormRow {
    pub text: String,
    pub focused: bool,
}

/// Props for the New PR composer overlay.
#[derive(Default, Props)]
pub struct NewPrFormProps {
    pub visible: bool,
    pub form: Option<NewPrFormState>,
    pub colors: ThemeColors,
}

/// The composer's header.
#[must_use]
pub fn new_pr_form_header() -> &'static str {
    "New Pull Request"
}

/// Project the composer into the lines it shows.
#[must_use]
pub fn new_pr_form_rows(form: &NewPrFormState) -> Vec<NewPrFormRow> {
    let mut rows = vec![
        branch_summary_row(form, NewPrFormFocus::Head),
        branch_summary_row(form, NewPrFormFocus::Base),
    ];
    rows.extend(branch_window_rows(form));
    rows.push(NewPrFormRow {
        text: format!("Title: {}", form.title_text),
        focused: form.focus == NewPrFormFocus::Title,
    });
    for (index, line) in body_lines(&form.body_text).into_iter().enumerate() {
        rows.push(NewPrFormRow {
            text: if index == 0 {
                format!("Body:  {line}")
            } else {
                format!("       {line}")
            },
            focused: form.focus == NewPrFormFocus::Body,
        });
    }
    rows
}

/// The body split into the lines it will occupy (at least one).
fn body_lines(body: &str) -> Vec<&str> {
    if body.is_empty() {
        return vec![""];
    }
    body.split('\n').collect()
}

/// The one-line summary of a branch field.
fn branch_summary_row(form: &NewPrFormState, field: NewPrFormFocus) -> NewPrFormRow {
    let (label, selected) = match field {
        NewPrFormFocus::Base => ("Base", form.base_branch()),
        _ => ("Head", form.head_branch()),
    };
    let value = if form.branches_loading {
        "loading branches..."
    } else {
        selected.unwrap_or("(no branches)")
    };
    NewPrFormRow {
        text: format!("{label}: {value}"),
        focused: form.focus == field,
    }
}

/// The visible slice of the branch list, shown only while a branch field has
/// focus so the composer stays compact while the text fields are edited.
fn branch_window_rows(form: &NewPrFormState) -> Vec<NewPrFormRow> {
    let index = match form.focus {
        NewPrFormFocus::Head => form.head_index,
        NewPrFormFocus::Base => form.base_index,
        NewPrFormFocus::Title | NewPrFormFocus::Body => return Vec::new(),
    };
    if form.branches.is_empty() {
        return Vec::new();
    }
    let start = branch_window_start(index, form.branches.len());
    form.branches
        .iter()
        .enumerate()
        .skip(start)
        .take(BRANCH_WINDOW_ROWS)
        .map(|(position, branch)| NewPrFormRow {
            text: format!("  {} {branch}", if position == index { ">" } else { " " }),
            focused: position == index,
        })
        .collect()
}

/// The first branch index the window shows, keeping the selection inside it.
#[must_use]
pub fn branch_window_start(selected: usize, total: usize) -> usize {
    if total <= BRANCH_WINDOW_ROWS {
        return 0;
    }
    let last_start = total - BRANCH_WINDOW_ROWS;
    selected
        .saturating_sub(BRANCH_WINDOW_ROWS / 2)
        .min(last_start)
}

/// The footer hint, which reports why a submit is refused when it is.
#[must_use]
pub fn new_pr_form_hint(form: &NewPrFormState) -> String {
    form.error
        .clone()
        .unwrap_or_else(|| "Tab field, Up/Down branch, Ctrl+Enter submit, Esc cancel".to_string())
}

/// New PR composer overlay.
#[component]
pub fn NewPrForm(props: &NewPrFormProps) -> impl Into<AnyElement<'static>> {
    // Check visibility before touching the form: a hidden composer is rendered
    // on every PR-screen frame, and cloning its branch list each time would be
    // an allocation for nothing.
    let Some(form) = props.form.as_ref().filter(|_| props.visible) else {
        return element! {
            Box(width: 0u32, height: 0u32) {}
        };
    };

    let rc = ResolvedColors::from_theme(Some(&props.colors));
    let rows = new_pr_form_rows(form);
    let hint = new_pr_form_hint(form);
    let hint_color = if form.error.is_some() {
        rc.bright
    } else {
        rc.dim
    };

    element! {
        Box(
            flex_direction: FlexDirection::Column,
            border_style: BorderStyle::Double,
            border_color: rc.bright,
            background_color: rc.bg,
            padding_left: 1u32,
            padding_right: 1u32,
        ) {
            Box(height: 1u32) {
                Text(content: new_pr_form_header(), weight: Weight::Bold, color: rc.bright)
            }
            #(rows.into_iter().map(|row| element! {
                Box(height: 1u32) {
                    Text(
                        content: row.text,
                        color: if row.focused { rc.bright } else { rc.fg },
                        weight: if row.focused { Weight::Bold } else { Weight::Normal },
                    )
                }
            }).collect::<Vec<_>>())
            Box(height: 1u32) {
                Text(content: super::SEPARATOR_LINE, color: rc.dim)
            }
            Box(height: 1u32) {
                Text(content: hint, color: hint_color)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BRANCH_WINDOW_ROWS, branch_window_start, new_pr_form_header, new_pr_form_hint,
        new_pr_form_rows,
    };
    use crate::state::{NewPrFormFocus, NewPrFormState};

    fn loaded_form() -> NewPrFormState {
        NewPrFormState {
            branches: vec![
                "main".to_string(),
                "feature/login".to_string(),
                "feature/logout".to_string(),
            ],
            head_index: 1,
            base_index: 0,
            ..NewPrFormState::default()
        }
    }

    fn row_texts(form: &NewPrFormState) -> Vec<String> {
        new_pr_form_rows(form).into_iter().map(|r| r.text).collect()
    }

    #[test]
    fn the_composer_shows_both_branches_the_title_and_the_body() {
        let mut form = loaded_form();
        form.focus = NewPrFormFocus::Title;
        form.title_text = "Add login".to_string();
        form.body_text = "why".to_string();

        let texts = row_texts(&form);
        assert!(
            texts.iter().any(|t| t == "Head: feature/login"),
            "{texts:?}"
        );
        assert!(texts.iter().any(|t| t == "Base: main"), "{texts:?}");
        assert!(texts.iter().any(|t| t == "Title: Add login"), "{texts:?}");
        assert!(texts.iter().any(|t| t == "Body:  why"), "{texts:?}");
    }

    #[test]
    fn a_loading_composer_says_so_instead_of_showing_a_branch() {
        let form = NewPrFormState {
            branches_loading: true,
            ..NewPrFormState::default()
        };
        let texts = row_texts(&form);
        assert!(
            texts.iter().any(|t| t.contains("loading branches")),
            "{texts:?}"
        );
    }

    #[test]
    fn a_repository_with_no_branches_says_so() {
        let texts = row_texts(&NewPrFormState::default());
        assert!(
            texts.iter().any(|t| t.contains("(no branches)")),
            "{texts:?}"
        );
    }

    #[test]
    fn the_branch_list_is_shown_only_while_a_branch_field_has_focus() {
        let form = loaded_form();
        assert!(
            row_texts(&form)
                .iter()
                .any(|t| t.contains("feature/logout")),
            "the head field lists the branches to choose from"
        );

        let mut typing = loaded_form();
        typing.focus = NewPrFormFocus::Body;
        assert!(
            !row_texts(&typing)
                .iter()
                .any(|t| t.contains("feature/logout")),
            "the list collapses while the body is edited"
        );
    }

    #[test]
    fn the_selected_branch_is_marked() {
        let rows = new_pr_form_rows(&loaded_form());
        let marked: Vec<&str> = rows
            .iter()
            .filter(|r| r.text.contains('>'))
            .map(|r| r.text.as_str())
            .collect();
        assert_eq!(marked.len(), 1, "{marked:?}");
        assert!(marked[0].contains("feature/login"), "{marked:?}");
    }

    #[test]
    fn a_multiline_body_occupies_one_row_per_line() {
        let mut form = loaded_form();
        form.focus = NewPrFormFocus::Body;
        form.body_text = "one\ntwo".to_string();

        let texts = row_texts(&form);
        assert!(texts.iter().any(|t| t == "Body:  one"), "{texts:?}");
        assert!(texts.iter().any(|t| t == "       two"), "{texts:?}");
    }

    #[test]
    fn the_window_keeps_the_selection_visible_in_a_long_branch_list() {
        let total = 40;
        for selected in 0..total {
            let start = branch_window_start(selected, total);
            assert!(selected >= start, "selection above the window");
            assert!(
                selected < start + BRANCH_WINDOW_ROWS,
                "selection below the window"
            );
            assert!(start + BRANCH_WINDOW_ROWS <= total, "window past the end");
        }
    }

    #[test]
    fn a_short_branch_list_is_never_scrolled() {
        assert_eq!(branch_window_start(2, 3), 0);
    }

    #[test]
    fn a_refused_submit_replaces_the_hint_with_the_reason() {
        let mut form = loaded_form();
        form.error = Some("Title cannot be empty.".to_string());
        assert_eq!(new_pr_form_hint(&form), "Title cannot be empty.");
    }

    #[test]
    fn the_composer_text_is_emoji_free() {
        let mut form = loaded_form();
        form.title_text = "Add login".to_string();
        let mut lines = row_texts(&form);
        lines.push(new_pr_form_header().to_string());
        lines.push(new_pr_form_hint(&form));
        for line in lines {
            assert!(line.is_ascii(), "got: {line}");
        }
    }
}
