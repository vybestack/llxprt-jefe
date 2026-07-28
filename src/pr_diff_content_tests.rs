//! Pure projection tests for pull-request deltas.

use crate::domain::{PrFileBlob, PrFileChange, PrFileStatus};
use crate::pr_diff_content::{
    DiffRowRole, build_delta_document, build_file_rows, build_file_rows_window, build_full_document,
};

fn file(status: PrFileStatus, path: &str, diff: Option<&str>) -> PrFileChange {
    PrFileChange {
        blob_sha: format!("blob-{path}"),
        path: path.to_string(),
        previous_path: None,
        status,
        additions: 1,
        deletions: 1,
        changes: 2,
        patch: diff.map(str::to_string),
    }
}

#[test]
fn removed_file_row_has_required_dash_marker_and_counts() {
    let rows = build_file_rows(&[file(PrFileStatus::Removed, "docs/old.md", None)]);

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].text, "- D docs/old.md  +1 -1");
    assert_eq!(rows[0].role, DiffRowRole::Removed);
}

#[test]
fn file_row_window_keeps_a_late_selection_visible() {
    let files = (0..12)
        .map(|index| {
            file(
                PrFileStatus::Modified,
                &format!("src/file-{index}.rs"),
                None,
            )
        })
        .collect::<Vec<_>>();

    let rows = build_file_rows_window(&files, Some(11), 8);

    assert_eq!(rows.len(), 8);
    assert_eq!(rows.last().map(|(index, _)| *index), Some(11));
}
#[test]
fn delta_document_preserves_hunk_and_line_roles_and_comment_anchors() {
    let changed = file(
        PrFileStatus::Modified,
        "src/main.rs",
        Some("@@ -41,2 +41,2 @@\n fn render() {\n-old_call();\n+new_call();"),
    );

    let document = build_delta_document(&changed);

    assert_eq!(document.rows[0].role, DiffRowRole::Hunk);
    assert_eq!(document.rows[2].text, "-  42 old_call();");
    assert_eq!(document.rows[2].role, DiffRowRole::Removed);
    assert_eq!(document.rows[2].anchor.as_ref().map(|a| a.line), Some(42));
    assert_eq!(document.rows[3].text, "+  42 new_call();");
    assert_eq!(document.rows[3].role, DiffRowRole::Added);
    assert_eq!(document.rows[3].anchor.as_ref().map(|a| a.line), Some(42));
}

#[test]
fn unavailable_patch_is_explicit_instead_of_an_empty_document() {
    let changed = file(PrFileStatus::Modified, "binary.dat", None);

    let document = build_delta_document(&changed);

    assert_eq!(document.rows.len(), 1);
    assert!(document.rows[0].text.contains("Delta unavailable"));
    assert_eq!(document.rows[0].role, DiffRowRole::Notice);
}

#[test]
fn full_document_interleaves_removed_rows_and_marks_added_rows() {
    let changed = file(
        PrFileStatus::Modified,
        "src/main.rs",
        Some(
            "@@ -1,4 +1,5 @@\n fn render() {\n     let unchanged = true;\n-old_call();\n+new_call();\n+assert!(unchanged);\n }",
        ),
    );
    let blob = PrFileBlob::Text(
        "fn render() {\n    let unchanged = true;\n    new_call();\n    assert!(unchanged);\n}\n"
            .to_string(),
    );

    let document = build_full_document(&changed, &blob);

    assert!(
        document
            .rows
            .iter()
            .any(|row| row.text.contains("old_call();") && row.role == DiffRowRole::Removed)
    );
    assert!(
        document
            .rows
            .iter()
            .any(|row| row.text.contains("assert!(unchanged);") && row.role == DiffRowRole::Added)
    );
    assert!(
        document.rows.iter().any(
            |row| row.text.contains("let unchanged = true;") && row.role == DiffRowRole::Normal
        )
    );
}

#[test]
fn removed_full_file_marks_every_prior_line_removed() {
    let changed = file(
        PrFileStatus::Removed,
        "docs/old.md",
        Some("@@ -1,2 +0,0 @@\n-old\n-docs"),
    );

    let document = build_full_document(&changed, &PrFileBlob::Text("old\ndocs\n".to_string()));

    assert_eq!(document.rows.len(), 2);
    assert!(
        document
            .rows
            .iter()
            .all(|row| row.role == DiffRowRole::Removed)
    );
}

