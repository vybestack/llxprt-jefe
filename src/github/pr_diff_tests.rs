//! GitHub boundary contracts for pull-request changed files.

use super::{GhError, PrFilesResponse, accumulate_pr_files};
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

#[test]
fn parse_pr_blob_json_surfaces_graphql_errors_array() {
    let json = r#"{"errors":[{"message":"rate limited"}]}"#;
    let error = parse_pr_blob_json(json)
        .err()
        .unwrap_or_else(|| panic!("GraphQL errors must produce an error"));
    let message = error.to_string();
    assert!(
        message.contains("rate limited"),
        "blob parser must surface the GraphQL error message, got: {message}"
    );
}

#[test]
fn parse_pr_blob_json_graphql_errors_precedence_over_missing_data() {
    let json = r#"{"errors":[{"message":"FORBIDDEN"}]}"#;
    let error = parse_pr_blob_json(json)
        .err()
        .unwrap_or_else(|| panic!("GraphQL errors must take precedence over missing data"));
    assert!(
        error.to_string().contains("FORBIDDEN"),
        "GraphQL errors must surface before the generic missing-object error, got: {error}"
    );
}

// ── Bounded pagination accumulator contracts (issue #376 A2) ────────────────

/// Build the JSON array string for one page of `count` files, each carrying an
/// index-encoded filename so accumulation order is observable.
fn files_page_json(page: u32, count: usize) -> String {
    let entries: Vec<String> = (0..count)
        .map(|index| {
            format!(
                r#"{{"sha":"blob-p{page}-i{index}","filename":"file_p{page}_{index}.rs","status":"modified","additions":1,"deletions":1,"changes":2}}"#
            )
        })
        .collect();
    format!("[{}]", entries.join(","))
}

/// Read-only page schedule used by the accumulator tests. Each entry maps a
/// 1-based page index to either its JSON body or a typed error to surface.
enum ScheduledPage {
    Body(String),
    Error(GhError),
}

/// Test-only helper: unwrap a `Result::Ok` or panic with context.
trait TestResultExt<T> {
    fn value_or_panic(self, context: &str) -> T;
}

impl<T, E: std::fmt::Debug> TestResultExt<T> for Result<T, E> {
    fn value_or_panic(self, context: &str) -> T {
        match self {
            Ok(value) => value,
            Err(error) => panic!("{context}: {error:?}"),
        }
    }
}

/// Test-only helper: assert a `Result::Err` or panic.
fn error_or_panic<T: std::fmt::Debug, E>(result: Result<T, E>, context: &str) -> E {
    match result {
        Err(error) => error,
        Ok(value) => panic!("{context}: unexpectedly succeeded with {value:?}"),
    }
}

fn run_accumulator(per_page: u32, schedule: &[ScheduledPage]) -> Result<PrFilesResponse, GhError> {
    let mut next = 0usize;
    accumulate_pr_files(
        |_page| {
            let entry = schedule.get(next);
            next += 1;
            match entry {
                Some(ScheduledPage::Body(body)) => Ok(body.clone()),
                Some(ScheduledPage::Error(error)) => Err(clone_error(error)),
                None => Ok("[]".to_owned()),
            }
        },
        per_page,
    )
}

fn clone_error(error: &GhError) -> GhError {
    match error {
        GhError::NotAuthenticated(msg) => GhError::NotAuthenticated(msg.clone()),
        GhError::NotInstalled => GhError::NotInstalled,
        GhError::ToolResolution(msg) => GhError::ToolResolution(msg.clone()),
        GhError::RateLimited => GhError::RateLimited,
        GhError::AccessDenied(msg) => GhError::AccessDenied(msg.clone()),
        GhError::ApiError(msg) => GhError::ApiError(msg.clone()),
        GhError::ParseError(msg) => GhError::ParseError(msg.clone()),
        GhError::NetworkError(msg) => GhError::NetworkError(msg.clone()),
    }
}

#[test]
fn full_first_page_followed_by_short_page_accumulates_in_order() {
    const PER_PAGE: u32 = 2;
    let per_page_count = PER_PAGE as usize;
    let schedule = [
        ScheduledPage::Body(files_page_json(1, per_page_count)),
        // Strictly shorter second page terminates accumulation.
        ScheduledPage::Body(files_page_json(2, 1)),
    ];
    let response = run_accumulator(PER_PAGE, &schedule)
        .value_or_panic("full then short page accumulates successfully");
    assert!(
        !response.truncated,
        "short page must terminate as non-truncated"
    );
    assert_eq!(response.files.len(), per_page_count + 1);
    assert_eq!(response.files[0].path, "file_p1_0.rs");
    assert_eq!(response.files[1].path, "file_p1_1.rs");
    assert_eq!(response.files[2].path, "file_p2_0.rs");
}

#[test]
fn empty_first_page_terminates_without_truncation() {
    let schedule = [ScheduledPage::Body("[]".to_owned())];
    let response = run_accumulator(2, &schedule)
        .value_or_panic("empty first page is a successful empty response");
    assert!(!response.truncated);
    assert!(response.files.is_empty());
}

#[test]
fn short_first_page_terminates_without_truncation() {
    let schedule = [ScheduledPage::Body(files_page_json(1, 1))];
    let response =
        run_accumulator(3, &schedule).value_or_panic("short first page accumulates successfully");
    assert!(!response.truncated);
    assert_eq!(response.files.len(), 1);
}

#[test]
fn thirty_full_pages_report_truncated() {
    const PER_PAGE: u32 = 2;
    let per_page_count = PER_PAGE as usize;
    let mut schedule = Vec::with_capacity(30);
    for page in 1..=30u32 {
        schedule.push(ScheduledPage::Body(files_page_json(page, per_page_count)));
    }
    let response = run_accumulator(PER_PAGE, &schedule)
        .value_or_panic("exactly 30 full pages accumulate successfully");
    assert!(
        response.truncated,
        "30 consecutive full pages must report truncation"
    );
    assert_eq!(response.files.len(), 30 * per_page_count);
    assert_eq!(
        response.files.first().map(|file| file.path.as_str()),
        Some("file_p1_0.rs")
    );
    assert_eq!(
        response.files.last().map(|file| file.path.as_str()),
        Some("file_p30_1.rs")
    );
}

#[test]
fn error_on_later_page_returns_err_without_partial_result() {
    const PER_PAGE: u32 = 2;
    let per_page_count = PER_PAGE as usize;
    let schedule = [
        ScheduledPage::Body(files_page_json(1, per_page_count)),
        ScheduledPage::Error(GhError::NetworkError("page 2 unreachable".to_owned())),
    ];
    let result = run_accumulator(PER_PAGE, &schedule);
    let error = error_or_panic(result, "later-page error must fail the whole read");
    assert!(
        matches!(error, GhError::NetworkError(ref msg) if msg == "page 2 unreachable"),
        "the originating typed error must be surfaced unchanged"
    );
}

#[test]
fn malformed_json_on_later_page_returns_parse_err_without_partial_result() {
    const PER_PAGE: u32 = 2;
    let per_page_count = PER_PAGE as usize;
    let schedule = [
        ScheduledPage::Body(files_page_json(1, per_page_count)),
        ScheduledPage::Body("{not json}".to_owned()),
    ];
    let result = run_accumulator(PER_PAGE, &schedule);
    assert!(
        matches!(result, Err(GhError::ParseError(_))),
        "malformed later page must fail fast as a parse error"
    );
}
