//! GitHub boundary contracts for pull-request changed files.

use super::{build_pr_files_api_path, parse_pr_blob_json, parse_pr_files_json};
use crate::domain::{PrFileBlob, PrFileStatus};

#[test]
fn builds_bounded_paginated_files_path() {
    assert_eq!(
        build_pr_files_api_path("owner", "repo", 376, 3, 100),
        "repos/owner/repo/pulls/376/files?per_page=100&page=3"
    );
}

#[test]
fn parses_file_status_blob_identity_previous_path_and_patch() {
    let json = r#"[{"sha":"blob","filename":"new name.rs","previous_filename":"old.rs","status":"renamed","additions":4,"deletions":2,"changes":6,"patch":"@@ -1 +1 @@\n-old\n+new"},{"sha":"oldblob","filename":"docs/old.md","status":"removed","additions":0,"deletions":2,"changes":2}]"#;
    let files = parse_pr_files_json(json).unwrap_or_else(|error| panic!("parse files: {error}"));

    assert_eq!(files.len(), 2);
    assert_eq!(files[0].blob_sha, "blob");
    assert_eq!(files[0].previous_path.as_deref(), Some("old.rs"));
    assert_eq!(files[0].status, PrFileStatus::Renamed);
    assert!(files[0].patch.is_some());
    assert_eq!(files[1].status, PrFileStatus::Removed);
    assert!(files[1].patch.is_none());
}

#[test]
fn unknown_status_and_malformed_json_degrade_explicitly() {
    let files = parse_pr_files_json(
        r#"[{"sha":"blob","filename":"future.rs","status":"moved_again","additions":0,"deletions":0,"changes":0}]"#,
    )
    .unwrap_or_else(|error| panic!("parse future status: {error}"));
    assert_eq!(
        files[0].status,
        PrFileStatus::Unknown("moved_again".to_string())
    );
    assert!(parse_pr_files_json("{}").is_err());
}

#[test]
fn parses_text_binary_and_truncated_blob_states() {
    let text = parse_pr_blob_json(r#"{"data":{"repository":{"object":{"byteSize":4,"isBinary":false,"isTruncated":false,"text":"code"}}}}"#)
        .unwrap_or_else(|error| panic!("parse text blob: {error}"));
    assert_eq!(text, PrFileBlob::Text("code".to_string()));

    let binary = parse_pr_blob_json(r#"{"data":{"repository":{"object":{"byteSize":4,"isBinary":true,"isTruncated":false,"text":null}}}}"#)
        .unwrap_or_else(|error| panic!("parse binary blob: {error}"));
    assert_eq!(binary, PrFileBlob::Binary);

    let truncated = parse_pr_blob_json(r#"{"data":{"repository":{"object":{"byteSize":2000000,"isBinary":false,"isTruncated":true,"text":null}}}}"#)
        .unwrap_or_else(|error| panic!("parse truncated blob: {error}"));
    assert_eq!(
        truncated,
        PrFileBlob::Truncated {
            byte_size: 2_000_000
        }
    );
}
