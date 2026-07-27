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
