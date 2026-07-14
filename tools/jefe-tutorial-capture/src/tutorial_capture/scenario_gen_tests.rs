//! Tests for the Tier B scenario JSON generation module.
//!
//! **Finding #5**: Verifies that scenario generation injects exact manifest
//! issue/PR titles and numbers for filter/select/assert.
//!
//! @requirement REQ-TUTORIAL-CAPTURE-004

use super::*;
use crate::manifest::{GitHubResource, GitHubResourceKind, RunId, RunManifest, RuntimeProfile};

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

impl<T> TestResultExt<T> for Option<T> {
    fn value_or_panic(self, context: &str) -> T {
        match self {
            Some(value) => value,
            None => panic!("{context}: None"),
        }
    }
}

/// Build a sample manifest with issue/PR/branch resources for scenario param tests.
fn sample_manifest_with_resources() -> RunManifest {
    let run_id = RunId::new("run-001").value_or_panic("valid run id");
    let mut manifest = RunManifest::new(
        run_id,
        "0.0.28",
        "tier-b-test",
        100,
        32,
        RuntimeProfile::Shim,
    );
    manifest.add_github_resource(GitHubResource {
        kind: GitHubResourceKind::Issue,
        repository: "fixture/test".to_string(),
        identifier: "42".to_string(),
        url: Some("https://github.com/fixture/test/issues/42".to_string()),
        title: "[tutorial-capture:run-001] fixture issue for documentation capture".to_string(),
    });
    manifest.add_github_resource(GitHubResource {
        kind: GitHubResourceKind::Branch,
        repository: "fixture/test".to_string(),
        identifier: "tutorial-capture/run-001".to_string(),
        url: None,
        title: String::new(),
    });
    manifest.add_github_resource(GitHubResource {
        kind: GitHubResourceKind::PullRequest,
        repository: "fixture/test".to_string(),
        identifier: "7".to_string(),
        url: Some("https://github.com/fixture/test/pull/7".to_string()),
        title: "[tutorial-capture:run-001] fixture pull request".to_string(),
    });
    manifest.set_fixture_github_repo("fixture/test");
    manifest
}

#[test]
fn extract_scenario_params_returns_exact_manifest_values() {
    let manifest = sample_manifest_with_resources();
    let params =
        extract_scenario_params(&manifest, "TutorialAgent").value_or_panic("should extract");
    assert_eq!(params.issue_number, "42");
    assert_eq!(params.pr_number, "7");
    assert_eq!(params.branch_name, "tutorial-capture/run-001");
    assert_eq!(params.agent_name, "TutorialAgent");
    assert!(
        params.issue_title.contains("run-001"),
        "issue title must contain run id: {}",
        params.issue_title
    );
    assert!(
        params.pr_title.contains("run-001"),
        "pr title must contain run id: {}",
        params.pr_title
    );
}

#[test]
fn extract_scenario_params_returns_none_without_resources() {
    let run_id = RunId::new("run-002").value_or_panic("valid run id");
    let manifest = RunManifest::new(run_id, "0.0.28", "test", 100, 32, RuntimeProfile::Shim);
    assert!(
        extract_scenario_params(&manifest, "Agent").is_none(),
        "should return None without resources"
    );
}

#[test]
fn generate_tier_b_scenario_injects_exact_titles_and_numbers() {
    let params = TierBScenarioParams {
        issue_title: "[tutorial-capture:run-001] fixture issue".to_string(),
        pr_title: "[tutorial-capture:run-001] fixture pull request".to_string(),
        branch_name: "tutorial-capture/run-001".to_string(),
        issue_number: "42".to_string(),
        pr_number: "7".to_string(),
        agent_name: "TutorialAgent".to_string(),
    };
    let json = generate_tier_b_scenario(&params);
    assert!(
        json.contains("[tutorial-capture:run-001] fixture issue"),
        "scenario must inject exact issue title: {json}"
    );
    assert!(
        json.contains("\"42\""),
        "scenario must inject exact issue number: {json}"
    );
    assert!(
        json.contains("[tutorial-capture:run-001] fixture pull request"),
        "scenario must inject exact PR title: {json}"
    );
    assert!(
        json.contains("\"7\""),
        "scenario must inject exact PR number: {json}"
    );
    assert!(
        json.contains("TutorialAgent"),
        "scenario must inject exact agent name: {json}"
    );
}

