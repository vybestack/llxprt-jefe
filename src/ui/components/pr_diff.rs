//! Thin iocraft renderer for the optional PR Changes drill-down.

use iocraft::prelude::*;

use crate::domain::PrFileBlob;
use crate::pr_diff_content::{
    DiffDocument, DiffDocumentRow, DiffRowRole, build_delta_document, build_document_window,
    build_file_rows_window, build_full_document, build_threaded_document,
};
use crate::state::{PrChangesFocus, PrDiffViewMode};
use crate::theme::{ResolvedColors, ThemeColors};

use super::detail_pane::composer_from_inline_state;
use super::text_box::TextBox;

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
    let selected_file = props.selected_file.and_then(|index| props.files.get(index));
    let document = selected_file.map(|file| {
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

    let composer = composer_from_inline_state(&props.inline_state);
    let composer_rows = crate::layout::DETAIL_COMPOSER_VIEWPORT_ROWS;

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
            #(composer_box(composer.as_ref(), &props.inline_state, composer_rows, rc))
            Text(
                content: changes_hints(ChangesHintCtx {
                    focus: props.focus,
                    mode: props.view_mode,
                    composing: composer.is_some(),
                    no_selected_file: selected_file.is_none(),
                    blob: if props.blob_loading {
                        BlobHintState::Loading
                    } else if props.blob_error.is_some() {
                        BlobHintState::Failed
                    } else {
                        BlobHintState::Idle
                    },
                }),
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

/// Contextual blob-read state for the Changes footer hint.
enum BlobHintState {
    Idle,
    Loading,
    Failed,
}

/// Context for building the Changes footer hint line.
struct ChangesHintCtx {
    focus: PrChangesFocus,
    mode: PrDiffViewMode,
    composing: bool,
    no_selected_file: bool,
    blob: BlobHintState,
}

/// Build the contextual hint line for the Changes footer.
///
/// Hints are contextual (issue #376 acceptance A11):
/// - While composing: a stable local status; the generated keybind bar owns shortcuts.
/// - File-list focus: file-list navigation + view toggle.
/// - Content focus with no selected file: a placeholder hint.
/// - Blob failure (when full file is needed): retry hint.
fn changes_hints(ctx: ChangesHintCtx) -> String {
    if ctx.composing {
        return "Composer active".to_string();
    }
    let view_hint = match ctx.mode {
        PrDiffViewMode::DeltasOnly => "v full file",
        PrDiffViewMode::FullFile => "v deltas",
    };
    match ctx.focus {
        PrChangesFocus::FileList => {
            format!("^/v select | Enter/Tab content | {view_hint} | Esc back")
        }
        PrChangesFocus::Content if ctx.no_selected_file => "BackTab files | Esc back".to_string(),
        PrChangesFocus::Content if matches!(ctx.blob, BlobHintState::Loading) => {
            format!("Loading full file… | {view_hint} | BackTab files | Esc back")
        }
        PrChangesFocus::Content if matches!(ctx.blob, BlobHintState::Failed) => {
            format!("r retry | {view_hint} | BackTab files | Esc back")
        }
        PrChangesFocus::Content => {
            format!(
                "^/v line | Tab files | {view_hint} | c comment | r reply | R resolve | Esc back"
            )
        }
    }
}

/// Build the embedded composer `TextBox` element using the established
/// wrapping/caret composer convention (issue #376 acceptance A11).
fn composer_box(
    composer: Option<&(String, usize, &'static str)>,
    inline_state: &crate::state::InlineState,
    composer_rows: usize,
    rc: ResolvedColors,
) -> Vec<AnyElement<'static>> {
    let Some((text, byte_cursor, prefix)) = composer else {
        return Vec::new();
    };
    let label = composer_label(inline_state);
    let content_width = usize::from(crate::layout::prs_detail_content_width(120));
    vec![
        element! {
            Box(
                border_style: BorderStyle::Single,
                border_color: rc.border_focused,
                flex_direction: FlexDirection::Column,
                padding_left: 1u32,
            ) {
                Box(height: 1u32) {
                    Text(content: label, color: rc.bright, wrap: TextWrap::NoWrap)
                }
                TextBox(
                    text: text.clone(),
                    byte_cursor: *byte_cursor,
                    viewport_rows: composer_rows,
                    content_width,
                    prefix: (*prefix).to_string(),
                    color: rc.fg,
                    caret_color: rc.bg,
                    caret_bg: rc.bright,
                )
            }
        }
        .into(),
    ]
}

/// Derive the stable user-facing composer label from the inline state.
fn composer_label(inline_state: &crate::state::InlineState) -> String {
    let crate::state::InlineState::Composer { target, .. } = inline_state else {
        return String::new();
    };
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::PrReviewCommentTarget;
    use crate::state::{ComposerTarget, InlineState};

    #[test]
    fn composing_hint_shows_only_submit_and_cancel() {
        let hint = changes_hints(ChangesHintCtx {
            focus: PrChangesFocus::Content,
            mode: PrDiffViewMode::DeltasOnly,
            composing: true,
            no_selected_file: false,
            blob: BlobHintState::Idle,
        });
        assert_eq!(hint, "Composer active");
    }

    #[test]
    fn file_list_hint_shows_navigation_and_view_toggle() {
        let hint = changes_hints(ChangesHintCtx {
            focus: PrChangesFocus::FileList,
            mode: PrDiffViewMode::DeltasOnly,
            composing: false,
            no_selected_file: false,
            blob: BlobHintState::Idle,
        });
        assert!(hint.contains("Enter/Tab content"));
        assert!(hint.contains("v full file"));
        assert!(hint.contains("Esc back"));
        assert!(
            !hint.contains("c comment"),
            "file-list hint must not advertise content-only comment key"
        );
    }

    #[test]
    fn content_hint_shows_comment_reply_resolve() {
        let hint = changes_hints(ChangesHintCtx {
            focus: PrChangesFocus::Content,
            mode: PrDiffViewMode::DeltasOnly,
            composing: false,
            no_selected_file: false,
            blob: BlobHintState::Idle,
        });
        assert!(hint.contains("c comment"));
        assert!(hint.contains("r reply"));
        assert!(hint.contains("R resolve"));
    }

    #[test]
    fn content_hint_shows_retry_when_blob_failed() {
        let hint = changes_hints(ChangesHintCtx {
            focus: PrChangesFocus::Content,
            mode: PrDiffViewMode::FullFile,
            composing: false,
            no_selected_file: false,
            blob: BlobHintState::Failed,
        });
        assert!(hint.contains("r retry"));
    }

    #[test]
    fn composer_box_renders_textbox_when_active() {
        let target = ComposerTarget::NewReviewThread {
            target: PrReviewCommentTarget {
                path: "src/app.rs".to_string(),
                line: 3,
                side: crate::domain::PrReviewThreadSide::Right,
                commit_id: "head376".to_string(),
            },
        };
        let inline_state = InlineState::Composer {
            target,
            text: "draft".to_string(),
            cursor: 5,
        };
        let composer = composer_from_inline_state(&inline_state);
        let rc = ResolvedColors::from_theme(Some(&ThemeColors::default()));
        let elements = composer_box(composer.as_ref(), &inline_state, 3, rc);
        assert_eq!(elements.len(), 1);
    }

    #[test]
    fn composer_box_returns_empty_when_no_composer() {
        let inline_state = InlineState::None;
        let composer = composer_from_inline_state(&inline_state);
        let rc = ResolvedColors::from_theme(Some(&ThemeColors::default()));
        let elements = composer_box(composer.as_ref(), &inline_state, 3, rc);
        assert!(elements.is_empty());
    }
}
