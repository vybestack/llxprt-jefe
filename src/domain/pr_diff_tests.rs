//! Behavioral contracts for pull-request changed files and unified patches.

use super::{DiffLineKind, ParsedDiff, PrFileStatus, parse_unified_diff};

#[test]
fn parses_multiple_hunks_with_old_and_new_line_numbers() {
    let patch = "@@ -2,3 +2,3 @@\n same\n-old\n+new\n tail\n@@ -10,1 +10,2 @@\n context\n+extra";
    let ParsedDiff::Hunks(hunks) = parse_unified_diff(Some(patch)) else {
        panic!("valid patch must produce hunks");
    };

    assert_eq!(hunks.len(), 2);
    assert_eq!((hunks[0].old_start, hunks[0].new_start), (2, 2));
    assert_eq!(hunks[0].lines[1].kind, DiffLineKind::Removed);
    assert_eq!(hunks[0].lines[1].old_line, Some(3));
    assert_eq!(hunks[0].lines[1].new_line, None);
    assert_eq!(hunks[0].lines[2].kind, DiffLineKind::Added);
    assert_eq!(hunks[0].lines[2].old_line, None);
    assert_eq!(hunks[0].lines[2].new_line, Some(3));
    assert_eq!(hunks[1].lines[1].new_line, Some(11));
}

#[test]
fn preserves_no_newline_marker_without_advancing_line_numbers() {
    let patch = "@@ -1 +1 @@\n-old\n+new\n\\ No newline at end of file";
    let ParsedDiff::Hunks(hunks) = parse_unified_diff(Some(patch)) else {
        panic!("valid patch must produce hunks");
    };

    assert_eq!(hunks[0].lines[2].kind, DiffLineKind::Marker);
    assert_eq!(hunks[0].lines[0].kind, DiffLineKind::Removed);
    assert_eq!(hunks[0].lines[0].old_line, Some(1));
    assert_eq!(hunks[0].lines[0].new_line, None);
    assert_eq!(hunks[0].lines[1].kind, DiffLineKind::Added);
    assert_eq!(hunks[0].lines[1].old_line, None);
    assert_eq!(hunks[0].lines[1].new_line, Some(1));
    assert_eq!(hunks[0].lines[2].content, " No newline at end of file");
    assert_eq!(hunks[0].lines[2].old_line, None);
    assert_eq!(hunks[0].lines[2].new_line, None);
}

#[test]
fn distinguishes_unavailable_empty_and_malformed_patches() {
    assert_eq!(parse_unified_diff(None), ParsedDiff::Unavailable);
    assert_eq!(parse_unified_diff(Some("")), ParsedDiff::Hunks(Vec::new()));
    assert!(matches!(
        parse_unified_diff(Some("+line before a hunk")),
        ParsedDiff::Malformed(_)
    ));
}

#[test]
fn malformed_before_hunk_header_includes_the_offending_row() {
    let row = "+line before a hunk";
    let result = parse_unified_diff(Some(row));
    let ParsedDiff::Malformed(message) = result else {
        panic!("expected Malformed, got {result:?}");
    };
    assert!(
        message.contains(row),
        "malformed message must include the offending row, got: {message}"
    );
}

#[test]
fn rejects_unrecognized_patch_row_prefixes() {
    assert!(matches!(
        parse_unified_diff(Some("@@ -1 +1 @@\n?unexpected")),
        ParsedDiff::Malformed(_)
    ));
}

#[test]
fn file_status_parser_is_total() {
    assert_eq!(PrFileStatus::from_api("added"), PrFileStatus::Added);
    assert_eq!(PrFileStatus::from_api("modified"), PrFileStatus::Modified);
    assert_eq!(PrFileStatus::from_api("changed"), PrFileStatus::Modified);
    assert_eq!(PrFileStatus::from_api("removed"), PrFileStatus::Removed);
    assert_eq!(PrFileStatus::from_api("renamed"), PrFileStatus::Renamed);
    assert_eq!(PrFileStatus::from_api("copied"), PrFileStatus::Copied);
    assert_eq!(
        PrFileStatus::from_api("future_status"),
        PrFileStatus::Unknown("future_status".to_string())
    );
}
