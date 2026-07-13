//! Tests for `edit_properties.rs` (extracted to keep that file under the
//! per-file line limit).

use super::*;

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

// ── Label diff ──────────────────────────────────────────────────────

#[test]
fn label_diff_adds_new_labels() {
    let current = vec!["bug".to_string()];
    let desired = vec!["bug".to_string(), "enhancement".to_string()];
    let (add, remove) = compute_label_diff(&current, &desired);
    assert_eq!(add, vec!["enhancement"]);
    assert!(remove.is_empty());
}

#[test]
fn label_diff_removes_dropped_labels() {
    let current = vec!["bug".to_string(), "wontfix".to_string()];
    let desired = vec!["bug".to_string()];
    let (add, remove) = compute_label_diff(&current, &desired);
    assert!(add.is_empty());
    assert_eq!(remove, vec!["wontfix"]);
}

#[test]
fn label_diff_case_insensitive_match() {
    let current = vec!["Bug".to_string()];
    let desired = vec!["bug".to_string()];
    let (add, remove) = compute_label_diff(&current, &desired);
    assert!(add.is_empty());
    assert!(remove.is_empty());
}

#[test]
fn label_diff_noop_when_identical() {
    let current = vec!["bug".to_string(), "enhancement".to_string()];
    let desired = current.clone();
    let (add, remove) = compute_label_diff(&current, &desired);
    assert!(add.is_empty());
    assert!(remove.is_empty());
}

// ── Assignee diff ───────────────────────────────────────────────────

#[test]
fn assignee_diff_add_and_remove() {
    let current = vec!["alice".to_string(), "bob".to_string()];
    let desired = vec!["bob".to_string(), "carol".to_string()];
    let (add, remove) = compute_assignee_diff(&current, &desired);
    assert_eq!(add, vec!["carol"]);
    assert_eq!(remove, vec!["alice"]);
}

// ── M8: diff function edge cases ────────────────────────────────────

#[test]
fn label_diff_identical_is_noop() {
    let current = vec!["bug".to_string(), "enhancement".to_string()];
    let desired = current.clone();
    let (add, remove) = compute_label_diff(&current, &desired);
    assert!(add.is_empty());
    assert!(remove.is_empty());
}

#[test]
fn label_diff_add_only() {
    let current = vec!["bug".to_string()];
    let desired = vec!["bug".to_string(), "enhancement".to_string()];
    let (add, remove) = compute_label_diff(&current, &desired);
    assert_eq!(add, vec!["enhancement"]);
    assert!(remove.is_empty());
}

#[test]
fn label_diff_removal_only() {
    let current = vec!["bug".to_string(), "wontfix".to_string()];
    let desired = vec!["bug".to_string()];
    let (add, remove) = compute_label_diff(&current, &desired);
    assert!(add.is_empty());
    assert_eq!(remove, vec!["wontfix"]);
}

#[test]
fn label_diff_empty_current_adds_all() {
    let current: Vec<String> = Vec::new();
    let desired = vec!["bug".to_string(), "enhancement".to_string()];
    let (add, remove) = compute_label_diff(&current, &desired);
    assert_eq!(add, vec!["bug", "enhancement"]);
    assert!(remove.is_empty());
}

// ── Arg builders ────────────────────────────────────────────────────

#[test]
fn build_edit_labels_args_issue_add_and_remove() {
    let args = build_edit_labels_args(
        PropertyEditTarget {
            owner: "owner",
            repo: "repo",
            number: 42,
            is_pr: false,
        },
        &["enhancement".to_string()],
        &["bug".to_string()],
    );
    assert_eq!(
        args,
        vec![
            "issue",
            "edit",
            "--repo",
            "owner/repo",
            "42",
            "--add-label",
            "enhancement",
            "--remove-label",
            "bug",
        ]
    );
}

#[test]
fn build_edit_labels_args_pr() {
    let args = build_edit_labels_args(
        PropertyEditTarget {
            owner: "owner",
            repo: "repo",
            number: 7,
            is_pr: true,
        },
        &["x".to_string()],
        &[],
    );
    assert_eq!(
        args,
        vec![
            "pr",
            "edit",
            "--repo",
            "owner/repo",
            "7",
            "--add-label",
            "x"
        ]
    );
}

