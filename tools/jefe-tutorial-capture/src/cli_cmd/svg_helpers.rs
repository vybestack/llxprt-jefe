//! SVG rendering helpers: metadata construction and single-artifact rendering.
//!
//! Extracted from `tmux_helpers.rs` to keep file sizes under the project limit.

use std::fs;
use std::path::{Path, PathBuf};

use super::cli::write_stderr;

use jefe_tutorial_capture::{
    ArtifactEntry, ArtifactKind, RunManifest, SvgRenderMetadata, render_screen_svg,
    validate_artifact_path,
};

/// Build SVG render metadata from a manifest.
fn svg_metadata_from_manifest(manifest: &RunManifest, label: &str) -> SvgRenderMetadata {
    SvgRenderMetadata {
        cols: manifest.cols,
        rows: manifest.rows,
        theme: manifest
            .theme
            .clone()
            .unwrap_or_else(|| "green-screen".to_string()),
        jefe_version: manifest.jefe_version.clone(),
        label: label.to_string(),
        scenario_hash: manifest.scenario_hash.clone(),
    }
}

/// Build color SVG render metadata from a manifest.
fn color_metadata_from_manifest(
    manifest: &RunManifest,
    label: &str,
) -> jefe_tutorial_capture::ColorSvgMetadata {
    jefe_tutorial_capture::ColorSvgMetadata {
        cols: manifest.cols,
        rows: manifest.rows,
        theme: manifest
            .theme
            .clone()
            .unwrap_or_else(|| "green-screen".to_string()),
        jefe_version: manifest.jefe_version.clone(),
        label: label.to_string(),
        scenario_hash: manifest.scenario_hash.clone(),
    }
}

/// Get the file stem from an artifact's relative path or fall back to its label.
fn artifact_file_stem(artifact: &ArtifactEntry) -> String {
    artifact
        .relative_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(&artifact.label)
        .to_string()
}

/// Render a color SVG from ANSI data if the `.ansi` file exists.
/// Returns `Some(relative_path)` if rendered so the caller can register it
/// as a ColorSvg artifact. Write errors are fatal (propagated as Err).
fn render_color_svg_if_available(
    artifact: &ArtifactEntry,
    manifest_dir: &Path,
    manifest: &RunManifest,
    svg_dir: &Path,
) -> Result<Option<PathBuf>, String> {
    let stem = artifact_file_stem(artifact);
    let ansi_name = format!("{stem}.ansi");
    let ansi_path = manifest_dir.join("artifacts").join(&ansi_name);
    let color_svg_name = format!("{stem}.color.svg");
    let color_svg_relative = PathBuf::from("svg").join(&color_svg_name);

    if !ansi_path.exists() {
        return Ok(None);
    }

    let ansi_text = fs::read_to_string(&ansi_path)
        .map_err(|e| format!("failed to read ANSI capture {}: {e}", ansi_path.display()))?;

    if let Err(err) = validate_artifact_path(&color_svg_relative) {
        return Err(format!("unsafe color SVG path: {err}"));
    }

    let ansi_lines: Vec<String> = ansi_text.lines().map(String::from).collect();
    let color_metadata = color_metadata_from_manifest(manifest, &artifact.label);
    let color_svg = jefe_tutorial_capture::render_color_svg(&ansi_lines, &color_metadata);
    let color_svg_path = svg_dir.join(&color_svg_name);
    fs::write(&color_svg_path, &color_svg).map_err(|e| {
        format!(
            "failed to write color SVG {}: {e}",
            color_svg_path.display()
        )
    })?;
    Ok(Some(color_svg_relative))
}

/// Render a single screen-capture artifact to an SVG file.
///
/// Result of rendering a single artifact: optional mono SVG and optional
/// color SVG relative paths for manifest registration.
pub struct RenderedSvgs {
    /// Monochrome SVG relative path within the artifact dir.
    pub mono_svg: Option<PathBuf>,
    /// Color SVG relative path within the artifact dir (if ANSI data existed).
    pub color_svg: Option<PathBuf>,
}

/// Render a single screen-capture artifact to SVG files (monochrome + color).
///
/// **Finding #7**: Validates the SVG output path for safety. Returns the
/// relative paths if rendered so the caller can register them in the manifest.
///
/// Returns `Ok(RenderedSvgs)` if processed (paths may be None if skipped),
/// or `Err(path)` on a write failure.
pub fn render_single_artifact(
    artifact: &ArtifactEntry,
    manifest_dir: &Path,
    manifest: &RunManifest,
    svg_dir: &Path,
) -> Result<RenderedSvgs, PathBuf> {
    if artifact.kind != ArtifactKind::ScreenCapture {
        return Ok(RenderedSvgs {
            mono_svg: None,
            color_svg: None,
        });
    }
    // Task #5: relative_path is relative to ArtifactDir; prepend "artifacts/"
    // to get the full path from the manifest dir.
    let screen_path = manifest_dir.join("artifacts").join(&artifact.relative_path);
    let text = match fs::read_to_string(&screen_path) {
        Ok(t) => t,
        Err(err) => {
            write_stderr(&format!(
                "warning: skipping {}: {err}\n",
                screen_path.display()
            ));
            return Ok(RenderedSvgs {
                mono_svg: None,
                color_svg: None,
            });
        }
    };
    let lines: Vec<String> = text.lines().map(String::from).collect();
    let metadata = svg_metadata_from_manifest(manifest, &artifact.label);
    let svg = render_screen_svg(&lines, &metadata);

    let stem = artifact_file_stem(artifact);
    let svg_name = format!("{stem}.svg");

    // Also render a color-preserving SVG from ANSI escape data if available.
    // Write errors are fatal (propagated as Err).
    let color_svg_relative =
        render_color_svg_if_available(artifact, manifest_dir, manifest, svg_dir).map_err(|e| {
            write_stderr(&format!("error: {e}\n"));
            svg_dir.join(format!("{stem}.color.svg"))
        })?;

    // Task #5: SVG relative path is relative to ArtifactDir.
    let svg_relative = PathBuf::from("svg").join(&svg_name);
    // Finding #7: validate the SVG output path for safety.
    if let Err(err) = validate_artifact_path(&svg_relative) {
        let svg_path = svg_dir.join(&svg_name);
        write_stderr(&format!("error: unsafe SVG path: {err}\n"));
        return Err(svg_path);
    }
    let svg_path = svg_dir.join(&svg_name);
    if let Err(err) = fs::write(&svg_path, &svg) {
        write_stderr(&format!("failed to write {}: {err}\n", svg_path.display()));
        return Err(svg_path);
    }
    Ok(RenderedSvgs {
        mono_svg: Some(svg_relative),
        color_svg: color_svg_relative,
    })
}