#[test]
fn full_document_retains_removed_rows_beyond_the_available_blob_lines() {
    let changed = file(
        PrFileStatus::Modified,
        "src/main.rs",
        Some("@@ -5 +5,0 @@\n-trailing_old_line();"),
    );

    let document = build_full_document(
        &changed,
        &PrFileBlob::Text("only_current_line();\n".to_string()),
    );

    assert!(document.rows.iter().any(|row| {
        row.text.contains("trailing_old_line();") && row.role == DiffRowRole::Removed
    }));
}
#[test]
fn threads_render_at_exact_side_and_unmapped_threads_remain_visible() {
    use crate::domain::{IssueComment, PrReviewThread, PrReviewThreadAnchor, PrReviewThreadSide};
    use crate::pr_diff_content::build_threaded_document;

    let changed = file(
        PrFileStatus::Modified,
        "src/main.rs",
        Some("@@ -1 +1 @@\n-old_call();\n+new_call();"),
    );
    let mapped = PrReviewThread {
        thread_id: "thread-right".to_string(),
        is_resolved: false,
        is_outdated: false,
        review_id: None,
        path: Some("src/main.rs".to_string()),
        line: Some(1),
        anchor: Some(PrReviewThreadAnchor {
            side: PrReviewThreadSide::Right,
            start_side: Some(PrReviewThreadSide::Right),
            start_line: Some(1),
            original_line: None,
            original_start_line: None,
        }),
        comments: vec![IssueComment {
            comment_id: 1,
            author_login: "reviewer".to_string(),
            created_at: "2026-07-01T00:00:00Z".to_string(),
            edited_at: None,
            body: "Use the shared helper".to_string(),
        }],
    };
    let mut unmapped = mapped.clone();
    unmapped.thread_id = "thread-old".to_string();
    unmapped.is_outdated = true;
    unmapped.line = Some(99);

    let document = build_threaded_document(
        &changed,
        build_delta_document(&changed),
        &[mapped, unmapped],
    );

    assert!(document.rows.iter().any(|row| {
        row.text.contains("reviewer: Use the shared helper") && row.thread_index == Some(0)
    }));
    assert!(
        document
            .rows
            .iter()
            .any(|row| row.text == "Unmapped reviews")
    );
    assert!(
        document
            .rows
            .iter()
            .any(|row| { row.text.contains("outdated") && row.thread_index == Some(1) })
    );
}

#[test]
fn left_side_thread_maps_to_the_removed_line() {
    use crate::domain::{IssueComment, PrReviewThread, PrReviewThreadAnchor, PrReviewThreadSide};
    use crate::pr_diff_content::build_threaded_document;

    let changed = file(
        PrFileStatus::Modified,
        "src/main.rs",
        Some(
            "@@ -1 +1 @@
-old_call();
+new_call();",
        ),
    );
    let thread = PrReviewThread {
        thread_id: "thread-left".to_string(),
        is_resolved: false,
        is_outdated: false,
        review_id: None,
        path: Some("src/main.rs".to_string()),
        line: Some(1),
        anchor: Some(PrReviewThreadAnchor {
            side: PrReviewThreadSide::Left,
            start_side: Some(PrReviewThreadSide::Left),
            start_line: Some(1),
            original_line: Some(1),
            original_start_line: Some(1),
        }),
        comments: vec![IssueComment {
            comment_id: 1,
            author_login: "reviewer".to_string(),
            created_at: "2026-07-01T00:00:00Z".to_string(),
            edited_at: None,
            body: "Keep this behavior".to_string(),
        }],
    };

    let document = build_threaded_document(&changed, build_delta_document(&changed), &[thread]);
    let removed_index = document
        .rows
        .iter()
        .position(|row| row.text.contains("old_call"))
        .unwrap_or_else(|| panic!("removed line should render"));
    let review_index = document
        .rows
        .iter()
        .position(|row| row.text.contains("reviewer: Keep this behavior"))
        .unwrap_or_else(|| panic!("left-side thread should render"));

    assert_eq!(review_index, removed_index + 2);
    assert_eq!(document.rows[review_index].thread_index, Some(0));
}