#[test]
fn generate_tier_b_scenario_uses_filter_and_expect_on_exact_identity() {
    let params = TierBScenarioParams {
        issue_title: "[tutorial-capture:run-001] fixture issue".to_string(),
        pr_title: "[tutorial-capture:run-001] fixture pull request".to_string(),
        branch_name: "tutorial-capture/run-001".to_string(),
        issue_number: "42".to_string(),
        pr_number: "7".to_string(),
        agent_name: "TutorialAgent".to_string(),
    };
    let json = generate_tier_b_scenario(&params);
    assert!(
        json.contains("\"type\""),
        "scenario must use type step to filter: {json}"
    );
    assert!(
        json.contains("\"waitFor\": \"[tutorial-capture:run-001] fixture issue\""),
        "scenario must waitFor exact issue title: {json}"
    );
    assert!(
        json.contains("\"expect\": \"[tutorial-capture:run-001] fixture issue\""),
        "scenario must expect exact issue title: {json}"
    );
    assert!(
        json.contains("\"waitFor\": \"42\""),
        "scenario must waitFor exact issue number: {json}"
    );
}

#[test]
fn generate_tier_b_merge_scenario_injects_exact_pr_title_and_number() {
    let params = TierBScenarioParams {
        issue_title: "[tutorial-capture:run-001] fixture issue".to_string(),
        pr_title: "[tutorial-capture:run-001] fixture pull request".to_string(),
        branch_name: "tutorial-capture/run-001".to_string(),
        issue_number: "42".to_string(),
        pr_number: "7".to_string(),
        agent_name: "TutorialAgent".to_string(),
    };
    let json = generate_tier_b_merge_scenario(&params);
    assert!(
        json.contains("\"expect\": \"[tutorial-capture:run-001] fixture pull request\""),
        "merge scenario must expect exact PR title: {json}"
    );
    assert!(
        json.contains("\"waitFor\": \"7\""),
        "merge scenario must waitFor exact PR number: {json}"
    );
    assert!(
        json.contains("\"waitFor\": \"Merged PR #7\""),
        "merge scenario must wait for the exact case-sensitive success notice: {json}"
    );
    assert!(
        !json.contains("\"waitFor\": \"merged\""),
        "generic lowercase merge markers can match neither Jefe success nor identity: {json}"
    );
}