#[test]
fn build_edit_labels_args_noop_returns_empty() {
    let args = build_edit_labels_args(
        PropertyEditTarget {
            owner: "owner",
            repo: "repo",
            number: 42,
            is_pr: false,
        },
        &[],
        &[],
    );
    assert!(args.is_empty());
}

#[test]
fn build_edit_labels_args_repeated_flags_not_comma_joined() {
    let args = build_edit_labels_args(
        PropertyEditTarget {
            owner: "o",
            repo: "r",
            number: 1,
            is_pr: false,
        },
        &["a".to_string(), "b".to_string()],
        &["c".to_string()],
    );
    assert_eq!(
        args,
        vec![
            "issue",
            "edit",
            "--repo",
            "o/r",
            "1",
            "--add-label",
            "a",
            "--add-label",
            "b",
            "--remove-label",
            "c",
        ]
    );
}

#[test]
fn build_edit_labels_args_comma_in_label_name() {
    let args = build_edit_labels_args(
        PropertyEditTarget {
            owner: "o",
            repo: "r",
            number: 1,
            is_pr: false,
        },
        &["backend,urgent".to_string()],
        &[],
    );
    assert_eq!(
        args,
        vec![
            "issue",
            "edit",
            "--repo",
            "o/r",
            "1",
            "--add-label",
            "backend,urgent",
        ]
    );
}

#[test]
fn build_edit_assignees_args_repeatable_flags() {
    let args = build_edit_assignees_args(
        PropertyEditTarget {
            owner: "o",
            repo: "r",
            number: 1,
            is_pr: false,
        },
        &["alice".to_string(), "bob".to_string()],
        &["carol".to_string()],
    );
    assert_eq!(
        args,
        vec![
            "issue",
            "edit",
            "--repo",
            "o/r",
            "1",
            "--add-assignee",
            "alice",
            "--add-assignee",
            "bob",
            "--remove-assignee",
            "carol",
        ]
    );
}

#[test]
fn build_set_milestone_args_issue() {
    let args = build_set_milestone_args("owner", "repo", 42, false, "v1.0");
    assert_eq!(
        args,
        vec![
            "issue",
            "edit",
            "--repo",
            "owner/repo",
            "42",
            "--milestone",
            "v1.0"
        ]
    );
}

#[test]
fn build_clear_milestone_args_pr() {
    let args = build_clear_milestone_args("owner", "repo", 7, true);
    assert_eq!(
        args,
        vec![
            "pr",
            "edit",
            "--repo",
            "owner/repo",
            "7",
            "--remove-milestone"
        ]
    );
}

#[test]
fn build_set_title_args_issue() {
    let args = build_set_title_args("owner", "repo", 42, false, "New Title");
    assert_eq!(
        args,
        vec![
            "issue",
            "edit",
            "--repo",
            "owner/repo",
            "42",
            "--title",
            "New Title"
        ]
    );
}

#[test]
fn build_close_args_pr() {
    let args = build_close_args("owner", "repo", 7, true);
    assert_eq!(args, vec!["pr", "close", "7", "--repo", "owner/repo"]);
}

#[test]
fn build_reopen_args_issue() {
    let args = build_reopen_args("owner", "repo", 42, false);
    assert_eq!(args, vec!["issue", "reopen", "42", "--repo", "owner/repo"]);
}

// ── Issue Type GraphQL ──────────────────────────────────────────────

#[test]
fn build_issue_types_query_args_shape() {
    let args = build_issue_types_query_args("owner", "repo");
    assert_eq!(args[0], "api");
    assert_eq!(args[1], "graphql");
    assert!(args[3].contains("issueTypes"));
    assert!(args.contains(&"-F".to_string()));
    assert!(args.contains(&"owner=owner".to_string()));
    assert!(args.contains(&"name=repo".to_string()));
}

