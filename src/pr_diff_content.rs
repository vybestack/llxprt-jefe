//! Pure, iocraft-free display projection for pull-request changes.

use std::collections::{BTreeMap, BTreeSet};

use crate::domain::{
    DiffLineKind, ParsedDiff, PrFileBlob, PrFileChange, PrFileStatus, parse_unified_diff,
};

/// Semantic color role for one projected row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffRowRole {
    Normal,
    Added,
    Removed,
    Hunk,
    Notice,
}

/// GitHub review-comment side for a patch-addressable row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DiffAnchorSide {
    Left,
    Right,
}

/// Line anchor carried by a commentable projected row.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DiffRowAnchor {
    pub path: String,
    pub line: u32,
    pub side: DiffAnchorSide,
}

/// One changed-file list row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrFileRow {
    pub text: String,
    pub role: DiffRowRole,
}

/// One line in the delta content document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffDocumentRow {
    pub text: String,
    pub role: DiffRowRole,
    pub anchor: Option<DiffRowAnchor>,
    pub thread_index: Option<usize>,
}

/// Projected delta document for one changed file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiffDocument {
    pub rows: Vec<DiffDocumentRow>,
}

/// Build changed-file list rows with stable status markers and counts.
#[must_use]
pub fn build_file_rows(files: &[PrFileChange]) -> Vec<PrFileRow> {
    files
        .iter()
        .map(|file| PrFileRow {
            text: format!(
                "{} {} {}  +{} -{}",
                status_prefix(&file.status),
                file.status.marker(),
                file.path,
                file.additions,
                file.deletions
            ),
            role: status_role(&file.status),
        })
        .collect()
}

/// Build a bounded file-list window that keeps the selected file visible.
#[must_use]
pub fn build_file_rows_window(
    files: &[PrFileChange],
    selected: Option<usize>,
    max_rows: usize,
) -> Vec<(usize, PrFileRow)> {
    if files.is_empty() || max_rows == 0 {
        return Vec::new();
    }
    let selected = selected.unwrap_or(0).min(files.len() - 1);
    let window_len = max_rows.min(files.len());
    let start = selected
        .saturating_sub(window_len / 2)
        .min(files.len() - window_len);
    build_file_rows(files)
        .into_iter()
        .enumerate()
        .skip(start)
        .take(window_len)
        .collect()
}
/// Build a bounded diff-document window that keeps the selected row visible.
#[must_use]
pub fn build_document_window(
    document: &DiffDocument,
    selected: Option<usize>,
    max_rows: usize,
) -> Vec<(usize, DiffDocumentRow)> {
    if document.rows.is_empty() || max_rows == 0 {
        return Vec::new();
    }
    let selected = selected.unwrap_or(0).min(document.rows.len() - 1);
    let window_len = max_rows.min(document.rows.len());
    let start = selected
        .saturating_sub(window_len / 2)
        .min(document.rows.len() - window_len);
    document
        .rows
        .iter()
        .cloned()
        .enumerate()
        .skip(start)
        .take(window_len)
        .collect()
}

/// Build the deltas-only document returned by GitHub's unified patch.
#[must_use]
pub fn build_delta_document(file: &PrFileChange) -> DiffDocument {
    match parse_unified_diff(file.patch.as_deref()) {
        ParsedDiff::Hunks(hunks) => DiffDocument {
            rows: hunks
                .into_iter()
                .flat_map(|hunk| {
                    std::iter::once(DiffDocumentRow {
                        text: hunk.header,
                        role: DiffRowRole::Hunk,
                        anchor: None,
                        thread_index: None,
                    })
                    .chain(hunk.lines.into_iter().map(|line| project_line(file, line)))
                })
                .collect(),
        },
        ParsedDiff::Unavailable => notice_document("Delta unavailable for this file"),
        ParsedDiff::Malformed(error) => notice_document(&format!("Malformed delta: {error}")),
    }
}
/// Build a full-file document with patch additions and removals interleaved.
#[must_use]
pub fn build_full_document(file: &PrFileChange, blob: &PrFileBlob) -> DiffDocument {
    match blob {
        PrFileBlob::Binary => notice_document("Binary file content cannot be displayed"),
        PrFileBlob::Truncated { byte_size } => {
            notice_document(&format!("File content truncated ({byte_size} bytes)"))
        }
        PrFileBlob::Text(text) if matches!(file.status, PrFileStatus::Removed) => {
            build_one_sided_full_document(file, text, DiffRowRole::Removed, DiffAnchorSide::Left)
        }
        PrFileBlob::Text(text) if matches!(file.status, PrFileStatus::Added) => {
            build_one_sided_full_document(file, text, DiffRowRole::Added, DiffAnchorSide::Right)
        }
        PrFileBlob::Text(text) => merge_full_text(file, text),
    }
}