#[test]
fn generate_tier_b_scenario_escapes_quotes_in_titles() {
    let params = TierBScenarioParams {
        issue_title: "[tutorial-capture:run-001] \"quoted\" issue".to_string(),
        pr_title: "[tutorial-capture:run-001] \"quoted\" PR".to_string(),
        branch_name: "tutorial-capture/run-001".to_string(),
        issue_number: "42".to_string(),
        pr_number: "7".to_string(),
        agent_name: "TutorialAgent".to_string(),
    };
    let json = generate_tier_b_scenario(&params);
    let parsed: serde_json::Value = serde_json::from_str(&json).value_or_panic("JSON should parse");
    assert!(parsed.is_object(), "scenario JSON must be a valid object");
    assert!(
        json.contains(r#"\"quoted\" issue"#),
        "issue-title quotes must be escaped in JSON: {json}"
    );
    assert!(
        json.contains(r#"\"quoted\" PR"#),
        "PR-title quotes must be escaped in JSON: {json}"
    );
}

// ── Finding #4: post-send Running marker + issue/PR distinction ───────

fn sample_params() -> TierBScenarioParams {
    TierBScenarioParams {
        issue_title: "[tutorial-capture:run-001] fixture issue".to_string(),
        pr_title: "[tutorial-capture:run-001] fixture pull request".to_string(),
        branch_name: "tutorial-capture/run-001".to_string(),
        issue_number: "42".to_string(),
        pr_number: "7".to_string(),
        agent_name: "TutorialAgent".to_string(),
    }
}

/// Finding #4: After sending an issue to an agent, the scenario must assert
/// a concrete `Running` marker — not just that the chooser appeared. This
/// proves the agent was actually started.
#[test]
fn generate_tier_b_scenario_asserts_running_after_issue_send() {
    let params = sample_params();
    let json = generate_tier_b_scenario(&params);
    let parsed: serde_json::Value = serde_json::from_str(&json).value_or_panic("JSON should parse");
    // Find the send-issue-to-agent macro and verify it contains "Running".
    let macros = parsed
        .get("macros")
        .and_then(|m| m.get("send-issue-to-agent"))
        .value_or_panic("send-issue-to-agent macro must exist");
    let steps = macros
        .get("steps")
        .and_then(|s| s.as_array())
        .value_or_panic("macro must have steps array");
    let has_running = steps.iter().any(|step| {
        step.get("expect")
            .and_then(|e| e.as_str())
            .is_some_and(|e| e == "Running")
    });
    assert!(
        has_running,
        "send-issue-to-agent macro must assert 'Running' after send: {json}"
    );
}

/// Finding #4: After sending a PR to an agent, the scenario must assert
/// a concrete `Running` marker.
#[test]
fn generate_tier_b_scenario_asserts_running_after_pr_send() {
    let params = sample_params();
    let json = generate_tier_b_scenario(&params);
    let parsed: serde_json::Value = serde_json::from_str(&json).value_or_panic("JSON should parse");
    let macros = parsed
        .get("macros")
        .and_then(|m| m.get("send-pr-to-agent"))
        .value_or_panic("send-pr-to-agent macro must exist");
    let steps = macros
        .get("steps")
        .and_then(|s| s.as_array())
        .value_or_panic("macro must have steps array");
    let has_running = steps.iter().any(|step| {
        step.get("expect")
            .and_then(|e| e.as_str())
            .is_some_and(|e| e == "Running")
    });
    assert!(
        has_running,
        "send-pr-to-agent macro must assert 'Running' after send: {json}"
    );
}

/// **issue #241 Tier B**: The Tier B scenario must press `/` to focus the
/// PR search input BEFORE typing the exact PR title, so the typed text
/// lands in the search box rather than being interpreted as list
/// navigation. The issue scenario already types into the issues search
/// input which is focused by default; the PR steps must explicitly focus
/// search with `/` first.
#[test]
fn generate_tier_b_scenario_pr_steps_press_slash_before_typing_pr_title() {
    let params = sample_params();
    let json = generate_tier_b_scenario(&params);
    let parsed: serde_json::Value = serde_json::from_str(&json).value_or_panic("JSON should parse");
    let steps = parsed
        .get("steps")
        .and_then(|s| s.as_array())
        .value_or_panic("scenario must have steps array");

    // Find the PR title type step and verify a `/` key step immediately
    // precedes it (possibly with a wait between).
    let pr_title_type_idx = steps
        .iter()
        .position(|s| {
            s.get("type")
                .and_then(|t| t.as_str())
                .is_some_and(|t| t.contains("fixture pull request"))
        })
        .value_or_panic("scenario must have a type step for the PR title");

    assert!(
        pr_title_type_idx > 0,
        "PR title type step must not be the first step"
    );

    // Look backwards from the type step for a `/` key step (within 3 steps).
    let lookback_start = pr_title_type_idx.saturating_sub(3);
    let has_slash_before = steps[lookback_start..pr_title_type_idx].iter().any(|s| {
        s.get("key")
            .and_then(|k| k.as_str())
            .is_some_and(|k| k == "/")
    });
    assert!(
        has_slash_before,
        "scenario must press '/' to focus PR search before typing the PR title: {json}"
    );
}

/// **issue #241 Tier B**: The Tier B merge scenario must press `/` to focus
/// the PR search input BEFORE typing the exact PR title.
#[test]
fn generate_tier_b_merge_scenario_presses_slash_before_typing_pr_title() {
    let params = sample_params();
    let json = generate_tier_b_merge_scenario(&params);
    let parsed: serde_json::Value = serde_json::from_str(&json).value_or_panic("JSON should parse");
    let steps = parsed
        .get("steps")
        .and_then(|s| s.as_array())
        .value_or_panic("scenario must have steps array");

    let pr_title_type_idx = steps
        .iter()
        .position(|s| {
            s.get("type")
                .and_then(|t| t.as_str())
                .is_some_and(|t| t.contains("fixture pull request"))
        })
        .value_or_panic("merge scenario must have a type step for the PR title");

    assert!(
        pr_title_type_idx > 0,
        "PR title type step must not be the first step"
    );

    let lookback_start = pr_title_type_idx.saturating_sub(3);
    let has_slash_before = steps[lookback_start..pr_title_type_idx].iter().any(|s| {
        s.get("key")
            .and_then(|k| k.as_str())
            .is_some_and(|k| k == "/")
    });
    assert!(
        has_slash_before,
        "merge scenario must press '/' to focus PR search before typing the PR title: {json}"
    );
}

/// Finding #4: Issue and PR sends use distinct capture labels so evidence
/// can be distinguished. Issue captures use `issue-sent-*` / `issue-send-chooser-*`;
/// PR captures use `pr-sent-*` / `pr-send-chooser-*`.
#[test]
fn generate_tier_b_scenario_distinguishes_issue_and_pr_capture_labels() {
    let params = sample_params();
    let json = generate_tier_b_scenario(&params);
    assert!(
        json.contains("issue-send-chooser-"),
        "scenario must use issue-specific chooser capture label: {json}"
    );
    assert!(
        json.contains("issue-sent-"),
        "scenario must use issue-specific sent capture label: {json}"
    );
    assert!(
        json.contains("pr-send-chooser-"),
        "scenario must use PR-specific chooser capture label: {json}"
    );
    assert!(
        json.contains("pr-sent-"),
        "scenario must use PR-specific sent capture label: {json}"
    );
}

/// Finding #4: The scenario uses distinct macros for issue vs PR sends
/// (`send-issue-to-agent` and `send-pr-to-agent`), not a single shared macro.
#[test]
fn generate_tier_b_scenario_uses_distinct_send_macros() {
    let params = sample_params();
    let json = generate_tier_b_scenario(&params);
    assert!(
        json.contains("send-issue-to-agent"),
        "scenario must define and use send-issue-to-agent macro: {json}"
    );
    assert!(
        json.contains("send-pr-to-agent"),
        "scenario must define and use send-pr-to-agent macro: {json}"
    );
    // Verify the steps reference the distinct macros.
    assert!(
        json.contains(r#""macro": "send-issue-to-agent""#),
        "issue steps must invoke send-issue-to-agent: {json}"
    );
    assert!(
        json.contains(r#""macro": "send-pr-to-agent""#),
        "PR steps must invoke send-pr-to-agent: {json}"
    );
}

// ── Finding #1: issue flow presses / then types exact title, asserts filter ──

/// Finding #1: The Tier B scenario must press `/` to focus the issue search
/// input BEFORE typing the exact issue title, so the typed text lands in the
/// search/filter box and narrows the list to the exact fixture issue.
#[test]
fn generate_tier_b_scenario_issue_steps_press_slash_before_typing_issue_title() {
    let params = sample_params();
    let json = generate_tier_b_scenario(&params);
    let parsed: serde_json::Value = serde_json::from_str(&json).value_or_panic("JSON should parse");
    let steps = parsed
        .get("steps")
        .and_then(|s| s.as_array())
        .value_or_panic("scenario must have steps array");

    // Find the issue title type step.
    let issue_title_type_idx = steps
        .iter()
        .position(|s| {
            s.get("type")
                .and_then(|t| t.as_str())
                .is_some_and(|t| t.contains("fixture issue"))
        })
        .value_or_panic("scenario must have a type step for the issue title");

    assert!(
        issue_title_type_idx > 0,
        "issue title type step must not be the first step"
    );

    // Look backwards from the type step for a `/` key step (within 3 steps).
    let lookback_start = issue_title_type_idx.saturating_sub(3);
    let has_slash_before = steps[lookback_start..issue_title_type_idx].iter().any(|s| {
        s.get("key")
            .and_then(|k| k.as_str())
            .is_some_and(|k| k == "/")
    });
    assert!(
        has_slash_before,
        "scenario must press '/' to focus issue search before typing the issue title: {json}"
    );
}

/// Finding #1: After typing the exact issue title into the search box and
/// pressing Enter (applying the filter), the scenario must `waitFor` the
/// exact issue title to appear (proving the filter narrowed the list),
/// BEFORE opening the detail.
#[test]
fn generate_tier_b_scenario_issue_filter_waits_for_exact_title_after_enter() {
    let params = sample_params();
    let json = generate_tier_b_scenario(&params);
    let parsed: serde_json::Value = serde_json::from_str(&json).value_or_panic("JSON should parse");
    let steps = parsed
        .get("steps")
        .and_then(|s| s.as_array())
        .value_or_panic("scenario must have steps array");

    // Find the first Enter key AFTER the issue title type step.
    let issue_title_type_idx = steps
        .iter()
        .position(|s| {
            s.get("type")
                .and_then(|t| t.as_str())
                .is_some_and(|t| t.contains("fixture issue"))
        })
        .value_or_panic("scenario must have a type step for the issue title");

    // After the type step, the next Enter applies the search filter, then a
    // waitFor for the exact title proves the filter narrowed the list.
    let after_type = &steps[issue_title_type_idx + 1..];
    let first_enter_idx = after_type
        .iter()
        .position(|s| {
            s.get("key")
                .and_then(|k| k.as_str())
                .is_some_and(|k| k == "Enter")
        })
        .value_or_panic("must have an Enter after typing the issue title");

    // Within 3 steps after Enter, there must be a waitFor for the exact issue title.
    let wait_window = &after_type[first_enter_idx + 1..];
    let max_check = wait_window.len().min(3);
    let has_wait_for_title = wait_window[..max_check].iter().any(|s| {
        s.get("waitFor")
            .and_then(|w| w.as_str())
            .is_some_and(|w| w.contains("fixture issue"))
    });
    assert!(
        has_wait_for_title,
        "scenario must waitFor exact issue title after Enter (assert filter narrowed): {json}"
    );
}

// ── Finding #2: capture-github fail-closed resource validation ────────

use crate::scenario_gen::TierBValidationError;

/// Finding #2: A manifest with all three resources (issue, branch, PR)
/// from the same fixture repo passes validation.
#[test]
fn validate_tier_b_resources_accepts_complete_consistent_set() {
    let manifest = sample_manifest_with_resources();
    let result = crate::validate_tier_b_resources(&manifest);
    assert!(
        result.is_ok(),
        "complete consistent resource set should pass: {result:?}"
    );
}

/// Finding #2: A manifest missing the PR resource must be refused —
/// capture-github fails closed rather than running a generic scenario.
#[test]
fn validate_tier_b_resources_refuses_missing_pr() {
    let mut manifest = sample_manifest_with_resources();
    // Remove the PR resource.
    manifest
        .github_resources
        .retain(|r| r.kind != GitHubResourceKind::PullRequest);
    let err = crate::validate_tier_b_resources(&manifest)
        .err()
        .value_or_panic("missing PR should be refused");
    assert!(
        matches!(err, TierBValidationError::MissingResource { .. }),
        "missing PR must produce MissingResource error: {err:?}"
    );
}

/// Finding #2: A manifest missing the issue resource must be refused.
#[test]
fn validate_tier_b_resources_refuses_missing_issue() {
    let mut manifest = sample_manifest_with_resources();
    manifest
        .github_resources
        .retain(|r| r.kind != GitHubResourceKind::Issue);
    let err = crate::validate_tier_b_resources(&manifest)
        .err()
        .value_or_panic("missing issue should be refused");
    assert!(
        matches!(err, TierBValidationError::MissingResource { .. }),
        "missing issue must produce MissingResource error: {err:?}"
    );
}

/// Finding #2: A manifest missing the branch resource must be refused.
#[test]
fn validate_tier_b_resources_refuses_missing_branch() {
    let mut manifest = sample_manifest_with_resources();
    manifest
        .github_resources
        .retain(|r| r.kind != GitHubResourceKind::Branch);
    let err = crate::validate_tier_b_resources(&manifest)
        .err()
        .value_or_panic("missing branch should be refused");
    assert!(
        matches!(err, TierBValidationError::MissingResource { .. }),
        "missing branch must produce MissingResource error: {err:?}"
    );
}

/// Finding #2: A manifest with no resources at all must be refused.
#[test]
fn validate_tier_b_resources_refuses_empty_resources() {
    let run_id = RunId::new("run-empty").value_or_panic("valid run id");
    let manifest = RunManifest::new(run_id, "0.0.28", "test", 100, 32, RuntimeProfile::Shim);
    let err = crate::validate_tier_b_resources(&manifest)
        .err()
        .value_or_panic("empty resources should be refused");
    assert!(
        matches!(err, TierBValidationError::MissingResource { .. }),
        "empty resources must produce MissingResource error: {err:?}"
    );
}

/// Finding #2: Resources from the wrong repository (not matching
/// fixture_github_repo) must be refused.
#[test]
fn validate_tier_b_resources_refuses_wrong_repo() {
    let mut manifest = sample_manifest_with_resources();
    manifest.set_fixture_github_repo("fixture/wrong-repo");
    let err = crate::validate_tier_b_resources(&manifest)
        .err()
        .value_or_panic("wrong repo should be refused");
    assert!(
        matches!(err, TierBValidationError::RepositoryMismatch { .. }),
        "wrong repo must produce RepositoryMismatch error: {err:?}"
    );
}

/// Finding #2: Resources from mixed repositories must be refused.
#[test]
fn validate_tier_b_resources_refuses_mixed_repos() {
    let mut manifest = sample_manifest_with_resources();
    // Change one resource's repo to something different.
    if let Some(issue) = manifest
        .github_resources
        .iter_mut()
        .find(|r| r.kind == GitHubResourceKind::Issue)
    {
        issue.repository = "fixture/other".to_string();
    }
    manifest.set_fixture_github_repo("fixture/test");
    let err = crate::validate_tier_b_resources(&manifest)
        .err()
        .value_or_panic("mixed repos should be refused");
    assert!(
        matches!(err, TierBValidationError::RepositoryMismatch { .. }),
        "mixed repos must produce RepositoryMismatch error: {err:?}"
    );
}

/// Finding #2: Duplicate resources of the same kind must be refused.
#[test]
fn validate_tier_b_resources_refuses_duplicate_issues() {
    let mut manifest = sample_manifest_with_resources();
    // Add a second issue resource.
    manifest.add_github_resource(GitHubResource {
        kind: GitHubResourceKind::Issue,
        repository: "fixture/test".to_string(),
        identifier: "99".to_string(),
        url: Some("https://github.com/fixture/test/issues/99".to_string()),
        title: String::new(),
    });
    let err = crate::validate_tier_b_resources(&manifest)
        .err()
        .value_or_panic("duplicate issue should be refused");
    assert!(
        matches!(err, TierBValidationError::DuplicateResource { .. }),
        "duplicate issue must produce DuplicateResource error: {err:?}"
    );
}

/// Finding #2: A resource with an empty identifier must be refused.
#[test]
fn validate_tier_b_resources_refuses_empty_identifier() {
    let mut manifest = sample_manifest_with_resources();
    // Set the issue identifier to empty.
    if let Some(issue) = manifest
        .github_resources
        .iter_mut()
        .find(|r| r.kind == GitHubResourceKind::Issue)
    {
        issue.identifier = String::new();
    }
    let err = crate::validate_tier_b_resources(&manifest)
        .err()
        .value_or_panic("empty identifier should be refused");
    assert!(
        matches!(err, TierBValidationError::EmptyIdentifier { .. }),
        "empty identifier must produce EmptyIdentifier error: {err:?}"
    );
}

// ── Finding #3: PR/merge after ApplySearch waitFor exact PR title ────

/// Finding #3: After applying the PR search filter (pressing Enter), the
/// scenario must `waitFor` the exact PR title to appear in the filtered list
/// BEFORE pressing Enter to open the PR detail. This proves the search
/// narrowed to the correct PR before navigation.
#[test]
fn generate_tier_b_scenario_pr_filter_waits_for_exact_title_before_detail_enter() {
    let params = sample_params();
    let json = generate_tier_b_scenario(&params);
    let parsed: serde_json::Value = serde_json::from_str(&json).value_or_panic("JSON should parse");
    let steps = parsed
        .get("steps")
        .and_then(|s| s.as_array())
        .value_or_panic("scenario must have steps array");

    // Find the PR title type step.
    let pr_type_idx = steps
        .iter()
        .position(|s| {
            s.get("type")
                .and_then(|t| t.as_str())
                .is_some_and(|t| t.contains("fixture pull request"))
        })
        .value_or_panic("must have PR title type step");

    // After the type, there should be Enter (apply filter), then waitFor for
    // the exact PR title, and THEN the detail Enter.
    let after_type = &steps[pr_type_idx + 1..];

    // First Enter applies the search filter.
    let filter_enter_idx = after_type
        .iter()
        .position(|s| {
            s.get("key")
                .and_then(|k| k.as_str())
                .is_some_and(|k| k == "Enter")
        })
        .value_or_panic("must have Enter after typing PR title (apply filter)");

    // After the filter Enter, there must be a waitFor for the exact PR title
    // BEFORE the next Enter (which opens the detail).
    let after_filter = &after_type[filter_enter_idx + 1..];
    let detail_enter_idx = after_filter
        .iter()
        .position(|s| {
            s.get("key")
                .and_then(|k| k.as_str())
                .is_some_and(|k| k == "Enter")
        })
        .value_or_panic("must have detail Enter after filter Enter");

    // Between filter Enter and detail Enter, there must be a waitFor for the
    // exact PR title.
    let between = &after_filter[..detail_enter_idx];
    let has_wait_for_pr_title = between.iter().any(|s| {
        s.get("waitFor")
            .and_then(|w| w.as_str())
            .is_some_and(|w| w.contains("fixture pull request"))
    });
    assert!(
        has_wait_for_pr_title,
        "scenario must waitFor exact PR title between filter Enter and detail Enter: {json}"
    );
}

/// Finding #3: The merge scenario must also waitFor the exact PR title
/// after the search filter Enter, before pressing Enter to open the detail.
#[test]
fn generate_tier_b_merge_scenario_pr_filter_waits_for_exact_title_before_detail_enter() {
    let params = sample_params();
    let json = generate_tier_b_merge_scenario(&params);
    let parsed: serde_json::Value = serde_json::from_str(&json).value_or_panic("JSON should parse");
    let steps = parsed
        .get("steps")
        .and_then(|s| s.as_array())
        .value_or_panic("scenario must have steps array");

    let pr_type_idx = steps
        .iter()
        .position(|s| {
            s.get("type")
                .and_then(|t| t.as_str())
                .is_some_and(|t| t.contains("fixture pull request"))
        })
        .value_or_panic("merge scenario must have PR title type step");

    let after_type = &steps[pr_type_idx + 1..];

    let filter_enter_idx = after_type
        .iter()
        .position(|s| {
            s.get("key")
                .and_then(|k| k.as_str())
                .is_some_and(|k| k == "Enter")
        })
        .value_or_panic("must have Enter after typing PR title");

    let after_filter = &after_type[filter_enter_idx + 1..];
    let detail_enter_idx = after_filter
        .iter()
        .position(|s| {
            s.get("key")
                .and_then(|k| k.as_str())
                .is_some_and(|k| k == "Enter")
        })
        .value_or_panic("must have detail Enter after filter Enter");

    let between = &after_filter[..detail_enter_idx];
    let has_wait_for_pr_title = between.iter().any(|s| {
        s.get("waitFor")
            .and_then(|w| w.as_str())
            .is_some_and(|w| w.contains("fixture pull request"))
    });
    assert!(
        has_wait_for_pr_title,
        "merge scenario must waitFor exact PR title between filter Enter and detail Enter: {json}"
    );
}

/// Finding #3: After the PR detail opens, there must be detail-specific
/// assertions: waitFor the PR number AND expect the exact PR title.
#[test]
fn generate_tier_b_scenario_pr_detail_has_specific_assertions() {
    let params = sample_params();
    let json = generate_tier_b_scenario(&params);
    let parsed: serde_json::Value = serde_json::from_str(&json).value_or_panic("JSON should parse");
    let steps = parsed
        .get("steps")
        .and_then(|s| s.as_array())
        .value_or_panic("scenario must have steps array");

    // Find the PR number waitFor step.
    let pr_number_wait_idx = steps
        .iter()
        .position(|s| {
            s.get("waitFor")
                .and_then(|w| w.as_str())
                .is_some_and(|w| w == "7")
        })
        .value_or_panic("must have waitFor for PR number");

    // After the PR number waitFor, there must be an expect for the exact
    // PR title within a few steps.
    let after_pr_number = &steps[pr_number_wait_idx..];
    let max_check = after_pr_number.len().min(5);
    let has_expect_pr_title = after_pr_number[..max_check].iter().any(|s| {
        s.get("expect")
            .and_then(|e| e.as_str())
            .is_some_and(|e| e.contains("fixture pull request"))
    });
    assert!(
        has_expect_pr_title,
        "PR detail must assert exact PR title after waitFor PR number: {json}"
    );
}

// ── Finding #5: serde_json serialization (no manual JSON escaping) ────

/// Finding #5: Generated scenario JSON must be valid and parseable by
/// serde_json. This verifies the scenario uses proper serialization rather
/// than manual string escaping.
#[test]
fn generate_tier_b_scenario_produces_valid_serde_json() {
    let params = TierBScenarioParams {
        issue_title: "[tutorial-capture:run-001] fixture issue".to_string(),
        pr_title: "[tutorial-capture:run-001] fixture pull request".to_string(),
        branch_name: "tutorial-capture/run-001".to_string(),
        issue_number: "42".to_string(),
        pr_number: "7".to_string(),
        agent_name: "TutorialAgent".to_string(),
    };
    let json = generate_tier_b_scenario(&params);
    // Must parse as valid JSON.
    let parsed: serde_json::Value =
        serde_json::from_str(&json).value_or_panic("must be valid JSON");
    assert!(parsed.is_object());
    assert!(parsed.get("steps").is_some());
    assert!(parsed.get("macros").is_some());
}

/// Finding #5: Titles with special characters are properly escaped by
/// serde_json serialization, not manual escaping.
#[test]
fn generate_tier_b_scenario_handles_special_chars_via_serde_json() {
    let params = TierBScenarioParams {
        issue_title: "[test] \"quotes\" and \\ backslash".to_string(),
        pr_title: "[test] <html> & ampersand".to_string(),
        branch_name: "tutorial-capture/run-001".to_string(),
        issue_number: "42".to_string(),
        pr_number: "7".to_string(),
        agent_name: "Agent<Test>".to_string(),
    };
    let json = generate_tier_b_scenario(&params);
    // Must parse as valid JSON despite special characters.
    let parsed: serde_json::Value =
        serde_json::from_str(&json).value_or_panic("must handle special chars via serde");
    // The exact values must be present after parsing.
    let issue_type_values: Vec<&str> = parsed
        .get("steps")
        .and_then(|s| s.as_array())
        .map(|steps| {
            steps
                .iter()
                .filter_map(|s| s.get("type").and_then(|t| t.as_str()))
                .collect()
        })
        .unwrap_or_default();
    assert!(
        issue_type_values
            .iter()
            .any(|t| t.contains("quotes") && t.contains("backslash")),
        "issue title with special chars must survive serde roundtrip: {json}"
    );
}

/// Finding #5: Merge scenario also produces valid serde_json output.
#[test]
fn generate_tier_b_merge_scenario_produces_valid_serde_json() {
    let params = sample_params();
    let json = generate_tier_b_merge_scenario(&params);
    let parsed: serde_json::Value =
        serde_json::from_str(&json).value_or_panic("merge scenario must be valid JSON");
    assert!(parsed.is_object());
    assert!(parsed.get("steps").is_some());
}

#[test]
fn merge_scenario_revalidates_exact_identity_before_irreversible_confirm() {
    let json = generate_tier_b_merge_scenario(&sample_params());
    let parsed: serde_json::Value = serde_json::from_str(&json).value_or_panic("valid JSON");
    let steps = parsed["steps"]
        .as_array()
        .value_or_panic("merge scenario steps");
    let armed_idx = steps
        .iter()
        .position(|step| step["waitFor"] == "Press Enter to confirm merge")
        .value_or_panic("armed confirmation wait");
    let final_enter_idx = steps[armed_idx + 1..]
        .iter()
        .position(|step| step["key"] == "Enter")
        .map(|idx| armed_idx + 1 + idx)
        .value_or_panic("final merge Enter");
    let identity_window = &steps[armed_idx + 1..final_enter_idx];
    assert!(
        identity_window
            .iter()
            .any(|step| { step["expect"] == "Merge Pull Request #7" })
    );
    assert!(
        identity_window
            .iter()
            .any(|step| { step["expect"] == "[tutorial-capture:run-001] fixture pull request" })
    );
}
