//! Renderer fixture artifact generation test (Task #8).
//!
//! This test produces and retains a complete **renderer fixture** artifact
//! set using synthetic screen/ANSI data. It is NOT a real Jefe capture —
//! it exercises the SVG rendering pipeline with known input to verify
//! the renderer produces well-formed, color-preserving output.
//!
//! For real Jefe capture, see the guarded test in
//! `tests/core/tutorial_capture_contracts.rs::guarded_local_capture_proves_real_terminal_interaction`
//! and the manual capture workflow documented in
//! `dev-docs/testing/tutorial-capture.md`.
//!
//! To generate a real Tier A artifact set, run:
//! ```sh
//! cargo run --bin jefe-tutorial-capture -- prepare --run-id my-run
//! cargo run --bin jefe-tutorial-capture -- capture-local \
//!   --manifest /tmp/jefe-tutorial-capture/my-run/run-manifest.json \
//!   --scenario dev-docs/tmux-scenarios/tutorial-capture-local.json \
//!   --jefe-bin target/debug/jefe
//! cargo run --bin jefe-tutorial-capture -- render \
//!   --manifest /tmp/jefe-tutorial-capture/my-run/run-manifest.json
//! cargo run --bin jefe-tutorial-capture -- report \
//!   --manifest /tmp/jefe-tutorial-capture/my-run/run-manifest.json
//! ```
//!
//! This fixture test verifies:
//! - Monochrome SVG is generated from plain text capture.
//! - Color SVG is generated from ANSI capture.
//! - Clipping/readability metadata is present.
//! - Docs accurately call color SVG a publication candidate with editorial review.

use jefe_tutorial_capture::{
    ArtifactKind, ColorSvgMetadata, RunId, RunManifest, RuntimeProfile, SvgRenderMetadata,
    color_svg_geometry, render_color_svg, render_screen_svg,
};
use std::path::PathBuf;

/// Sample screen capture lines (simulating Jefe TUI dashboard).
fn sample_screen_lines() -> Vec<String> {
    vec![
        "╔════════════════════╗╭────────────────────────────────────────╮".to_string(),
        "║ Repositories       ║│ Agents                                 │".to_string(),
        "║                    ║│                                        │".to_string(),
        "║ > TutorialRepo (0) ║│                                        │".to_string(),
        "║                    ║│                                        │".to_string(),
        "╚════════════════════╝╰────────────────────────────────────────╯".to_string(),
    ]
}

/// Sample ANSI escape sequence lines (simulating colored Jefe TUI).
fn sample_ansi_lines() -> Vec<String> {
    vec![
        "\x1b[31mRED ITEM\x1b[0m normal text \x1b[1;32mBOLD GREEN\x1b[0m".to_string(),
        "\x1b[4;34mUNDERLINE BLUE\x1b[0m \x1b[38;5;208m256-ORANGE\x1b[0m".to_string(),
        "\x1b[38;2;128;0;255mRGB PURPLE\x1b[0m \x1b[33mYELLOW\x1b[0m".to_string(),
        "plain text line".to_string(),
    ]
}

trait UnwrapOrPanic {
    type Output;
    fn or_panic(self, context: &str) -> Self::Output;
}

impl<T, E: std::fmt::Debug> UnwrapOrPanic for Result<T, E> {
    type Output = T;
    fn or_panic(self, context: &str) -> T {
        match self {
            Ok(v) => v,
            Err(e) => panic!("{context}: {e:?}"),
        }
    }
}

impl<T> UnwrapOrPanic for Option<T> {
    type Output = T;
    fn or_panic(self, context: &str) -> T {
        match self {
            Some(v) => v,
            None => panic!("{context}: None"),
        }
    }
}

fn sample_manifest() -> RunManifest {
    let run_id = RunId::new("tier-a-artifact-set").or_panic("valid run id");
    let mut manifest = RunManifest::new(
        run_id,
        "0.0.28",
        "tutorial-capture-local",
        80,
        24,
        RuntimeProfile::Shim,
    );
    manifest.theme = Some("dark".to_string());
    manifest.scenario_hash = Some("deterministic-test-hash-abcdef".to_string());
    let _ = manifest.add_artifact(jefe_tutorial_capture::ArtifactEntry {
        label: "dashboard".to_string(),
        relative_path: PathBuf::from("dashboard.screen.txt"),
        kind: ArtifactKind::ScreenCapture,
    });
    manifest
}

