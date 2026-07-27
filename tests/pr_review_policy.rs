//! Repository-level contract tests for the pr-review delivery process.
//!
//! These tests keep the pr-review process doc (`dev-docs/code-review-process.md`)
//! and its contributor entry point aligned with the process defined in issue
//! #451. They mirror the markdown-section assertions in
//! `tests/coderabbit_policy.rs` so the process cannot silently drift away from
//! its defining properties (cross-verification, traceability, tiered findings,
//! Rust guardrail confirmations) without a CI failure.

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

/// Return the body of a `## <heading>` section collapsed onto one space-
/// separated line. Stops at the next `## ` heading so nested `###` subsections
/// are included but sibling top-level sections are not. Returns an empty string
/// when the heading is absent.
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

/// Return the whole document collapsed onto one space-separated line, so a
/// phrase may span section boundaries (e.g. a preamble paragraph before the
/// first heading). Returns an empty string when the file is missing.
fn markdown_all(text: &str) -> String {
    text.lines()
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn pr_review_doc_defines_the_delivery_process() -> io::Result<()> {
    let doc = repository_text("dev-docs/code-review-process.md")?;
    let body = markdown_all(&doc);

    // The four defining properties of the pr-review process, distinguished
    // from a raw OCR dump.
    assert!(
        body.contains("OCR CLI") && body.contains("independent source analysis"),
        "the process must combine an OCR CLI pass with independent source analysis"
    );
    assert!(
        body.contains("cross-verified"),
        "findings must be cross-verified against the current source"
    );
    assert!(
        body.contains("traceability"),
        "the review must be posted under the reviewer's account for traceability"
    );
    assert!(
        body.contains("verdict"),
        "the review must deliver a holistic verdict, not just findings"
    );

    Ok(())
}

#[test]
fn pr_review_doc_defines_the_coderabbit_style_template() -> io::Result<()> {
    let doc = repository_text("dev-docs/code-review-process.md")?;
    let body = markdown_all(&doc);

    // Preamble + structure adapted from the llxprt-code exemplar.
    assert!(
        body.contains("posted under my account for traceability"),
        "the template preamble must state it is posted under the reviewer's account"
    );
    assert!(
        body.contains("Overall verdict"),
        "the template must have an Overall verdict section"
    );
    assert!(
        body.contains("## Findings"),
        "the template must have a Findings section"
    );
    assert!(
        body.contains("[Severity]"),
        "each finding must be tiered with a [Severity] label"
    );
    assert!(
        body.contains("path:") || body.contains("`path"),
        "each finding must cite a file path and line"
    );

    // Severity tiers must map to the existing CodeRabbit scheme.
    for tier in ["High", "Medium", "Low", "Nit"] {
        assert!(
            body.contains(tier),
            "the template must define the {tier} severity tier"
        );
    }

    // The review must end by distinguishing blocking items from suggestions.
    assert!(
        body.contains("blocking") && body.contains("suggestion"),
        "the template must distinguish blocking findings from suggestions"
    );

    Ok(())
}

#[test]
fn pr_review_doc_ties_findings_to_the_rust_quality_baseline() -> io::Result<()> {
    let doc = repository_text("dev-docs/code-review-process.md")?;
    let body = markdown_all(&doc);

    assert!(
        body.contains("Result") && body.contains("Option"),
        "the rubric must reference Result/Option discipline"
    );
    assert!(
        body.contains("unwrap") && body.contains("expect"),
        "the rubric must forbid unwrap/expect in production paths"
    );
    assert!(
        body.contains("unsafe"),
        "the rubric must forbid unsafe code"
    );
    assert!(
        body.contains("deterministic"),
        "the rubric must require deterministic state transitions"
    );
    assert!(
        body.contains("fail-fast"),
        "the rubric must record the fail-fast preference over defense-in-depth"
    );

    Ok(())
}

#[test]
fn pr_review_doc_defines_a_rust_guardrail_checklist() -> io::Result<()> {
    let doc = repository_text("dev-docs/code-review-process.md")?;
    let checklist = markdown_section(&doc, "Rust guardrail-confirmation checklist");

    assert!(
        !checklist.is_empty(),
        "the process doc must have a Rust guardrail-confirmation checklist section"
    );

    // Each item must tie to a mechanically-checkable repo gate.
    assert!(
        checklist.contains("cargo fmt --all --check"),
        "the checklist must require cargo fmt --all --check"
    );
    assert!(
        checklist.contains("clippy --workspace --all-targets --all-features -- -D warnings"),
        "the checklist must require the full clippy gate"
    );
    assert!(
        checklist.contains("check-clippy-allows.sh"),
        "the checklist must assert no new clippy allows via check-clippy-allows.sh"
    );
    assert!(
        checklist.contains("complexity"),
        "the checklist must forbid lint/complexity rule weakening"
    );
    assert!(
        checklist.contains("exclusion") || checklist.contains("exclude"),
        "the checklist must forbid excluding source/tests from review"
    );
    assert!(
        checklist.contains(".llxprt/"),
        "the checklist must require .llxprt/ to remain untouched"
    );

    Ok(())
}

#[test]
fn contributor_guide_links_the_pr_review_process() -> io::Result<()> {
    let contributor_guide = repository_text("CONTRIBUTING.md")?;

    assert!(
        contributor_guide.contains("code-review-process.md"),
        "the contributor entry point must link to the pr-review process doc"
    );

    Ok(())
}
