//! Checked inventory of every geometry consumer below the layout resolver.
//!
//! Issue #706 (Workbench cutover 3) makes the committed ResolvedLayout the
//! sole PTY geometry authority. The deletion sweep that finishes that cutover
//! removes ambient terminal-size reads, legacy `compute_pty_layout` family
//! derivations, the windowed fork, and fabricated `(120, 40)` size fallbacks
//! one site at a time. This file is the referee for that sweep: every
//! production occurrence of those patterns is inventoried, and the inventory
//! changes only in the commit that deliberately deletes or consciously adds a
//! site. A mismatch fails the build in both directions, so the sweep can
//! neither strand a forgotten consumer nor delete one silently.
//!
//! Scanned: every `src/**/*.rs` file. Excluded: files named `*_tests.rs` and
//! everything under `src/bin/` (harness tooling), which are not production
//! consumers. Occurrences inside `#[cfg(test)]` modules of scanned files are
//! still counted; they are part of the file's consumer surface.
//!
//! At issue #706 close this inventory is the geometry consumer manifest that
//! the owner-evidence ledger cites, so it must reconcile with the recorded
//! artifact hashes at that point.

use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
};

/// The below-resolver geometry patterns the cutover deletes.
const PATTERNS: [&str; 9] = [
    // Ambient terminal-size queries (crossterm direct, and the mouse wrapper).
    "terminal::size(",
    "terminal_size(",
    // Legacy PTY layout derivation and its overlay mirrors.
    "compute_pty_layout",
    "compute_shell_overlay_pty_layout",
    "compute_terminal_manager_pty_layout",
    // The windowed fork and its enabling environment read.
    "is_fullscreen_enabled",
    "JEFE_WINDOWED",
    "effective_render_size_for_windowed",
    // The fabricated default size a resolver-backed consumer must not need.
    "unwrap_or((120, 40))",
];

/// The recorded inventory: pattern, then file and its count of matching lines.
///
/// `src/app_shell.rs` holds the one sanctioned boundary read that feeds the
/// resolver; everything else here is a sweep target for issue #706 and its
/// successor cutovers.
const EXPECTED: &[(&str, &[(&str, usize)])] = &[
    (
        "terminal::size(",
        &[
            ("src/app_input/action_handlers.rs", 1),
            ("src/app_input/actions_orchestration.rs", 1),
            ("src/app_input/mod.rs", 2),
            ("src/app_input/prs_orchestration.rs", 1),
            ("src/app_input/shell_overlay.rs", 1),
            ("src/app_shell.rs", 1),
            ("src/app_shell_key_routing.rs", 1),
            ("src/mouse_terminal_geometry.rs", 1),
            ("src/ui/components/issue_detail.rs", 2),
            ("src/ui/components/scrollable_text.rs", 1),
            ("src/ui/screens/actions.rs", 1),
            ("src/ui/screens/errors.rs", 1),
            ("src/ui/screens/issues.rs", 1),
            ("src/ui/screens/pull_requests.rs", 1),
            ("src/ui/screens/split.rs", 1),
            ("src/ui/screens/terminal_manager.rs", 1),
        ],
    ),
    (
        "terminal_size(",
        &[
            ("src/mouse_action_execution.rs", 1),
            ("src/mouse_terminal_geometry.rs", 2),
            ("src/mouse_routing.rs", 8),
        ],
    ),
    ("compute_pty_layout", &[("src/layout.rs", 5)]),
    (
        "compute_shell_overlay_pty_layout",
        &[
            ("src/app_input/mod.rs", 1),
            ("src/app_input/shell_overlay.rs", 1),
            ("src/app_shell_terminal_geometry.rs", 1),
            ("src/layout.rs", 1),
            ("src/mouse_routing.rs", 2),
            ("src/mouse_terminal_geometry.rs", 2),
        ],
    ),
    (
        "compute_terminal_manager_pty_layout",
        &[
            ("src/app_input/shell_overlay.rs", 1),
            ("src/app_shell_terminal_geometry.rs", 1),
            ("src/layout.rs", 1),
        ],
    ),
    (
        "is_fullscreen_enabled",
        &[("src/layout.rs", 5), ("src/main.rs", 2)],
    ),
    ("JEFE_WINDOWED", &[("src/layout.rs", 1)]),
    (
        "effective_render_size_for_windowed",
        &[("src/layout.rs", 1)],
    ),
    (
        "unwrap_or((120, 40))",
        &[
            ("src/app_input/action_handlers.rs", 1),
            ("src/app_input/actions_orchestration.rs", 1),
            ("src/app_input/mod.rs", 2),
            ("src/app_input/prs_orchestration.rs", 1),
            ("src/app_input/shell_overlay.rs", 1),
            ("src/app_shell.rs", 1),
            ("src/app_shell_key_routing.rs", 1),
            ("src/mouse_terminal_geometry.rs", 1),
            ("src/ui/screens/actions.rs", 1),
            ("src/ui/screens/errors.rs", 1),
            ("src/ui/screens/issues.rs", 1),
            ("src/ui/screens/pull_requests.rs", 1),
            ("src/ui/screens/split.rs", 1),
            ("src/ui/screens/terminal_manager.rs", 1),
        ],
    ),
];