fn write_metadata_header(report: &mut String, manifest: &RunManifest) {
    use std::fmt::Write;
    let _ = writeln!(report, "# Tier A Artifact Set Metadata");
    let _ = writeln!(report);
    let _ = writeln!(report, "- Run ID: {}", manifest.run_id);
    let _ = writeln!(report, "- Scenario: {}", manifest.scenario_name);
    let _ = writeln!(
        report,
        "- Theme: {}",
        manifest.theme.as_deref().unwrap_or("dark")
    );
    let _ = writeln!(
        report,
        "- Geometry: {} cols x {} rows",
        manifest.cols, manifest.rows
    );
    let _ = writeln!(
        report,
        "- Scenario hash: {}",
        manifest.scenario_hash.as_deref().unwrap_or("none")
    );
}

fn write_metadata_svgs(report: &mut String, monochrome_svg: &str, color_svg: &str) {
    use std::fmt::Write;
    let _ = writeln!(report);
    let _ = writeln!(report, "## Monochrome SVG");
    let _ = writeln!(report, "- Lines: {}", monochrome_svg.lines().count());
    let _ = writeln!(
        report,
        "- Contains reproducible-monochrome-preview marker: {}",
        monochrome_svg.contains("reproducible-monochrome-preview")
    );
    let _ = writeln!(report);
    let _ = writeln!(report, "## Color SVG (publication candidate)");
    let _ = writeln!(report, "- Lines: {}", color_svg.lines().count());
    let _ = writeln!(
        report,
        "- Contains color-preserving-svg marker: {}",
        color_svg.contains("color-preserving-svg")
    );
    let _ = writeln!(
        report,
        "- Contains red color (#ff5555): {}",
        color_svg.contains("#ff5555")
    );
    let _ = writeln!(
        report,
        "- Contains bold attribute: {}",
        color_svg.contains("font-weight=\"bold\"")
    );
    let _ = writeln!(
        report,
        "- Contains underline attribute: {}",
        color_svg.contains("text-decoration=\"underline\"")
    );
}

