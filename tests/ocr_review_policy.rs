//! Repository-level contract tests for the CI OpenCodeReview (OCR) rigor.
//!
//! These tests keep the CI OCR workflow's coverage classification,
//! reproducibility manifests, non-blocking observational role, and
//! finding-evaluation rubric aligned with the re-sync contract from
//! issue #449 (the llxprt-code OCR rigor re-sync). They mirror
//! `tests/coderabbit_policy.rs` by asserting over repository text rather
//! than runtime behavior, so CI fails mechanically if the rigor regresses.

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

/// A workflow `path:` block is an indented YAML list of artifact names under
/// the `actions/upload-artifact` step. Collect the artifact names so the
/// contract test can assert the manifest triple is uploaded.
fn upload_artifact_paths(workflow: &str) -> Vec<String> {
    let lines = workflow.lines().collect::<Vec<_>>();
    let mut paths = Vec::new();

    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if !trimmed.starts_with("path: |") {
            continue;
        }
        let block_indent = line.len() - line.trim_start().len();
        for following in lines.iter().skip(index + 1) {
            let following_trimmed = following.trim_end();
            if following_trimmed.is_empty() {
                continue;
            }
            // Indentation must be measured on the trimmed line so trailing
            // whitespace cannot inflate the count.
            let following_indent = following_trimmed.len() - following_trimmed.trim_start().len();
            if following_indent <= block_indent {
                break;
            }
            // Inside a `path: |` literal block scalar every non-empty line is
            // literal content; only comments are non-content. Do not skip lines
            // starting with '-', which would drop legitimately hyphen-leading
            // artifact paths.
            let candidate = following_trimmed.trim();
            if candidate.starts_with('#') {
                continue;
            }
            paths.push(candidate.to_string());
        }
    }
    paths
}

#[test]
fn coverage_classification_maps_completed_with_errors_to_partial() -> io::Result<()> {
    let workflow = repository_text(".github/workflows/ocr-review.yml")?;

    // The four-state coverage vocabulary must appear in the workflow so a
    // zero-exit run reporting completed_with_errors is never summarized clean.
    for term in [
        "complete_best_effort",
        "partial",
        "unknown",
        "completed_with_errors",
    ] {
        assert!(
            workflow.contains(term),
            "OCR workflow must reference coverage term {term} so a zero-exit completed_with_errors run is not summarized as clean"
        );
    }

    assert!(
        workflow.contains("coverage"),
        "OCR workflow must derive and surface a coverage classification"
    );

    Ok(())
}

#[test]
fn ocr_run_summary_never_reports_partial_or_unknown_as_clean() -> io::Result<()> {
    let workflow = repository_text(".github/workflows/ocr-review.yml")?;

    // A partial/unknown run must not collapse to the "No findings." clean
    // summary line. The workflow must gate the clean summary on coverage.
    assert!(
        workflow.contains("No findings."),
        "OCR workflow must retain the existing clean summary line so the coverage gate is observable against it"
    );
    assert!(
        workflow.contains("complete_best_effort"),
        "OCR workflow must only allow the clean summary when coverage is complete_best_effort"
    );

    Ok(())
}

#[test]
fn reproducibility_manifests_are_uploaded_as_artifacts() -> io::Result<()> {
    let workflow = repository_text(".github/workflows/ocr-review.yml")?;
    let paths = upload_artifact_paths(&workflow);

    assert!(
        !paths.is_empty(),
        "OCR workflow must upload an artifact bundle before the manifest contract can be asserted"
    );
    for manifest in ["manifest.pre.json", "manifest.post.json", "sha256.txt"] {
        assert!(
            paths.iter().any(|p| p == manifest),
            "OCR workflow must upload reproducibility artifact {manifest}"
        );
    }

    Ok(())
}

#[test]
fn manifest_provider_config_is_redacted_before_upload() -> io::Result<()> {
    let workflow = repository_text(".github/workflows/ocr-review.yml")?;

    // The redaction step must cover the manifest files so provider fields
    // written into manifest.pre.json cannot leak into the artifact bundle.
    for manifest in ["manifest.pre.json", "manifest.post.json", "sha256.txt"] {
        assert!(
            workflow.contains(manifest),
            "OCR workflow redaction must include {manifest}"
        );
    }

    Ok(())
}