#[test]
fn outdated_thread_label_uses_original_line_when_current_line_is_absent() {
    use crate::domain::{IssueComment, PrReviewThread, PrReviewThreadAnchor, PrReviewThreadSide};
    use crate::pr_diff_content::build_threaded_document;

    let changed = file(
        PrFileStatus::Modified,
        "src/main.rs",
        Some("@@ -4 +4 @@\n-old\n+new"),
    );
    let thread = PrReviewThread {
        thread_id: "thread-outdated".to_string(),
        is_resolved: false,
        is_outdated: true,
        review_id: None,
        path: Some("src/main.rs".to_string()),
        line: None,
        anchor: Some(PrReviewThreadAnchor {
            side: PrReviewThreadSide::Left,
            start_side: Some(PrReviewThreadSide::Left),
            start_line: None,
            original_line: Some(4),
            original_start_line: None,
        }),
        comments: vec![IssueComment {
            comment_id: 1,
            author_login: "reviewer".to_string(),
            created_at: "2026-07-01T00:00:00Z".to_string(),
            edited_at: None,
            body: "Keep the prior line".to_string(),
        }],
    };

    let document = build_threaded_document(&changed, build_delta_document(&changed), &[thread]);
    let label = document
        .rows
        .iter()
        .find(|row| row.thread_index == Some(0))
        .map_or_else(
            || panic!("outdated thread should remain visible"),
            |row| row.text.as_str(),
        );

    assert!(label.contains("line 4"), "unexpected thread label: {label}");
}

// ---------------------------------------------------------------------------
// Issue #376 remediation: Full File with text but absent/malformed patch must
// render an explicit `Delta highlighting unavailable` notice while preserving
// the full text and inventing no comment anchors.
// ---------------------------------------------------------------------------

#[test]
fn full_file_with_absent_patch_renders_delta_unavailable_notice_and_preserves_text() {
    let changed = file(PrFileStatus::Modified, "src/main.rs", None);
    let blob = PrFileBlob::Text(
        "fn render() {
    new_call();
}
"
        .to_string(),
    );

    let document = build_full_document(&changed, &blob);

    let notice = document.rows.iter().find(|row| {
        row.role == DiffRowRole::Notice && row.text.contains("Delta highlighting unavailable")
    });
    assert!(notice.is_some(), "must render the delta-unavailable notice");

    assert!(
        document
            .rows
            .iter()
            .any(|row| row.text.contains("new_call();")),
        "must preserve full file text"
    );
    assert!(
        document
            .rows
            .iter()
            .any(|row| row.text.contains("fn render()")),
        "must preserve full file text"
    );
    assert!(
        document.rows.iter().all(|row| row.anchor.is_none()),
        "must invent no comment anchors when patch is absent"
    );
}

#[test]
fn full_file_with_malformed_patch_renders_delta_unavailable_notice_and_preserves_text() {
    let changed = file(
        PrFileStatus::Modified,
        "src/main.rs",
        Some("not a valid patch"),
    );
    let blob = PrFileBlob::Text(
        "fn render() {
    new_call();
}
"
        .to_string(),
    );

    let document = build_full_document(&changed, &blob);

    assert!(
        document
            .rows
            .iter()
            .any(|row| row.role == DiffRowRole::Notice
                && row.text.contains("Delta highlighting unavailable")),
        "must render the delta-unavailable notice for a malformed patch"
    );
    assert!(
        document
            .rows
            .iter()
            .any(|row| row.text.contains("new_call();")),
        "must preserve full file text despite malformed patch"
    );
    assert!(
        document.rows.iter().all(|row| row.anchor.is_none()),
        "must invent no comment anchors when patch is malformed"
    );
}

#[test]
fn full_file_with_absent_patch_on_added_file_renders_notice_and_preserves_text() {
    let changed = file(PrFileStatus::Added, "src/new.rs", None);
    let blob = PrFileBlob::Text(
        "fn new() {}
"
        .to_string(),
    );

    let document = build_full_document(&changed, &blob);

    assert!(
        document
            .rows
            .iter()
            .any(|row| row.role == DiffRowRole::Notice
                && row.text.contains("Delta highlighting unavailable")),
        "added-file full view must render the delta-unavailable notice"
    );
    assert!(
        document
            .rows
            .iter()
            .any(|row| row.text.contains("fn new()")),
        "added-file full view must preserve text"
    );
    assert!(
        document.rows.iter().all(|row| row.anchor.is_none()),
        "added-file full view must invent no anchors"
    );
}