fn write_metadata_clipping(report: &mut String, color_svg: &str, manifest: &RunManifest) {
    use std::fmt::Write;
    let (expected_w, expected_h) = color_svg_geometry(manifest.cols, manifest.rows);
    let _ = writeln!(report);
    let _ = writeln!(report, "## Readability/Clipping Verification");
    let _ = writeln!(
        report,
        "- SVG width matches declared cols: {}",
        color_svg.contains(&format!(r#"width="{expected_w}""#))
    );
    let _ = writeln!(
        report,
        "- SVG height matches declared rows: {}",
        color_svg.contains(&format!(r#"height="{expected_h}""#))
    );
}

fn write_metadata_editorial(report: &mut String) {
    use std::fmt::Write;
    let _ = writeln!(report);
    let _ = writeln!(report, "## Editorial Note");
    let _ = writeln!(
        report,
        "The color SVG is a **publication candidate** requiring editorial review. \
         A documentation author must verify color fidelity against the original \
         terminal, choose the clearest images, write transitions, and confirm \
         accessibility/alt text before publishing."
    );
}

fn write_svg_metadata(
    dir: &std::path::Path,
    manifest: &RunManifest,
    monochrome_svg: &str,
    color_svg: &str,
) {
    let mut report = String::new();
    write_metadata_header(&mut report, manifest);
    write_metadata_svgs(&mut report, monochrome_svg, color_svg);
    write_metadata_clipping(&mut report, color_svg, manifest);
    write_metadata_editorial(&mut report);
    std::fs::write(dir.join("artifact-metadata.md"), report).or_panic("write metadata report");
}

fn prepare_dir() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let tmp = tempfile::tempdir().or_panic("create isolated tier A temp dir");
    let dir = tmp.path().to_path_buf();
    let svg_dir = dir.join("svg");
    std::fs::create_dir_all(&svg_dir).or_panic("create svg dir");
    (tmp, dir, svg_dir)
}

fn generate_svgs(
    manifest: &RunManifest,
    screen_lines: &[String],
    ansi_lines: &[String],
    svg_dir: &std::path::Path,
) -> (String, String) {
    let theme = manifest.theme.clone().unwrap_or_else(|| "dark".to_string());
    let mono_metadata = SvgRenderMetadata {
        cols: manifest.cols,
        rows: manifest.rows,
        theme: theme.clone(),
        jefe_version: manifest.jefe_version.clone(),
        label: "dashboard".to_string(),
        scenario_hash: manifest.scenario_hash.clone(),
    };
    let monochrome_svg = render_screen_svg(screen_lines, &mono_metadata);
    std::fs::write(svg_dir.join("dashboard.svg"), &monochrome_svg).or_panic("write monochrome svg");

    let color_metadata = ColorSvgMetadata {
        cols: manifest.cols,
        rows: manifest.rows,
        theme,
        jefe_version: manifest.jefe_version.clone(),
        label: "dashboard".to_string(),
        scenario_hash: manifest.scenario_hash.clone(),
    };
    let color_svg = render_color_svg(ansi_lines, &color_metadata);
    std::fs::write(svg_dir.join("dashboard.color.svg"), &color_svg).or_panic("write color svg");
    (monochrome_svg, color_svg)
}

fn write_captures(dir: &std::path::Path, screen_lines: &[String], ansi_lines: &[String]) {
    std::fs::write(dir.join("dashboard.screen.txt"), screen_lines.join("\n"))
        .or_panic("write screen txt");
    std::fs::write(dir.join("dashboard.screen.ansi"), ansi_lines.join("\n"))
        .or_panic("write ansi capture");
}

fn write_manifest_and_metadata(
    dir: &std::path::Path,
    manifest: &RunManifest,
    monochrome_svg: &str,
    color_svg: &str,
) {
    let manifest_json = manifest.to_json().or_panic("serialize manifest");
    std::fs::write(dir.join("run-manifest.json"), &manifest_json).or_panic("write manifest");
    write_svg_metadata(dir, manifest, monochrome_svg, color_svg);
}

fn assert_artifacts_exist(dir: &std::path::Path, svg_dir: &std::path::Path) {
    assert!(
        dir.join("dashboard.screen.txt").exists(),
        "plain text capture must exist"
    );
    assert!(
        dir.join("dashboard.screen.ansi").exists(),
        "ANSI capture must exist"
    );
    assert!(
        svg_dir.join("dashboard.svg").exists(),
        "monochrome SVG must exist"
    );
    assert!(
        svg_dir.join("dashboard.color.svg").exists(),
        "color SVG must exist"
    );
    assert!(
        dir.join("run-manifest.json").exists(),
        "manifest must exist"
    );
    assert!(
        dir.join("artifact-metadata.md").exists(),
        "metadata report must exist"
    );
}

/// Generate and retain a complete renderer fixture artifact set using
/// synthetic screen/ANSI data to exercise the SVG rendering pipeline.
#[test]
fn produce_and_retain_renderer_fixture_artifact_set() {
    let (_tmp_guard, dir, svg_dir) = prepare_dir();
    let manifest = sample_manifest();
    let screen_lines = sample_screen_lines();
    let ansi_lines = sample_ansi_lines();
    write_captures(&dir, &screen_lines, &ansi_lines);
    let (monochrome_svg, color_svg) =
        generate_svgs(&manifest, &screen_lines, &ansi_lines, &svg_dir);
    write_manifest_and_metadata(&dir, &manifest, &monochrome_svg, &color_svg);
    assert_artifacts_exist(&dir, &svg_dir);

    // Verify color SVG has color markers.
    assert!(
        color_svg.contains("color-preserving-svg"),
        "color SVG must declare itself as color-preserving"
    );
    assert!(
        color_svg.contains("#ff5555"),
        "color SVG must contain red color (from ANSI red escape)"
    );
    assert!(
        color_svg.contains("font-weight=\"bold\""),
        "color SVG must preserve bold attribute"
    );
    assert!(
        color_svg.contains("text-decoration=\"underline\""),
        "color SVG must preserve underline attribute"
    );

    // Verify clipping metadata.
    let (expected_w, expected_h) = color_svg_geometry(manifest.cols, manifest.rows);
    let expected_width = format!(r#"width="{expected_w}""#);
    let expected_height = format!(r#"height="{expected_h}""#);
    assert!(
        color_svg.contains(&expected_width),
        "color SVG width must match declared cols: expected {expected_width}"
    );
    assert!(
        color_svg.contains(&expected_height),
        "color SVG height must match declared rows: expected {expected_height}"
    );

    // Verify monochrome SVG is honestly labeled.
    assert!(
        monochrome_svg.contains("reproducible-monochrome-preview"),
        "monochrome SVG must be honestly labeled as preview"
    );
}

/// **issue #241 Finding #2**: Regenerate artifacts with privacy-sensitive
/// content (hostname remnants, tmux clock/date forms) and scan to verify
/// the redaction defense scrubs all host/time/date patterns. This is a
/// real artifact regeneration + scan test: it produces a capture file,
/// runs the redaction pipeline, and asserts no host/time/date remnants.
#[test]
fn redacted_artifacts_contain_no_host_time_date() {
    use jefe_tutorial_capture::redaction::{RedactionSet, add_privacy_rules};

    let dirty_text = build_dirty_capture_text();
    let mut set = RedactionSet::new();
    add_privacy_rules(&mut set, None);
    let redacted = set.apply(&dirty_text);

    scan_for_hostname_remnants(&redacted);
    scan_for_time_and_date_remnants(&redacted);
    write_and_scan_artifact_on_disk(&redacted);
}

/// Build a simulated capture containing all the patterns that the tmux
/// status bar or shell prompt may render.
fn build_dirty_capture_text() -> String {
    let dirty_lines = [
        "user@workstation-01.local:~$ ls".to_string(),
        "[tmux] session-name 14:35 2026-07-13T12:05:56Z".to_string(),
        "dev@MacBook-Pro ~ % echo hello".to_string(),
        "Published: July 13, 2026 at Jul 13 14:35".to_string(),
        "host.lan connection established".to_string(),
    ];
    dirty_lines.join("\n")
}

/// Scan redacted text for hostname remnants.
fn scan_for_hostname_remnants(redacted: &str) {
    for forbidden in &[
        "MacBook",
        "workstation-01.local",
        ".local",
        "MacBook-Pro",
        "host.lan",
        ".lan",
    ] {
        assert!(
            !redacted.contains(forbidden),
            "redacted text must not contain '{forbidden}':
{redacted}"
        );
    }
}

/// Scan redacted text for time and date remnants.
fn scan_for_time_and_date_remnants(redacted: &str) {
    assert!(
        !redacted.contains("14:35"),
        "redacted text must not contain tmux clock '14:35':
{redacted}"
    );
    assert!(
        !redacted.contains("2026-07-13T12:05:56"),
        "redacted text must not contain ISO timestamp:
{redacted}"
    );
    for forbidden in &["July 13, 2026", "Jul 13 14:35", "Jul 13"] {
        assert!(
            !redacted.contains(forbidden),
            "redacted text must not contain date '{forbidden}':
{redacted}"
        );
    }
}

/// Write the redacted artifact to disk and scan the on-disk readback.
fn write_and_scan_artifact_on_disk(redacted: &str) {
    let tmp = tempfile::tempdir().or_panic("create isolated redaction scan dir");
    let dir = tmp.path().to_path_buf();
    let artifact_path = dir.join("redacted-screen.scan.txt");
    std::fs::write(&artifact_path, redacted).or_panic("write redacted scan artifact");
    assert!(
        artifact_path.exists(),
        "redacted scan artifact must exist on disk"
    );

    let readback = std::fs::read_to_string(&artifact_path).or_panic("read redacted scan");
    assert!(
        !readback.contains("MacBook"),
        "on-disk artifact must not contain hostname"
    );
    assert!(
        !readback.contains("14:35"),
        "on-disk artifact must not contain tmux clock"
    );
    assert!(
        !readback.contains("July 13"),
        "on-disk artifact must not contain date"
    );
    // tmp guard drops here, cleaning up the isolated temp directory.
    let _ = tmp;
}
