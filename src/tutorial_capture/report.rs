//! Markdown evidence report generation for tutorial-capture runs.
//!
//! After a run completes, the workflow produces a concise Markdown report for
//! the documentation author. The report includes setup and versions, scenario
//! outcome, actions performed, artifact index, fixture resource URLs, cleanup
//! outcome, and any discrepancies.
//!
//! ## Boundary
//!
//! This module owns report *rendering* from typed manifest data. It is pure:
//! it does not read or write files.
//!
//! @requirement REQ-TUTORIAL-CAPTURE-005

use std::fmt::Write;

use super::manifest::{
    ArtifactEntry, ArtifactKind, GitHubResource, GitHubResourceKind, RunManifest, RunOutcome,
};

/// Render a Markdown evidence report from a run manifest.
///
/// The report is evidence for writing the tutorial, not automatically
/// generated prose to publish without review. It includes:
/// - Setup and versions
/// - Scenario outcome
/// - Observed actions (keybindings/labels)
/// - Artifact index
/// - GitHub fixture resources
/// - Owned local paths
/// - Discrepancies
/// - Cleanup outcome
/// - Editorial note
///
/// @requirement REQ-TUTORIAL-CAPTURE-005
#[must_use]
pub fn render_report(manifest: &RunManifest) -> String {
    let mut report = String::new();
    let _ = writeln!(report, "# Tutorial Capture Run Report");
    report.push('\n');
    render_setup_section(&mut report, manifest);
    render_outcome_section(&mut report, manifest);
    render_actions_section(&mut report, manifest);
    render_artifact_index(&mut report, manifest);
    render_github_resources(&mut report, manifest);
    render_owned_paths(&mut report, manifest);
    render_discrepancies(&mut report, manifest);
    render_cleanup_section(&mut report, manifest);
    render_disclaimer(&mut report);
    report
}

/// Render the setup and versions section.
fn render_setup_section(report: &mut String, manifest: &RunManifest) {
    let _ = writeln!(report, "## Setup");
    report.push('\n');
    let _ = writeln!(report, "- **Run ID**: {}", manifest.run_id);
    let _ = writeln!(report, "- **Jefe version**: {}", manifest.jefe_version);
    if let Some(commit) = &manifest.git_commit {
        let _ = writeln!(report, "- **Git commit**: `{commit}`");
    }
    let _ = writeln!(report, "- **Scenario**: {}", manifest.scenario_name);
    let _ = writeln!(
        report,
        "- **Terminal geometry**: {} cols x {} rows",
        manifest.cols, manifest.rows
    );
    let _ = writeln!(
        report,
        "- **Runtime profile**: {}",
        runtime_profile_label(manifest.runtime_profile)
    );
    if let Some(repo) = &manifest.fixture_repo_path {
        let _ = writeln!(report, "- **Local fixture repo**: {}", repo.display());
    }
    if let Some(gh_repo) = &manifest.fixture_github_repo {
        let _ = writeln!(report, "- **GitHub fixture repo**: {gh_repo}");
    }
    report.push('\n');
    // issue #241 Finding #3: render tool versions table/list.
    render_tool_versions(report, manifest);
}

/// Render the tool versions table/list if `manifest.tool_versions` is present.
///
/// **issue #241 Finding #3**: The report must include a tool versions
/// table/list so reproducibility metadata is visible in the evidence report.
/// Null values (tool not found) are shown as "not found on PATH".
fn render_tool_versions(report: &mut String, manifest: &RunManifest) {
    if let Some(versions) = &manifest.tool_versions
        && let Some(map) = versions.as_object()
        && !map.is_empty()
    {
        let _ = writeln!(report, "### Tool Versions");
        report.push('\n');
        report.push_str("| Tool | Version |\n");
        report.push_str("| --- | --- |\n");
        for (tool, version) in map {
            let version_str = match version {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Null => "not found on PATH".to_string(),
                other => other.to_string(),
            };
            let _ = writeln!(
                report,
                "| {} | {} |",
                escape_table_cell(tool),
                escape_table_cell(&version_str)
            );
        }
        report.push('\n');
    }
}