#[test]
fn full_file_with_absent_patch_on_removed_file_renders_notice_and_preserves_text() {
    let changed = file(PrFileStatus::Removed, "docs/old.md", None);
    let blob = PrFileBlob::Text(
        "Old content.
"
        .to_string(),
    );

    let document = build_full_document(&changed, &blob);

    assert!(
        document
            .rows
            .iter()
            .any(|row| row.role == DiffRowRole::Notice
                && row.text.contains("Delta highlighting unavailable")),
        "removed-file full view must render the delta-unavailable notice"
    );
    assert!(
        document
            .rows
            .iter()
            .any(|row| row.text.contains("Old content")),
        "removed-file full view must preserve text"
    );
    assert!(
        document.rows.iter().all(|row| row.anchor.is_none()),
        "removed-file full view must invent no anchors"
    );
}

// ---------------------------------------------------------------------------
// Issue #376 remediation: thread routing — outdated threads must go to the
// selected file's Unmapped section rather than collide with current
// coordinates; pathless/degraded threads must remain visible exactly once.
// ---------------------------------------------------------------------------

#[test]
fn outdated_thread_goes_to_unmapped_even_when_its_line_matches_a_current_anchor() {
    use crate::domain::{IssueComment, PrReviewThread, PrReviewThreadAnchor, PrReviewThreadSide};
    use crate::pr_diff_content::build_threaded_document;

    // Current RIGHT side line 1 is an addition with a commentable anchor.
    let changed = file(
        PrFileStatus::Modified,
        "src/main.rs",
        Some(
            "@@ -1 +1 @@
-old_call();
+new_call();",
        ),
    );
    // An outdated thread whose original/current line coincidentally equals 1
    // must NOT collide with the current RIGHT-line-1 addition anchor.
    let outdated = PrReviewThread {
        thread_id: "thread-outdated".to_string(),
        is_resolved: false,
        is_outdated: true,
        review_id: None,
        path: Some("src/main.rs".to_string()),
        line: Some(1),
        anchor: Some(PrReviewThreadAnchor {
            side: PrReviewThreadSide::Right,
            start_side: Some(PrReviewThreadSide::Right),
            start_line: Some(1),
            original_line: Some(1),
            original_start_line: Some(1),
        }),
        comments: vec![IssueComment {
            comment_id: 1,
            author_login: "reviewer".to_string(),
            created_at: "2026-07-01T00:00:00Z".to_string(),
            edited_at: None,
            body: "Old concern".to_string(),
        }],
    };

    let document = build_threaded_document(&changed, build_delta_document(&changed), &[outdated]);

    let unmapped_index = document
        .rows
        .iter()
        .position(|row| row.text == "Unmapped reviews");
    assert!(
        unmapped_index.is_some(),
        "outdated thread must reach the Unmapped section"
    );

    let review_label_index = document
        .rows
        .iter()
        .position(|row| row.text.contains("outdated") && row.thread_index == Some(0));
    assert!(
        matches!((review_label_index, unmapped_index), (Some(rev), Some(um)) if rev > um),
        "outdated thread rows must appear after the Unmapped header"
    );
    // The outdated thread must NOT render immediately after the current
    // RIGHT-line-1 addition row (no collision).
    let addition_index = document
        .rows
        .iter()
        .position(|row| row.text.contains("new_call();"));
    assert!(
        matches!((addition_index, unmapped_index), (Some(add), Some(um)) if add < um),
        "current addition must render before the Unmapped section"
    );
}

#[test]
fn pathless_thread_remains_visible_exactly_once_in_unmapped() {
    use crate::domain::{IssueComment, PrReviewThread, PrReviewThreadAnchor, PrReviewThreadSide};
    use crate::pr_diff_content::build_threaded_document;

    let changed = file(
        PrFileStatus::Modified,
        "src/main.rs",
        Some(
            "@@ -1 +1 @@
-old_call();
+new_call();",
        ),
    );
    // A pathless (degraded) thread with no path.
    let pathless = PrReviewThread {
        thread_id: "thread-pathless".to_string(),
        is_resolved: false,
        is_outdated: false,
        review_id: None,
        path: None,
        line: Some(1),
        anchor: Some(PrReviewThreadAnchor {
            side: PrReviewThreadSide::Right,
            start_side: Some(PrReviewThreadSide::Right),
            start_line: Some(1),
            original_line: None,
            original_start_line: None,
        }),
        comments: vec![IssueComment {
            comment_id: 1,
            author_login: "reviewer".to_string(),
            created_at: "2026-07-01T00:00:00Z".to_string(),
            edited_at: None,
            body: "Where is this?".to_string(),
        }],
    };

    let document = build_threaded_document(&changed, build_delta_document(&changed), &[pathless]);

    let count = document
        .rows
        .iter()
        .filter(|row| row.text.contains("Where is this?"))
        .count();
    assert_eq!(count, 1, "pathless thread must appear exactly once");
    assert!(
        document
            .rows
            .iter()
            .any(|row| row.text == "Unmapped reviews"),
        "pathless thread must render under the Unmapped section"
    );
}