#[test]
fn parse_issue_types_extracts_id_name_pairs() {
    let json = r#"{"data":{"repository":{"issueTypes":{"nodes":[{"id":"T_1","name":"Bug"},{"id":"T_2","name":"Feature"}]}}}}"#;
    let types = parse_issue_types(json).value_or_panic("should parse");
    assert_eq!(
        types,
        vec![
            ("T_1".to_string(), "Bug".to_string()),
            ("T_2".to_string(), "Feature".to_string())
        ]
    );
}

#[test]
fn parse_issue_types_empty_list() {
    let json = r#"{"data":{"repository":{"issueTypes":{"nodes":[]}}}}"#;
    let types = parse_issue_types(json).value_or_panic("should parse");
    assert!(types.is_empty());
}

#[test]
fn parse_issue_types_missing_nodes_errors() {
    let json = r#"{"data":{"repository":{}}}"#;
    assert!(parse_issue_types(json).is_err());
}

#[test]
fn build_issue_node_id_query_args_shape() {
    let args = build_issue_node_id_query_args("owner", "repo", 42);
    assert!(args[3].contains("issue(number:"));
    assert!(args.contains(&"number=42".to_string()));
}

#[test]
fn parse_issue_node_info_with_type() {
    let json =
        r#"{"data":{"repository":{"issue":{"id":"I_123","issueType":{"id":"T_1","name":"Bug"}}}}}"#;
    let info = parse_issue_node_info(json).value_or_panic("should parse");
    assert_eq!(info.node_id, "I_123");
    assert_eq!(info.current_type_id, Some("T_1".to_string()));
}

#[test]
fn parse_issue_node_info_without_type() {
    let json = r#"{"data":{"repository":{"issue":{"id":"I_456","issueType":null}}}}"#;
    let info = parse_issue_node_info(json).value_or_panic("should parse");
    assert_eq!(info.node_id, "I_456");
    assert_eq!(info.current_type_id, None);
}

#[test]
fn build_update_issue_type_args_set() {
    let args = build_update_issue_type_args("I_123", Some("T_1"));
    assert!(args[3].contains("$type: ID!"));
    assert!(args.contains(&"id=I_123".to_string()));
    assert!(args.contains(&"type=T_1".to_string()));
}

#[test]
fn build_update_issue_type_args_clear() {
    let args = build_update_issue_type_args("I_123", None);
    assert!(args[3].contains("issueTypeId: null"));
    assert!(args.contains(&"id=I_123".to_string()));
    assert!(!args.iter().any(|a| a.starts_with("type=")));
}

// ── Options-fetch queries ───────────────────────────────────────────

#[test]
fn parse_label_names_sorted() {
    let json = r#"{"data":{"repository":{"labels":{"nodes":[{"name":"zebra"},{"name":"apple"},{"name":"Bug"}]}}}}"#;
    let names = parse_label_names(json).value_or_panic("should parse");
    assert_eq!(names, vec!["apple", "Bug", "zebra"]);
}

#[test]
fn test_parse_milestone_titles() {
    let json =
        r#"{"data":{"repository":{"milestones":{"nodes":[{"title":"v1.0"},{"title":"v2.0"}]}}}}"#;
    let titles = parse_milestone_titles(json).value_or_panic("should parse");
    assert_eq!(titles, vec!["v1.0", "v2.0"]);
}

#[test]
fn test_parse_assignee_logins() {
    let json =
        r#"{"data":{"repository":{"assignees":{"nodes":[{"login":"alice"},{"login":"bob"}]}}}}"#;
    let logins = parse_assignee_logins(json).value_or_panic("should parse");
    assert_eq!(logins, vec!["alice", "bob"]);
}

#[test]
fn parse_label_names_missing_path_errors() {
    let json = r#"{"data":{}}"#;
    assert!(parse_label_names(json).is_err());
}

// ── F7: pagination ──────────────────────────────────────────────────

