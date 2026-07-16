//! Repository-level contract tests for CodeRabbit demand controls.
//!
//! These tests keep the root vendor configuration and the contributor-facing
//! review lifecycle aligned with Jefe's review-demand policy.

use std::{fs, io, path::Path};

fn repository_text(relative_path: &str) -> io::Result<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative_path);
    fs::read_to_string(&path).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("could not read {}: {error}", path.display()),
        )
    })
}

fn leading_spaces(line: &str) -> usize {
    line.bytes().take_while(|byte| *byte == b' ').count()
}

fn is_yaml_key(line: &str, key: &str, indent: usize) -> bool {
    if leading_spaces(line) != indent {
        return false;
    }
    line.trim()
        .strip_prefix(key)
        .and_then(|suffix| suffix.strip_prefix(':'))
        .is_some_and(|suffix| suffix.trim().is_empty() || suffix.trim_start().starts_with('#'))
}

fn yaml_section<'a>(text: &'a str, key: &str, indent: usize) -> Vec<&'a str> {
    let lines = text.lines().collect::<Vec<_>>();
    let Some(start) = lines.iter().position(|line| is_yaml_key(line, key, indent)) else {
        return Vec::new();
    };

    lines
        .into_iter()
        .skip(start + 1)
        .take_while(|line| line.trim().is_empty() || leading_spaces(line) > indent)
        .collect()
}

fn section_contains(section: &[&str], setting: &str) -> bool {
    section.iter().any(|line| line.trim() == setting)
}

fn yaml_list_scalar<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let item = line.trim().strip_prefix("- ")?;
    let value = item.strip_prefix(key)?.strip_prefix(':')?.trim();
    Some(value.trim_matches(['\'', '"']))
}

fn path_instruction<'a>(config: &'a str, path: &str) -> Vec<&'a str> {
    let lines = config.lines().collect::<Vec<_>>();
    let Some(start) = lines
        .iter()
        .position(|line| yaml_list_scalar(line, "path") == Some(path))
    else {
        return Vec::new();
    };
    let indent = leading_spaces(lines[start]);

    lines
        .into_iter()
        .skip(start + 1)
        .take_while(|line| line.trim().is_empty() || leading_spaces(line) > indent)
        .collect()
}

fn excludes_rust_scope(filter: &str) -> bool {
    let pattern = filter
        .trim()
        .trim_start_matches("- ")
        .trim_matches(['\'', '"']);
    let excluded = pattern.strip_prefix('!');

    excluded.is_some_and(|path| {
        path == "*.rs"
            || path == "**/*.rs"
            || path == "src"
            || path.starts_with("src/")
            || path == "tests"
            || path.starts_with("tests/")
            || path.contains("/src/")
            || path.contains("/tests/")
    })
}