fn merge_full_text(file: &PrFileChange, text: &str) -> DiffDocument {
    let mut removed: BTreeMap<u32, Vec<DiffDocumentRow>> = BTreeMap::new();

    let mut added = BTreeSet::new();
    let mut right_anchors = BTreeSet::new();
    if let ParsedDiff::Hunks(hunks) = parse_unified_diff(file.patch.as_deref()) {
        for hunk in hunks {
            let mut new_position = hunk.new_start;
            for line in hunk.lines {
                if line.kind == DiffLineKind::Removed {
                    removed
                        .entry(new_position)
                        .or_default()
                        .push(project_line(file, line));
                } else if let Some(new_line) = line.new_line {
                    right_anchors.insert(new_line);
                    if line.kind == DiffLineKind::Added {
                        added.insert(new_line);
                    }
                    new_position = new_line.saturating_add(1);
                }
            }
        }
    }
    let mut rows = Vec::new();
    for (index, content) in text.lines().enumerate() {
        let line = u32::try_from(index + 1).unwrap_or(u32::MAX);
        if let Some(deleted) = removed.remove(&line) {
            rows.extend(deleted);
        }
        let role = if added.contains(&line) {
            DiffRowRole::Added
        } else {
            DiffRowRole::Normal
        };
        rows.push(full_text_row(
            file,
            index + 1,
            content,
            role,
            right_anchors.contains(&line),
        ));
    }
    for deleted in removed.into_values() {
        rows.extend(deleted);
    }
    DiffDocument { rows }
}

fn build_one_sided_full_document(
    file: &PrFileChange,
    text: &str,
    role: DiffRowRole,
    side: DiffAnchorSide,
) -> DiffDocument {
    let commentable = patch_lines_for_side(file, side);
    DiffDocument {
        rows: text
            .lines()
            .enumerate()
            .map(|(index, content)| {
                let line = u32::try_from(index + 1).unwrap_or(u32::MAX);
                full_text_row(file, index + 1, content, role, commentable.contains(&line))
            })
            .collect(),
    }
}

fn patch_lines_for_side(file: &PrFileChange, side: DiffAnchorSide) -> BTreeSet<u32> {
    let mut lines = BTreeSet::new();
    if let ParsedDiff::Hunks(hunks) = parse_unified_diff(file.patch.as_deref()) {
        for line in hunks.into_iter().flat_map(|hunk| hunk.lines) {
            let number = match side {
                DiffAnchorSide::Left => line.old_line,
                DiffAnchorSide::Right => line.new_line,
            };
            if let Some(number) = number {
                lines.insert(number);
            }
        }
    }
    lines
}

fn full_text_row(
    file: &PrFileChange,
    line_index: usize,
    content: &str,
    role: DiffRowRole,
    commentable: bool,
) -> DiffDocumentRow {
    let line = u32::try_from(line_index).unwrap_or(u32::MAX);
    let side = if role == DiffRowRole::Removed {
        DiffAnchorSide::Left
    } else {
        DiffAnchorSide::Right
    };
    DiffDocumentRow {
        text: format!(
            "{} {line:>3} {content}",
            if role == DiffRowRole::Added {
                '+'
            } else if role == DiffRowRole::Removed {
                '-'
            } else {
                ' '
            }
        ),
        role,
        anchor: commentable.then(|| DiffRowAnchor {
            path: file.path.clone(),
            line,
            side,
        }),
        thread_index: None,
    }
}

fn project_line(file: &PrFileChange, line: crate::domain::DiffLine) -> DiffDocumentRow {
    let (prefix, number, role, side) = match line.kind {
        DiffLineKind::Added => (
            '+',
            line.new_line,
            DiffRowRole::Added,
            Some(DiffAnchorSide::Right),
        ),
        DiffLineKind::Removed => (
            '-',
            line.old_line,
            DiffRowRole::Removed,
            Some(DiffAnchorSide::Left),
        ),
        DiffLineKind::Context => (
            ' ',
            line.new_line,
            DiffRowRole::Normal,
            Some(DiffAnchorSide::Right),
        ),
        DiffLineKind::Marker => ('\\', None, DiffRowRole::Notice, None),
    };
    let anchor = number.zip(side).map(|(line, side)| DiffRowAnchor {
        path: file.path.clone(),
        line,
        side,
    });
    DiffDocumentRow {
        text: number.map_or_else(
            || format!("{prefix}     {}", line.content),
            |number| format!("{prefix} {number:>3} {}", line.content),
        ),
        role,
        anchor,
        thread_index: None,
    }
}