#[test]
fn parse_label_names_page_returns_cursor_when_more() {
    let json = r#"{"data":{"repository":{"labels":{"nodes":[{"name":"a"}],"pageInfo":{"hasNextPage":true,"endCursor":"Y2VyMQ=="}}}}}"#;
    let (names, next) = parse_label_names_page(json).value_or_panic("should parse");
    assert_eq!(names, vec!["a"]);
    assert_eq!(next.as_deref(), Some("Y2VyMQ=="));
}

#[test]
fn parse_label_names_page_no_cursor_when_done() {
    let json = r#"{"data":{"repository":{"labels":{"nodes":[{"name":"a"}],"pageInfo":{"hasNextPage":false,"endCursor":null}}}}}"#;
    let (names, next) = parse_label_names_page(json).value_or_panic("should parse");
    assert_eq!(names, vec!["a"]);
    assert!(next.is_none());
}

#[test]
fn parse_label_names_page_no_pageinfo_means_no_more() {
    // Backward compat: responses without pageInfo are treated as final.
    let json = r#"{"data":{"repository":{"labels":{"nodes":[{"name":"a"}]}}}}"#;
    let (names, next) = parse_label_names_page(json).value_or_panic("should parse");
    assert_eq!(names, vec!["a"]);
    assert!(next.is_none());
}

#[test]
fn build_labels_query_args_with_cursor_includes_after() {
    let args = build_labels_query_args("o", "r", Some("Y2VyMQ=="));
    assert!(args.iter().any(|a| a == "after=Y2VyMQ=="));
    assert!(args.iter().any(|a| a.contains("after: $after")));
}

#[test]
fn build_labels_query_args_without_cursor_omits_after() {
    let args = build_labels_query_args("o", "r", None);
    assert!(args.iter().all(|a| !a.starts_with("after=")));
    assert!(args.iter().all(|a| !a.contains("after: $after")));
}

#[test]
fn parse_milestone_titles_page_returns_cursor_when_more() {
    let json = r#"{"data":{"repository":{"milestones":{"nodes":[{"title":"v1"}],"pageInfo":{"hasNextPage":true,"endCursor":"Y3I="}}}}}"#;
    let (titles, next) = parse_milestone_titles_page(json).value_or_panic("should parse");
    assert_eq!(titles, vec!["v1"]);
    assert_eq!(next.as_deref(), Some("Y3I="));
}

#[test]
fn parse_assignee_logins_page_returns_cursor_when_more() {
    let json = r#"{"data":{"repository":{"assignees":{"nodes":[{"login":"alice"}],"pageInfo":{"hasNextPage":true,"endCursor":"YXNz"}}}}}"#;
    let (logins, next) = parse_assignee_logins_page(json).value_or_panic("should parse");
    assert_eq!(logins, vec!["alice"]);
    assert_eq!(next.as_deref(), Some("YXNz"));
}

// ── H2: Issue-type id/name end-to-end ───────────────────────────────

#[test]
fn issue_type_display_name_submit_id() {
    // Parse a sample GraphQL issue-types response
    let json = r#"{"data":{"repository":{"issueTypes":{"nodes":[{"id":"IT_kwDOABCD1234","name":"Bug"},{"id":"IT_kwDOABCD5678","name":"Feature"}]}}}}"#;
    let types = parse_issue_types(json).value_or_panic("should parse issue types");
    // The first element is the opaque node ID, the second is the display name.
    // Display the name, submit the id.
    let options: Vec<(String, String)> = types
        .iter()
        .map(|(id, name)| (name.clone(), id.clone()))
        .collect();
    assert_eq!(options[0].0, "Bug");
    assert_eq!(options[0].1, "IT_kwDOABCD1234");
    assert_eq!(options[1].0, "Feature");
    assert_eq!(options[1].1, "IT_kwDOABCD5678");

    // Simulate selecting "Feature" (the second option)
    let selected_name = &options[1].0;
    let selected_id = &options[1].1;

    // Generate the mutation args for setting the issue type
    let node_id = "I_12345";
    let args = build_update_issue_type_args(node_id, Some(selected_id.as_str()));

    // Assert the correct node ID is passed (not the name)
    assert!(args.contains(&format!("type={selected_id}")));
    assert!(!args.iter().any(|a| a == &format!("type={selected_name}")));
}