/// Render the scenario outcome section.
fn render_outcome_section(report: &mut String, manifest: &RunManifest) {
    let _ = writeln!(report, "## Scenario Outcome");
    report.push('\n');
    let _ = writeln!(report, "- **Outcome**: {}", outcome_label(manifest.outcome));
    report.push('\n');
}

/// Render the observed actions section (keybindings/labels).
///
/// @requirement REQ-TUTORIAL-CAPTURE-005
fn render_actions_section(report: &mut String, manifest: &RunManifest) {
    let _ = writeln!(report, "## Observed Actions");
    report.push('\n');
    if manifest.observed_actions.is_empty() {
        report.push_str("_No actions were recorded._\n\n");
        return;
    }
    report.push_str("| Keybinding | Description | Checkpoint |\n");
    report.push_str("| --- | --- | --- |\n");
    for action in &manifest.observed_actions {
        let _ = writeln!(
            report,
            "| {} | {} | {} |",
            escape_table_cell(&action.keybinding),
            escape_table_cell(&action.description),
            escape_table_cell(action.checkpoint.as_deref().unwrap_or("—")),
        );
    }
    report.push('\n');
}

/// Render the discrepancies section.
///
/// @requirement REQ-TUTORIAL-CAPTURE-005
fn render_discrepancies(report: &mut String, manifest: &RunManifest) {
    if manifest.discrepancies.is_empty() {
        return;
    }
    let _ = writeln!(report, "## Discrepancies");
    report.push('\n');
    for discrepancy in &manifest.discrepancies {
        // Finding #12: no emoji — use plain text marker.
        // Finding #12: Markdown-escape the discrepancy text.
        let _ = writeln!(report, "- WARNING: {}", escape_markdown(discrepancy));
    }
    report.push('\n');
}

/// Render the artifact index.
fn render_artifact_index(report: &mut String, manifest: &RunManifest) {
    let _ = writeln!(report, "## Artifact Index");
    report.push('\n');
    if manifest.artifacts.is_empty() {
        report.push_str("_No artifacts were produced._\n\n");
        return;
    }
    report.push_str("| Label | Path | Kind |\n");
    report.push_str("| --- | --- | --- |\n");
    for artifact in &manifest.artifacts {
        // Task #5: relative_path is relative to ArtifactDir; prepend "artifacts/"
        // for display readability.
        let display_path = std::path::Path::new("artifacts").join(&artifact.relative_path);
        let _ = writeln!(
            report,
            "| {} | {} | {} |",
            escape_table_cell(&artifact.label),
            escape_table_cell(&display_path.display().to_string()),
            artifact_kind_label(artifact.kind)
        );
    }
    report.push('\n');
}

/// Render the GitHub fixture resources section.
fn render_github_resources(report: &mut String, manifest: &RunManifest) {
    if manifest.github_resources.is_empty() {
        return;
    }
    let _ = writeln!(report, "## GitHub Fixture Resources");
    report.push('\n');
    report.push_str("| Kind | Repository | Identifier | URL |\n");
    report.push_str("| --- | --- | --- | --- |\n");
    for resource in &manifest.github_resources {
        let _ = writeln!(
            report,
            "| {} | {} | {} | {} |",
            github_resource_kind_label(resource.kind),
            escape_table_cell(&resource.repository),
            escape_table_cell(&resource.identifier),
            escape_table_cell(resource.url.as_deref().unwrap_or("N/A")),
        );
    }
    report.push('\n');
}

/// Render the owned local paths section.
fn render_owned_paths(report: &mut String, manifest: &RunManifest) {
    if manifest.owned_paths.is_empty() {
        return;
    }
    let _ = writeln!(report, "## Owned Local Paths");
    report.push('\n');
    for path in &manifest.owned_paths {
        let _ = writeln!(
            report,
            "- [{}] {}",
            owned_path_kind_label(path.kind),
            path.path.display()
        );
    }
    report.push('\n');
}