#[test]
fn ocr_review_job_is_not_a_required_ci_gate() -> io::Result<()> {
    let ocr = repository_text(".github/workflows/ocr-review.yml")?;
    let ci = repository_text(".github/workflows/ci.yml")?;

    // OCR is an observational, non-blocking review signal. The OCR workflow
    // must not be wired into ci.yml's required-gate chain.
    assert!(
        !ci.contains("ocr-review") && !ci.contains("OCR Review"),
        "ci.yml must not reference the OCR workflow; OCR stays an observational signal, not a required gate"
    );

    // The OCR job itself must remain non-blocking by exit semantics: it must
    // not use continue-on-error:false gating on the review step, and the
    // workflow must declare itself observational.
    assert!(
        ocr.contains("observational") || ocr.contains("non-blocking"),
        "OCR workflow must document its observational/non-blocking role"
    );

    Ok(())
}

#[test]
fn finding_evaluation_rubric_defines_validity_disposition_and_comparison() -> io::Result<()> {
    let rubric = repository_text("dev-docs/code-review-process.md")?;

    let validity = markdown_section(&rubric, "Finding validity");
    let disposition = markdown_section(&rubric, "Finding disposition");
    let comparison = markdown_section(&rubric, "Run comparison eligibility");

    for term in ["valid", "partial", "invalid", "unverifiable"] {
        assert!(
            validity.contains(term),
            "finding-evaluation rubric must define validity term {term}"
        );
    }
    for term in ["fix", "explain", "defer", "user"] {
        assert!(
            disposition.contains(term),
            "finding-evaluation rubric must define disposition term {term}"
        );
    }
    assert!(
        !comparison.is_empty(),
        "finding-evaluation rubric must define run comparison eligibility"
    );

    Ok(())
}

#[test]
fn contributor_guide_links_the_review_process_rubric() -> io::Result<()> {
    let contributing = repository_text("CONTRIBUTING.md")?;

    assert!(
        contributing.contains("dev-docs/code-review-process.md"),
        "the contributor entry point must link to the finding-evaluation rubric"
    );

    Ok(())
}

#[test]
fn doc_reconciles_ci_connectivity_preflight_with_no_routine_tests_rule() -> io::Result<()> {
    // Issue #464 Finding 5: the CI workflow runs a bounded `ocr llm test`
    // connectivity preflight on every run, while the doc says "do not run
    // routine connectivity tests." The doc must carve out an explicit CI
    // exception so the prose no longer contradicts the workflow.
    let rubric = repository_text("dev-docs/code-review-process.md")?;
    let provider_section = markdown_section(&rubric, "Provider and remediation discipline");

    assert!(
        provider_section.contains("connectivity"),
        "provider/remediation section must address connectivity preflight to reconcile with the CI workflow"
    );
    // The carve-out must distinguish the bounded CI preflight from ordinary
    // interactive/local connectivity tests.
    assert!(
        provider_section.to_lowercase().contains("ci") || provider_section.contains("CI"),
        "provider/remediation section must explicitly carve out the CI bounded connectivity preflight"
    );

    Ok(())
}

#[test]
fn doc_comparison_eligibility_records_machine_supported_evidence() -> io::Result<()> {
    // Issue #464 Finding 3: the doc claims the manifest triple lets eligibility
    // be checked after the fact. With the pre-run manifest now recording
    // trusted-base, worktree, arg, rule, and provider evidence, the doc must
    // reflect that the comparison-eligibility check is machine-supported, not
    // merely documented.
    let rubric = repository_text("dev-docs/code-review-process.md")?;
    let comparison = markdown_section(&rubric, "Run comparison eligibility");

    // The comparison section must reference the manifest evidence that makes
    // eligibility machine-checkable.
    assert!(
        comparison.contains("worktree"),
        "comparison-eligibility section must reference worktree state evidence recorded in the manifest"
    );
    assert!(
        comparison.contains("manifest"),
        "comparison-eligibility section must reference the reproducibility manifest"
    );

    Ok(())
}