fn production_sources(directory: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name == "bin") {
                continue;
            }
            production_sources(&path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "rs")
            && !path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().ends_with("_tests.rs"))
        {
            out.push(path);
        }
    }
    Ok(())
}

fn scan(root: &Path) -> BTreeMap<&'static str, BTreeMap<String, usize>> {
    let mut sources = Vec::new();
    production_sources(&root.join("src"), &mut sources)
        .unwrap_or_else(|error| panic!("could not walk src/: {error}"));
    sources.sort();

    let mut found: BTreeMap<&'static str, BTreeMap<String, usize>> = BTreeMap::new();
    for source in &sources {
        let text = fs::read_to_string(source)
            .unwrap_or_else(|error| panic!("could not read {}: {error}", source.display()));
        let relative = source
            .strip_prefix(root)
            .unwrap_or(source)
            .to_string_lossy()
            .replace('\\', "/");
        for pattern in PATTERNS {
            let count = text.lines().filter(|line| line.contains(pattern)).count();
            if count > 0 {
                found
                    .entry(pattern)
                    .or_default()
                    .insert(relative.clone(), count);
            }
        }
    }
    found
}

fn describe(differences: Vec<String>) -> String {
    if differences.is_empty() {
        "inventory matches".to_owned()
    } else {
        differences.join("\n")
    }
}

fn compare(root: &Path) -> Vec<String> {
    let found = scan(root);
    let mut differences = Vec::new();
    for (pattern, expected_files) in EXPECTED {
        let expected: BTreeMap<&str, usize> = expected_files.iter().copied().collect();
        let actual = found.get(*pattern);
        for (file, count) in &expected {
            let seen = actual.and_then(|files| files.get(*file));
            if seen != Some(count) {
                differences.push(format!(
                    "{pattern} in {file}: expected {count} line(s), found {}",
                    seen.copied().unwrap_or(0)
                ));
            }
        }
        if let Some(files) = actual {
            for (file, count) in files {
                if !expected.contains_key(file.as_str()) {
                    differences.push(format!(
                        "{pattern} in {file}: unrecorded consumer, {count} line(s)"
                    ));
                }
            }
        } else {
            differences.push(format!("{pattern}: expected consumers, found none"));
        }
    }
    for pattern in found.keys() {
        if !EXPECTED.iter().any(|(known, _)| known == pattern) {
            differences.push(format!("{pattern}: pattern scanned but not recorded"));
        }
    }
    differences
}

#[test]
fn geometry_consumers_below_the_resolver_match_the_checked_inventory() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    assert!(
        root.join("src").is_dir(),
        "test must run from the workspace manifest"
    );
    let differences = compare(root);
    assert!(
        differences.is_empty(),
        "geometry consumer inventory drifted; delete or record the sites in the \
         same commit:\n{}",
        describe(differences)
    );
}