/// Render the cleanup section.
fn render_cleanup_section(report: &mut String, manifest: &RunManifest) {
    let _ = writeln!(report, "## Cleanup");
    report.push('\n');
    if manifest.cleanup_completed {
        report.push_str("Cleanup completed. Manifest-owned resources were removed.\n");
    } else {
        report.push_str(
            "Cleanup has not yet been performed. Run `cleanup` to remove manifest-owned resources.\n",
        );
    }
    report.push('\n');
}

/// Render the editorial disclaimer.
fn render_disclaimer(report: &mut String) {
    let _ = writeln!(report, "## Editorial Note");
    report.push('\n');
    report.push_str(
        "This report is evidence for writing the tutorial, not prose to publish without review. \
         A documentation author must still choose the clearest images, write transitions, \
         remove incidental complexity, confirm accessibility/alt text, and ensure the final page \
         remains a tutorial rather than a transcript of the harness run.\n",
    );
}

/// Human-readable label for a runtime profile.
fn runtime_profile_label(profile: super::manifest::RuntimeProfile) -> &'static str {
    match profile {
        super::manifest::RuntimeProfile::Shim => "deterministic shim",
        super::manifest::RuntimeProfile::RealLlxprt => "real llxprt runtime",
        super::manifest::RuntimeProfile::RealCodePuppy => "real code-puppy runtime",
    }
}

/// Human-readable label for a run outcome.
fn outcome_label(outcome: RunOutcome) -> &'static str {
    match outcome {
        RunOutcome::Pending => "pending",
        RunOutcome::Success => "success",
        RunOutcome::Failed => "failed",
        RunOutcome::Partial => "partial",
    }
}

/// Human-readable label for an artifact kind.
fn artifact_kind_label(kind: ArtifactKind) -> &'static str {
    match kind {
        ArtifactKind::ScreenCapture => "screen capture",
        ArtifactKind::Scrollback => "scrollback",
        ArtifactKind::Report => "report",
        ArtifactKind::Manifest => "manifest",
        ArtifactKind::Visual => "monochrome SVG",
        ArtifactKind::AnsiCapture => "ANSI capture",
        ArtifactKind::ColorSvg => "color SVG",
        ArtifactKind::Scenario => "scenario",
    }
}

/// Human-readable label for a GitHub resource kind.
fn github_resource_kind_label(kind: GitHubResourceKind) -> &'static str {
    match kind {
        GitHubResourceKind::Issue => "issue",
        GitHubResourceKind::Branch => "branch",
        GitHubResourceKind::PullRequest => "pull request",
    }
}

/// Human-readable label for an owned path kind.
fn owned_path_kind_label(kind: super::manifest::OwnedPathKind) -> &'static str {
    match kind {
        super::manifest::OwnedPathKind::ConfigDir => "config dir",
        super::manifest::OwnedPathKind::FixtureRepo => "fixture repo",
        super::manifest::OwnedPathKind::FixtureClone => "fixture clone",
        super::manifest::OwnedPathKind::ArtifactDir => "artifact dir",
        super::manifest::OwnedPathKind::ShimDir => "shim dir",
    }
}

/// Escape Markdown special characters in text content so that user-provided
/// or captured text does not break the report structure or inject Markdown.
///
/// Escapes: `\`, backtick, `*`, `_`, `[`, `]`, `#`, `!`, `|`.
///
/// **Finding #12**: Report content must be Markdown-escaped to prevent
/// injection and structural breakage.
fn escape_markdown(text: &str) -> String {
    let mut result = String::with_capacity(text.len() + text.len() / 4);
    for ch in text.chars() {
        match ch {
            '\\' | '`' | '*' | '_' | '[' | ']' | '#' | '!' | '|' => {
                result.push('\\');
                result.push(ch);
            }
            _ => result.push(ch),
        }
    }
    result
}