#[test]
fn thread_with_missing_anchor_metadata_goes_to_unmapped() {
    use crate::domain::{IssueComment, PrReviewThread};
    use crate::pr_diff_content::build_threaded_document;

    let changed = file(
        PrFileStatus::Modified,
        "src/main.rs",
        Some(
            "@@ -1 +1 @@
-old_call();
+new_call();",
        ),
    );
    // Thread matches the file path but its anchor metadata is missing (None),
    // so it cannot be placed at a concrete row.
    let no_anchor = PrReviewThread {
        thread_id: "thread-no-anchor".to_string(),
        is_resolved: false,
        is_outdated: false,
        review_id: None,
        path: Some("src/main.rs".to_string()),
        line: None,
        anchor: None,
        comments: vec![IssueComment {
            comment_id: 1,
            author_login: "reviewer".to_string(),
            created_at: "2026-07-01T00:00:00Z".to_string(),
            edited_at: None,
            body: "Unanchored".to_string(),
        }],
    };

    let document = build_threaded_document(&changed, build_delta_document(&changed), &[no_anchor]);

    assert!(
        document
            .rows
            .iter()
            .any(|row| row.text == "Unmapped reviews"),
        "missing-anchor thread must reach the Unmapped section"
    );
    let count = document
        .rows
        .iter()
        .filter(|row| row.text.contains("Unanchored"))
        .count();
    assert_eq!(count, 1, "missing-anchor thread must appear exactly once");
}

#[test]
fn outdated_thread_range_uses_original_range_in_unmapped_label() {
    use crate::domain::{IssueComment, PrReviewThread, PrReviewThreadAnchor, PrReviewThreadSide};
    use crate::pr_diff_content::build_threaded_document;

    let changed = file(
        PrFileStatus::Modified,
        "src/main.rs",
        Some(
            "@@ -1 +1 @@
-old_call();
+new_call();",
        ),
    );
    let outdated_range = PrReviewThread {
        thread_id: "thread-outdated-range".to_string(),
        is_resolved: false,
        is_outdated: true,
        review_id: None,
        path: Some("src/main.rs".to_string()),
        line: None,
        anchor: Some(PrReviewThreadAnchor {
            side: PrReviewThreadSide::Left,
            start_side: Some(PrReviewThreadSide::Left),
            start_line: None,
            original_line: Some(7),
            original_start_line: Some(4),
        }),
        comments: vec![IssueComment {
            comment_id: 1,
            author_login: "reviewer".to_string(),
            created_at: "2026-07-01T00:00:00Z".to_string(),
            edited_at: None,
            body: "Range concern".to_string(),
        }],
    };

    let document =
        build_threaded_document(&changed, build_delta_document(&changed), &[outdated_range]);
    let label = document
        .rows
        .iter()
        .find(|row| row.thread_index == Some(0))
        .map_or_else(
            || panic!("outdated range thread should render"),
            |row| row.text.as_str(),
        );

    assert!(
        label.contains("line 4-7"),
        "outdated range label must use the original range: {label}"
    );
    assert!(
        label.contains("outdated"),
        "outdated range label must mark the thread outdated: {label}"
    );
}