/// Insert review threads after their exact diff-side line, retaining unmapped threads.
#[must_use]
pub fn build_threaded_document(
    file: &PrFileChange,
    document: DiffDocument,
    threads: &[crate::domain::PrReviewThread],
) -> DiffDocument {
    let anchor_rows = document
        .rows
        .iter()
        .enumerate()
        .filter_map(|(index, row)| row.anchor.clone().map(|anchor| (anchor, index)))
        .collect::<BTreeMap<_, _>>();
    let mut mapped: BTreeMap<usize, Vec<DiffDocumentRow>> = BTreeMap::new();
    let mut unmapped = Vec::new();
    for (thread_index, thread) in threads.iter().enumerate() {
        if !thread_matches_file(file, thread) {
            continue;
        }
        let row = thread_anchor(file, thread).and_then(|anchor| anchor_rows.get(&anchor).copied());
        let rows = project_thread(thread, thread_index);
        if let Some(row) = row {
            mapped.entry(row).or_default().extend(rows);
        } else {
            unmapped.extend(rows);
        }
    }
    let mut rows = Vec::new();
    for (index, row) in document.rows.into_iter().enumerate() {
        rows.push(row);
        if let Some(thread_rows) = mapped.remove(&index) {
            rows.extend(thread_rows);
        }
    }
    if !unmapped.is_empty() {
        rows.push(review_row("Unmapped reviews".to_string(), None));
        rows.extend(unmapped);
    }
    DiffDocument { rows }
}

fn thread_matches_file(file: &PrFileChange, thread: &crate::domain::PrReviewThread) -> bool {
    thread
        .path
        .as_deref()
        .is_some_and(|path| path == file.path || file.previous_path.as_deref() == Some(path))
}

fn thread_anchor(
    file: &PrFileChange,
    thread: &crate::domain::PrReviewThread,
) -> Option<DiffRowAnchor> {
    let metadata = thread.anchor.as_ref()?;
    let line = if thread.is_outdated {
        metadata.original_line.or(thread.line)?
    } else {
        thread.line.or(metadata.original_line)?
    };
    let side = match metadata.side {
        crate::domain::PrReviewThreadSide::Left => DiffAnchorSide::Left,
        crate::domain::PrReviewThreadSide::Right => DiffAnchorSide::Right,
    };
    Some(DiffRowAnchor {
        path: file.path.clone(),
        line,
        side,
    })
}

fn project_thread(
    thread: &crate::domain::PrReviewThread,
    thread_index: usize,
) -> Vec<DiffDocumentRow> {
    let status = if thread.is_outdated {
        "outdated"
    } else if thread.is_resolved {
        "resolved"
    } else {
        "unresolved"
    };
    let range = thread.anchor.as_ref().and_then(|anchor| {
        let end = thread.line.or(anchor.original_line);
        let start = if thread.is_outdated {
            anchor.original_start_line.or(end)
        } else {
            anchor.start_line.or(end)
        };
        start.zip(end).map(|(start, end)| {
            if start == end {
                end.to_string()
            } else {
                format!("{start}-{end}")
            }
        })
    });
    let mut rows = vec![review_row(
        format!(
            "Review [{status}] line {}",
            range.unwrap_or_else(|| "?".to_string())
        ),
        Some(thread_index),
    )];
    rows.extend(thread.comments.iter().flat_map(|comment| {
        comment.body.lines().map(|line| {
            review_row(
                format!("{}: {line}", comment.author_login),
                Some(thread_index),
            )
        })
    }));
    rows
}

fn review_row(text: String, thread_index: Option<usize>) -> DiffDocumentRow {
    DiffDocumentRow {
        text,
        role: DiffRowRole::Notice,
        anchor: None,
        thread_index,
    }
}

fn status_prefix(status: &PrFileStatus) -> char {
    if matches!(status, PrFileStatus::Removed) {
        '-'
    } else {
        ' '
    }
}

fn status_role(status: &PrFileStatus) -> DiffRowRole {
    match status {
        PrFileStatus::Added => DiffRowRole::Added,
        PrFileStatus::Removed => DiffRowRole::Removed,
        PrFileStatus::Modified
        | PrFileStatus::Renamed
        | PrFileStatus::Copied
        | PrFileStatus::Unknown(_) => DiffRowRole::Normal,
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