/// Escape Markdown table cell content (escapes `|` and newlines).
fn escape_table_cell(text: &str) -> String {
    text.replace('|', "\\|").replace('\n', " ")
}

/// An artifact entry builder for test/report convenience.
impl ArtifactEntry {
    /// Create a new artifact entry.
    #[must_use]
    pub fn new(
        label: impl Into<String>,
        relative_path: impl Into<std::path::PathBuf>,
        kind: ArtifactKind,
    ) -> Self {
        Self {
            label: label.into(),
            relative_path: relative_path.into(),
            kind,
        }
    }
}

/// A GitHub resource builder for test/report convenience.
impl GitHubResource {
    /// Create a new GitHub resource record.
    #[must_use]
    pub fn new(
        kind: GitHubResourceKind,
        repository: impl Into<String>,
        identifier: impl Into<String>,
        url: Option<String>,
    ) -> Self {
        Self {
            kind,
            repository: repository.into(),
            identifier: identifier.into(),
            url,
            title: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::manifest::{
        ArtifactKind, GitHubResource, GitHubResourceKind, OwnedPathKind, RunId, RunManifest,
        RuntimeProfile,
    };
    use super::*;
    use std::path::PathBuf;

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

    fn sample_manifest() -> RunManifest {
        let mut manifest = RunManifest::new(
            RunId::new("tutorial-run-001").value_or_panic("valid id"),
            "0.0.28",
            "tutorial-capture-local",
            100,
            32,
            RuntimeProfile::Shim,
        );
        manifest.git_commit = Some("abc1234".to_string());
        manifest.add_owned_path(
            OwnedPathKind::ConfigDir,
            PathBuf::from("/tmp/jefe-tutorial/tutorial-run-001/config"),
        );
        manifest.add_artifact(ArtifactEntry::new(
            "dashboard-oriented",
            PathBuf::from("dashboard-oriented.screen.txt"),
            ArtifactKind::ScreenCapture,
        ));
        manifest.add_artifact(ArtifactEntry::new(
            "agent-running",
            PathBuf::from("agent-running.screen.txt"),
            ArtifactKind::ScreenCapture,
        ));
        manifest
    }

    // ── Report structure ──────────────────────────────────────────────────

    #[test]
    fn report_has_title() {
        let manifest = sample_manifest();
        let report = render_report(&manifest);
        assert!(report.starts_with("# Tutorial Capture Run Report"));
    }

    #[test]
    fn report_includes_run_id() {
        let manifest = sample_manifest();
        let report = render_report(&manifest);
        assert!(report.contains("tutorial-run-001"));
    }

    #[test]
    fn report_includes_jefe_version() {
        let manifest = sample_manifest();
        let report = render_report(&manifest);
        assert!(report.contains("0.0.28"));
    }

    #[test]
    fn report_includes_git_commit() {
        let manifest = sample_manifest();
        let report = render_report(&manifest);
        assert!(report.contains("abc1234"));
    }

    #[test]
    fn report_includes_scenario_name() {
        let manifest = sample_manifest();
        let report = render_report(&manifest);
        assert!(report.contains("tutorial-capture-local"));
    }

    #[test]
    fn report_includes_terminal_geometry() {
        let manifest = sample_manifest();
        let report = render_report(&manifest);
        assert!(report.contains("100 cols x 32 rows"));
    }

    #[test]
    fn report_includes_runtime_profile() {
        let manifest = sample_manifest();
        let report = render_report(&manifest);
        assert!(report.contains("deterministic shim"));
    }

    // ── Outcome ───────────────────────────────────────────────────────────

    #[test]
    fn report_includes_outcome_label() {
        let mut manifest = sample_manifest();
        manifest.set_outcome(RunOutcome::Success);
        let report = render_report(&manifest);
        assert!(report.contains("success"));
    }

    #[test]
    fn report_shows_pending_outcome_by_default() {
        let manifest = sample_manifest();
        let report = render_report(&manifest);
        assert!(report.contains("pending"));
    }

    // ── Artifact index ───────────────────────────────────────────────────

    #[test]
    fn report_includes_artifact_table_when_artifacts_exist() {
        let manifest = sample_manifest();
        let report = render_report(&manifest);
        assert!(report.contains("dashboard-oriented"));
        assert!(report.contains("agent-running"));
        assert!(report.contains("dashboard-oriented.screen.txt"));
    }

    #[test]
    fn report_shows_no_artifacts_message_when_empty() {
        let manifest = RunManifest::new(
            RunId::new("empty-run").value_or_panic("valid id"),
            "0.0.28",
            "scenario",
            80,
            24,
            RuntimeProfile::Shim,
        );
        let report = render_report(&manifest);
        assert!(report.contains("No artifacts were produced"));
    }

    // ── GitHub resources ─────────────────────────────────────────────────

    #[test]
    fn report_includes_github_resources_when_present() {
        let mut manifest = sample_manifest();
        manifest.add_github_resource(GitHubResource::new(
            GitHubResourceKind::Issue,
            "fixture/test-repo",
            "42",
            Some("https://github.com/fixture/test-repo/issues/42".to_string()),
        ));
        let report = render_report(&manifest);
        assert!(report.contains("fixture/test-repo"));
        assert!(report.contains("issues/42"));
    }

    #[test]
    fn report_omits_github_section_when_no_resources() {
        let manifest = sample_manifest();
        let report = render_report(&manifest);
        assert!(!report.contains("GitHub Fixture Resources"));
    }

    // ── Owned paths ───────────────────────────────────────────────────────

    #[test]
    fn report_includes_owned_paths_when_present() {
        let manifest = sample_manifest();
        let report = render_report(&manifest);
        assert!(report.contains("Owned Local Paths"));
        assert!(report.contains("config dir"));
    }

    // ── Cleanup ───────────────────────────────────────────────────────────

    #[test]
    fn report_shows_cleanup_not_completed_by_default() {
        let manifest = sample_manifest();
        let report = render_report(&manifest);
        assert!(report.contains("Cleanup has not yet been performed"));
    }

    #[test]
    fn report_shows_cleanup_completed_when_marked() {
        let mut manifest = sample_manifest();
        manifest.mark_cleanup_completed();
        let report = render_report(&manifest);
        assert!(report.contains("Cleanup completed"));
    }

    // ── Editorial note ───────────────────────────────────────────────────

    #[test]
    fn report_includes_editorial_note() {
        let manifest = sample_manifest();
        let report = render_report(&manifest);
        assert!(report.contains("Editorial Note"));
        assert!(report.contains("not prose to publish without review"));
    }

    // ── Observed actions ────────────────────────────────────────────────

    #[test]
    fn report_includes_observed_actions_section() {
        let mut manifest = sample_manifest();
        manifest.add_observed_action(
            "N",
            "Open New Repository form",
            Some("new-repository-form".to_string()),
        );
        manifest.add_observed_action("Enter", "Submit form", None);
        let report = render_report(&manifest);
        assert!(report.contains("Observed Actions"));
        assert!(report.contains("| N | Open New Repository form | new-repository-form |"));
        assert!(report.contains("| Enter | Submit form"));
    }

    #[test]
    fn report_shows_no_actions_message_when_empty() {
        let manifest = sample_manifest();
        let report = render_report(&manifest);
        assert!(report.contains("No actions were recorded"));
    }

    // ── Discrepancies ───────────────────────────────────────────────────

    #[test]
    fn report_includes_discrepancies_when_present() {
        let mut manifest = sample_manifest();
        manifest.add_discrepancy("Expected 'Issues' label but found 'Issue'");
        let report = render_report(&manifest);
        assert!(report.contains("Discrepancies"));
        assert!(report.contains("Expected 'Issues' label but found 'Issue'"));
    }

    /// Finding #12: discrepancies must not contain emoji.
    #[test]
    fn report_discrepancies_have_no_emoji() {
        let mut manifest = sample_manifest();
        manifest.add_discrepancy("test discrepancy");
        let report = render_report(&manifest);
        assert!(
            !report.contains("\u{26a0}"),
            "report must not contain warning emoji in discrepancies"
        );
    }

    /// Finding #12: Markdown pipe characters in content must be escaped.
    #[test]
    fn report_escapes_markdown_pipe_in_discrepancies() {
        let mut manifest = sample_manifest();
        manifest.add_discrepancy("text | with | pipes");
        let report = render_report(&manifest);
        // The pipe in the discrepancy text must be escaped as \|
        // (but the Markdown table separators are unescaped).
        assert!(
            report.contains(r"text \| with \| pipes"),
            "pipe characters in discrepancy must be escaped: check report"
        );
    }

    #[test]
    fn report_omits_discrepancies_section_when_empty() {
        let manifest = sample_manifest();
        let report = render_report(&manifest);
        assert!(!report.contains("## Discrepancies"));
    }

    // ── tool_versions setup table (issue #241 Finding #3) ───────────────

    /// **issue #241 Finding #3**: When the manifest has `tool_versions`, the
    /// report must include a tool versions table/list in the Setup section.
    #[test]
    fn report_includes_tool_versions_table_when_present() {
        let mut manifest = sample_manifest();
        manifest.tool_versions = Some(serde_json::json!({
            "tmux": "tmux 3.4",
            "git": "git version 2.43.0",
            "gh": "gh version 2.40.1"
        }));
        let report = render_report(&manifest);
        assert!(
            report.contains("Tool Versions"),
            "report must include a Tool Versions section: {report}"
        );
        assert!(
            report.contains("tmux 3.4"),
            "report must include tmux version: {report}"
        );
        assert!(
            report.contains("git version 2.43.0"),
            "report must include git version: {report}"
        );
        assert!(
            report.contains("gh version 2.40.1"),
            "report must include gh version: {report}"
        );
    }

    /// **issue #241 Finding #3**: When `tool_versions` is absent, the report
    /// omits the section (no empty table).
    #[test]
    fn report_omits_tool_versions_section_when_absent() {
        let manifest = sample_manifest();
        let report = render_report(&manifest);
        assert!(
            !report.contains("Tool Versions"),
            "report must not include Tool Versions section when absent: {report}"
        );
    }

    /// **issue #241 Finding #3**: When a tool version is null (tool not found),
    /// the report shows a placeholder rather than "null".
    #[test]
    fn report_shows_placeholder_for_null_tool_versions() {
        let mut manifest = sample_manifest();
        manifest.tool_versions = Some(serde_json::json!({
            "tmux": "tmux 3.4",
            "gh": null
        }));
        let report = render_report(&manifest);
        assert!(
            report.contains("Tool Versions"),
            "report must include Tool Versions section: {report}"
        );
        assert!(
            report.contains("tmux 3.4"),
            "report must include tmux version"
        );
        assert!(
            !report.contains("null"),
            "report must not show 'null' for missing tool: {report}"
        );
        assert!(
            report.contains("not found")
                || report.contains("unavailable")
                || report.contains("N/A"),
            "report must show a placeholder for missing tool: {report}"
        );
    }

    // ── Builder convenience ──────────────────────────────────────────────

    #[test]
    fn artifact_entry_new_builds_correctly() {
        let entry = ArtifactEntry::new(
            "test-label",
            PathBuf::from("test.txt"),
            ArtifactKind::ScreenCapture,
        );
        assert_eq!(entry.label, "test-label");
        assert_eq!(entry.relative_path, PathBuf::from("test.txt"));
    }

    #[test]
    fn github_resource_new_builds_correctly() {
        let resource = GitHubResource::new(
            GitHubResourceKind::Issue,
            "owner/repo",
            "1",
            Some("url".to_string()),
        );
        assert_eq!(resource.repository, "owner/repo");
        assert_eq!(resource.identifier, "1");
    }
}