#[test]
fn renamed_file_thread_maps_to_the_current_path() {
    use crate::domain::{IssueComment, PrReviewThread, PrReviewThreadAnchor, PrReviewThreadSide};
    use crate::pr_diff_content::build_threaded_document;

    // File was renamed; thread refers to the previous path but is NOT outdated.
    let mut changed = file(
        PrFileStatus::Renamed,
        "src/new_name.rs",
        Some(
            "@@ -1 +1 @@
 context
+added();",
        ),
    );
    changed.previous_path = Some("src/old_name.rs".to_string());

    let thread_on_old_path = PrReviewThread {
        thread_id: "thread-renamed".to_string(),
        is_resolved: false,
        is_outdated: false,
        review_id: None,
        path: Some("src/old_name.rs".to_string()),
        line: Some(2),
        anchor: Some(PrReviewThreadAnchor {
            side: PrReviewThreadSide::Right,
            start_side: Some(PrReviewThreadSide::Right),
            start_line: Some(2),
            original_line: Some(2),
            original_start_line: Some(2),
        }),
        comments: vec![IssueComment {
            comment_id: 1,
            author_login: "reviewer".to_string(),
            created_at: "2026-07-01T00:00:00Z".to_string(),
            edited_at: None,
            body: "Renamed mapping".to_string(),
        }],
    };

    let document = build_threaded_document(
        &changed,
        build_delta_document(&changed),
        &[thread_on_old_path],
    );

    // The thread should map inline (not in Unmapped) because the rename is
    // preserved for non-outdated threads.
    let added_index = document
        .rows
        .iter()
        .position(|row| row.text.contains("added();"));
    let review_index = document
        .rows
        .iter()
        .position(|row| row.text.contains("Renamed mapping"));
    assert!(
        document
            .rows
            .iter()
            .all(|row| row.text != "Unmapped reviews"),
        "non-outdated renamed-path thread must NOT go to Unmapped"
    );
    assert!(
        added_index.is_some_and(|add| review_index.is_some_and(|rev| rev > add)),
        "renamed-path thread must render inline after its anchor row"
    );
}

#[test]
fn full_file_with_absent_patch_shows_delta_notice_and_preserves_text_without_anchors() {
    let changed = file(PrFileStatus::Modified, "src/main.rs", None);
    let blob = PrFileBlob::Text("fn main() {}\n".to_string());

    let document = build_full_document(&changed, &blob);

    assert!(
        document.rows.iter().any(|row| {
            row.role == DiffRowRole::Notice && row.text.contains("Delta highlighting unavailable")
        }),
        "must show explicit Delta highlighting unavailable notice"
    );
    assert!(
        document
            .rows
            .iter()
            .any(|row| row.text.contains("fn main() {}")),
        "must preserve full text"
    );
    assert!(
        document.rows.iter().all(|row| row.anchor.is_none()),
        "must not invent highlight/comment anchors when patch is absent"
    );
}

#[test]
fn full_file_with_malformed_patch_shows_delta_notice_and_preserves_text_without_anchors() {
    let changed = file(
        PrFileStatus::Modified,
        "src/main.rs",
        Some("not a valid patch"),
    );
    let blob = PrFileBlob::Text("fn main() {}\n".to_string());

    let document = build_full_document(&changed, &blob);

    assert!(
        document.rows.iter().any(|row| {
            row.role == DiffRowRole::Notice && row.text.contains("Delta highlighting unavailable")
        }),
        "must show explicit Delta highlighting unavailable notice for malformed patch"
    );
    assert!(
        document
            .rows
            .iter()
            .any(|row| row.text.contains("fn main() {}")),
        "must preserve full text even with malformed patch"
    );
    assert!(
        document.rows.iter().all(|row| row.anchor.is_none()),
        "must not invent anchors when patch is malformed"
    );
}

#[test]
fn full_file_absent_patch_notice_applies_to_added_status() {
    let changed = file(PrFileStatus::Added, "src/new.rs", None);
    let blob = PrFileBlob::Text("fn new() {}\n".to_string());

    let document = build_full_document(&changed, &blob);

    assert!(
        document
            .rows
            .iter()
            .any(|row| row.text.contains("Delta highlighting unavailable")),
        "added status with absent patch must show notice"
    );
    assert!(
        document
            .rows
            .iter()
            .any(|row| row.text.contains("fn new() {}")),
        "added status must preserve full text"
    );
}

#[test]
fn full_file_absent_patch_notice_applies_to_removed_status() {
    let changed = file(PrFileStatus::Removed, "docs/old.md", None);
    let blob = PrFileBlob::Text("old content\n".to_string());

    let document = build_full_document(&changed, &blob);

    assert!(
        document
            .rows
            .iter()
            .any(|row| row.text.contains("Delta highlighting unavailable")),
        "removed status with absent patch must show notice"
    );
    assert!(
        document
            .rows
            .iter()
            .any(|row| row.text.contains("old content")),
        "removed status must preserve full prior text"
    );
}

#[test]
fn full_file_absent_patch_notice_applies_to_renamed_status() {
    let changed = file(PrFileStatus::Renamed, "src/renamed.rs", None);
    let blob = PrFileBlob::Text("fn renamed() {}\n".to_string());

    let document = build_full_document(&changed, &blob);

    assert!(
        document
            .rows
            .iter()
            .any(|row| row.text.contains("Delta highlighting unavailable")),
        "renamed status with absent patch must show notice"
    );
}