fn markdown_section(text: &str, heading: &str) -> String {
    let header = format!("## {heading}");
    let lines = text.lines().collect::<Vec<_>>();
    let Some(start) = lines.iter().position(|line| line.trim() == header) else {
        return String::new();
    };

    lines
        .into_iter()
        .skip(start + 1)
        .take_while(|line| !line.starts_with("## "))
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn automatic_reviews_are_ready_only_and_bounded() -> io::Result<()> {
    let config = repository_text(".coderabbit.yaml")?;
    let auto_review = yaml_section(&config, "auto_review", 2);

    assert!(
        section_contains(&auto_review, "enabled: false"),
        "automatic review must require an explicit ready trigger"
    );
    assert!(
        section_contains(&auto_review, "drafts: false"),
        "draft pull requests must remain excluded"
    );
    assert!(
        section_contains(&auto_review, "auto_incremental_review: true"),
        "incremental review must remain enabled after opt-in"
    );
    assert!(
        section_contains(&auto_review, "auto_pause_after_reviewed_commits: 2"),
        "automatic incremental review must pause after two reviewed commits"
    );
    for marker in ["- \"[WIP]\"", "- \"DO NOT MERGE\"", "- \"[skip review]\""] {
        assert!(
            section_contains(&auto_review, marker),
            "ready-only review is missing title exclusion {marker}"
        );
    }
    assert!(
        section_contains(&auto_review, "- \"review-ready\""),
        "the review-ready label must trigger opt-in review"
    );
    assert!(
        section_contains(&auto_review, "- \"!wip\"")
            && section_contains(&auto_review, "- \"!do-not-review\""),
        "WIP exclusion labels must override the positive ready label"
    );

    Ok(())
}

#[test]
fn rust_scope_filter_detection_uses_complete_path_components() {
    for protected in [
        "- \"!src/**\"",
        "- \"!tests/**\"",
        "- \"!**/src/**\"",
        "- \"!**/tests/**\"",
        "- \"!**/*.rs\"",
    ] {
        assert!(
            excludes_rust_scope(protected),
            "protected Rust scope exclusion was not detected: {protected}"
        );
    }
    for unrelated in [
        "- \"!not-src/**\"",
        "- \"!tests-helper/**\"",
        "- \"!target/**\"",
    ] {
        assert!(
            !excludes_rust_scope(unrelated),
            "unrelated exclusion was misclassified as Rust scope: {unrelated}"
        );
    }
}

#[test]
fn review_scope_includes_rust_source_tests_and_jefe_workflows() -> io::Result<()> {
    let config = repository_text(".coderabbit.yaml")?;
    let path_filters = yaml_section(&config, "path_filters", 2);

    assert!(
        path_filters.is_empty(),
        "the initial demand policy must not add path filters; future filters require deliberate contract review"
    );

    let rust = path_instruction(&config, "**/*.rs").join(" ");
    let source = path_instruction(&config, "src/**/*.rs").join(" ");
    let tests = path_instruction(&config, "tests/**/*.rs").join(" ");
    let short_extension_workflows =
        path_instruction(&config, ".github/workflows/**/*.yml").join(" ");
    let long_extension_workflows =
        path_instruction(&config, ".github/workflows/**/*.yaml").join(" ");

    assert!(
        rust.contains("use Option for")
            && rust.contains("absence and Result with typed errors for fallible operations"),
        "Rust guidance must distinguish absence from fallible operations"
    );
    assert!(
        source.contains("state transitions deterministic"),
        "production guidance must preserve Jefe ownership boundaries"
    );
    assert!(
        tests.contains("changed tests and behavioral coverage"),
        "test guidance must preserve first-class review coverage"
    );
    assert!(
        short_extension_workflows.contains("untrusted pull request code")
            && long_extension_workflows.contains("untrusted pull request code"),
        "both GitHub workflow extensions must receive Jefe safety guidance"
    );

    Ok(())
}

#[test]
fn contributor_policy_defines_deliberate_review_lifecycle() -> io::Result<()> {
    let policy = repository_text("dev-docs/code-review-demand.md")?;
    let lifecycle = markdown_section(&policy, "Deliberate ready-for-review lifecycle");
    let manual_requests = markdown_section(&policy, "Manual review requests and allowance cost");
    let contributor_guide = repository_text("CONTRIBUTING.md")?;

    assert!(
        contributor_guide.contains("CodeRabbit review-demand policy"),
        "the contributor entry point must link to the demand policy"
    );
    assert!(
        lifecycle.contains("Keep the pull request in draft"),
        "active implementation must remain draft"
    );
    assert!(
        lifecycle.contains("add the `review-ready` label"),
        "ready review must use the configured explicit trigger"
    );
    assert!(
        lifecycle.contains("current head SHA"),
        "readiness and coverage must be tied to the exact head"
    );
    assert!(
        manual_requests.contains("`@coderabbitai review`")
            && manual_requests.contains("`@coderabbitai full review`"),
        "both manual review commands must be documented in the manual-request section"
    );
    assert!(
        manual_requests.contains("cost one PR review from the allowance"),
        "manual review allowance cost must be explicit"
    );
    assert!(
        manual_requests.contains("Do not request a review when the reviewed head"),
        "duplicate exact-head requests must be prohibited"
    );
    assert!(
        lifecycle.contains("Do not infer coverage from the absence of a throttle"),
        "missing throttle evidence must not imply coverage"
    );

    Ok(())
}

#[test]
fn measurement_policy_requires_reproducible_complete_windows() -> io::Result<()> {
    let policy = repository_text("dev-docs/code-review-demand.md")?;
    let events = markdown_section(&policy, "Immutable measurement events");
    let windows = markdown_section(&policy, "Complete rolling-window evaluation");

    for event_type in [
        "`review_requested`",
        "`review_completed`",
        "`review_throttled`",
        "`review_coverage_observed`",
    ] {
        assert!(
            events.contains(event_type),
            "measurement policy is missing event type {event_type}"
        );
    }
    assert!(
        events.contains("append-only"),
        "measurement events must remain immutable"
    );
    assert!(
        events.contains("resolved configuration fingerprint")
            && events.contains("eligibility snapshot"),
        "events must preserve effective configuration and eligibility evidence"
    );
    assert!(
        events.contains("terminal state is `merged` or `closed`"),
        "coverage cohort terminal states must be defined"
    );
    assert!(
        events.contains("qualifying ready/opt-in observation")
            && events.contains("remains in the cohort"),
        "terminal coverage cohort membership must survive later ready-state changes"
    );
    assert!(
        windows.contains("[T-28d, T)") && windows.contains("[T-56d, T-28d)"),
        "adjacent complete rolling windows must be explicit"
    );
    assert!(
        windows.contains("measurement cutoff `T`")
            && windows.contains("publication time `P = T + 7d`")
            && windows.contains("as-of boundary is `P`"),
        "window outcomes need explicit cutoff, publication, and as-of boundaries"
    );
    assert!(
        windows.contains("zero denominator") && windows.contains("non-comparable"),
        "undefined ratios and mixed configurations need deterministic handling"
    );
    assert!(
        windows.contains("throttle rate and exact-head review coverage"),
        "demand and coverage metrics must be evaluated together"
    );
    assert!(
        windows.contains("Never tune from a partial window"),
        "partial windows must not drive tuning"
    );

    Ok(())
}
