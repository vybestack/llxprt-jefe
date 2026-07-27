//! Thin iocraft renderer for the optional PR Changes drill-down.

use iocraft::prelude::*;

use crate::domain::PrFileBlob;
use crate::pr_diff_content::{
    DiffDocument, DiffDocumentRow, DiffRowRole, build_delta_document, build_document_window,
    build_file_rows_window, build_full_document, build_threaded_document,
};
use crate::state::{PrChangesFocus, PrDiffViewMode};
use crate::theme::{ResolvedColors, ThemeColors};

/// Props for the changed-files review component.
#[derive(Default, Props)]
pub struct PrDiffProps {
    pub pr_number: u64,
    pub files: Vec<crate::domain::PrFileChange>,
    pub selected_file: Option<usize>,
    pub selected_row: Option<usize>,
    pub focus: PrChangesFocus,
    pub view_mode: PrDiffViewMode,
    pub loading: bool,
    pub error: Option<String>,
    pub truncated: bool,
    pub full_blob: Option<PrFileBlob>,
    pub blob_loading: bool,
    pub blob_error: Option<String>,
    pub review_threads: Vec<crate::domain::PrReviewThread>,
    pub inline_state: crate::state::InlineState,
    pub viewport_rows: usize,
    pub colors: ThemeColors,
}

/// Render a changed-files list over the selected file's delta document.
#[component]
pub fn PrDiff(props: &PrDiffProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(Some(&props.colors));
    let mode = match props.view_mode {
        PrDiffViewMode::DeltasOnly => "Deltas Only",
        PrDiffViewMode::FullFile => "Full File",
    };
    let file_rows = build_file_rows_window(&props.files, props.selected_file, 8);
    let document = props
        .selected_file
        .and_then(|index| props.files.get(index))
        .map(|file| {
            let content = if props.view_mode == PrDiffViewMode::DeltasOnly {
                build_delta_document(file)
            } else if let Some(blob) = props.full_blob.as_ref() {
                build_full_document(file, blob)
            } else if props.blob_loading {
                notice_document("Loading full file…")
            } else {
                notice_document(
                    props
                        .blob_error
                        .as_deref()
                        .unwrap_or("Full file unavailable"),
                )
            };
            build_threaded_document(file, content, &props.review_threads)
        });
    let status = props.error.clone().unwrap_or_else(|| {
        if props.loading {
            "Loading changed files…".to_string()
        } else if props.files.is_empty() {
            "No changed files".to_string()
        } else if props.truncated {
            "GitHub limits this view to the first 3,000 files".to_string()
        } else {
            String::new()
        }
    });

    element! {
        Box(flex_direction: FlexDirection::Column, width: 100pct, height: 100pct) {
            Box(height: 1u32, padding_left: 1u32) {
                Text(
                    content: format!("Changes — PR {} — {mode}", props.pr_number),
                    color: rc.bright,
                    weight: Weight::Bold,
                )
            }
            Box(
                height: 10u32,
                border_style: BorderStyle::Single,
                border_color: if props.focus == PrChangesFocus::FileList { rc.border_focused } else { rc.border },
                flex_direction: FlexDirection::Column,
            ) {
                #(file_rows.iter().map(|(index, row)| {
                    let selected = props.selected_file == Some(*index);
                    element! {
                        Box(height: 1u32, padding_left: 1u32) {
                            Text(
                                content: format!("{}{}", if selected { "> " } else { "  " }, row.text),
                                color: row_color(row.role, rc),
                                weight: if selected { Weight::Bold } else { Weight::Normal },
                            )
                        }
                    }
                }).collect::<Vec<_>>())
                #(if status.is_empty() { vec![] } else { vec![element! {
                    Box(height: 1u32, padding_left: 1u32) { Text(content: status.clone(), color: rc.dim) }
                }] })
            }
            Box(
                flex_grow: 1.0_f32,
                border_style: BorderStyle::Single,
                border_color: if props.focus == PrChangesFocus::Content { rc.border_focused } else { rc.border },
                flex_direction: FlexDirection::Column,
            ) {
                #(document.map_or_else(Vec::new, |document| build_document_window(&document, props.selected_row, props.viewport_rows).into_iter().map(|(index, row)| {
                    let selected = props.selected_row == Some(index);
                    element! {
                        Box(height: 1u32, padding_left: 1u32) {
                            Text(
                                content: format!("{}{}", if selected { "> " } else { "  " }, row.text),
                                color: row_color(row.role, rc),
                                weight: if selected { Weight::Bold } else { Weight::Normal },
                            )
                        }
                    }
                }).collect::<Vec<_>>()))
            }
            #(composer_elements(&props.inline_state, rc))
            Text(
                content: changes_hints(props.view_mode),
                color: rc.fg,
                weight: Weight::Bold,
            )
        }

    }
}

fn notice_document(message: &str) -> DiffDocument {
    DiffDocument {
        rows: vec![DiffDocumentRow {
            text: message.to_string(),
            role: DiffRowRole::Notice,
            anchor: None,
            thread_index: None,
        }],
    }
}

fn changes_hints(mode: PrDiffViewMode) -> &'static str {
    match mode {
        PrDiffViewMode::DeltasOnly => {
            "^/v select | Enter/Tab content | BackTab files | v full file | c comment | r reply | R resolve | Esc back"
        }
        PrDiffViewMode::FullFile => {
            "^/v select | Enter/Tab content | BackTab files | v deltas | c comment | r reply | R resolve | Esc back"
        }
    }
}
fn composer_elements(
    inline_state: &crate::state::InlineState,
    colors: ResolvedColors,
) -> Vec<AnyElement<'static>> {
    let crate::state::InlineState::Composer { text, target, .. } = inline_state else {
        return Vec::new();
    };
    vec![
        element! {
            Box(
                border_style: BorderStyle::Single,
                border_color: colors.border_focused,
                flex_direction: FlexDirection::Column,
                padding_left: 1u32,
            ) {
                Text(content: composer_label(target), color: colors.bright)
                Text(content: text.clone(), color: colors.fg)
                Text(content: "Alt+Enter submit | Esc cancel", color: colors.dim)
            }
        }
        .into(),
    ]
}

fn composer_label(target: &crate::state::ComposerTarget) -> String {
    match target {
        crate::state::ComposerTarget::NewReviewThread { target } => {
            format!("Review comment — {}:{}", target.path, target.line)
        }
        crate::state::ComposerTarget::ReplyToReviewThread { .. } => {
            "Review thread reply".to_string()
        }
        _ => "Pull request comment".to_string(),
    }
}

fn row_color(role: DiffRowRole, colors: ResolvedColors) -> Color {
    match role {
        DiffRowRole::Added => colors.bright,
        DiffRowRole::Removed => colors.error,
        DiffRowRole::Hunk | DiffRowRole::Notice => colors.dim,
        DiffRowRole::Normal => colors.fg,
    }
}
