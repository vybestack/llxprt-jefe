//! Real-runtime validation scenario generation and evidence recording.
//!
//! **Finding #2**: `validate-runtime` launches the real Jefe TUI via tmux
//! with a curated PATH containing only the selected real runtime, then
//! drives a minimal scenario that opens New Agent (key `n`) to trigger the
//! runtime chooser and asserts on the runtime chooser identity. The scenario
//! asserts on the runtime label text as displayed by the Jefe form
//! ("LLxprt" or "code_puppy"), not just a generic title, and records
//! semantic evidence in the manifest.
//!
//! ## Boundary
//!
//! This module owns the *scenario JSON generation* for validate-runtime.
//! The actual tmux launch and scenario execution are performed by the
//! harness runner and the CLI `tmux_helpers` layer.
//!
//! @requirement REQ-TUTORIAL-CAPTURE-003

use std::path::Path;

use super::RunManifest;
use super::manifest::{ArtifactKind, RuntimeProfile};
use super::persistence::{self, PersistenceError};

/// The expected binary name for a given runtime profile.
///
/// **Finding #7**: Tests assert on the actual runtime binary name text that
/// appears in Jefe's runtime chooser, not just a generic title.
#[must_use]
pub fn runtime_binary_name(profile: RuntimeProfile) -> Option<&'static str> {
    match profile {
        RuntimeProfile::RealLlxprt => Some("llxprt"),
        RuntimeProfile::RealCodePuppy => Some("code-puppy"),
        RuntimeProfile::Shim => None,
    }
}

/// The expected runtime label for a given runtime profile — the text shown
/// in the Jefe "Agent Runtime" selector.
///
/// **Finding #2**: The Jefe form renders `fields.agent_kind` which stores the
/// label (`LLxprt` for llxprt, `code_puppy` for code-puppy). The scenario
/// must assert on this exact label text.
#[must_use]
pub fn runtime_label(profile: RuntimeProfile) -> Option<&'static str> {
    match profile {
        RuntimeProfile::RealLlxprt => Some("LLxprt"),
        RuntimeProfile::RealCodePuppy => Some("code_puppy"),
        RuntimeProfile::Shim => None,
    }
}

/// Generate a validate-runtime scenario JSON for the given runtime profile.
///
/// The scenario:
/// 1. Waits for the Jefe dashboard to appear.
/// 2. Creates a repository (key `N` for New Repository) so lowercase `n`
///    maps to New Agent (Jefe routes `n` to New Repository when no repo
///    exists, and to New Agent when a repo is already selected).
/// 3. Types the fixture repo name, Tabs to path, enters the path, and
///    presses Enter to create the repo.
/// 4. Opens New Agent (key `n`).
/// 5. Waits for "New Agent" and "Agent Runtime" to appear.
/// 6. Asserts the runtime chooser shows the expected runtime label.
/// 7. Captures the chooser state as semantic evidence.
/// 8. Exits without starting an agent (proves detection without launching).
///
/// **Finding #2**: The scenario opens New Agent (lowercase `n`) after
/// creating a repo. It proves Jefe detection by asserting the runtime label
/// text in the chooser, not by starting an agent.
///
/// **Finding #7**: The scenario asserts on the actual runtime label
/// (`LLxprt` or `code_puppy`), not just a generic "runtime" title.
///
/// @requirement REQ-TUTORIAL-CAPTURE-003
#[must_use]
pub fn generate_validate_runtime_scenario(profile: RuntimeProfile) -> Option<String> {
    let label = runtime_label(profile)?;
    let opposite = opposite_runtime_label(profile);

    // Build the scenario as structured JSON so escaping is handled by
    // serde_json rather than a hand-rolled escape function (Finding #3).
    //
    // **Finding #2**: The scenario asserts on the full "Agent Runtime" row
    // text (not just the runtime label), asserts the selected runtime's
    // exact label is present, AND asserts the opposite runtime's label
    // is absent via `waitForNot` (the supported harness step that blocks
    // until a pattern no longer appears on screen). This proves Jefe
    // detected ONLY the selected runtime in the chooser. Only visible UI
    // labels are used — binary names are internal and never appear in
    // the TUI form.
    let scenario = serde_json::json!({
        "config": {
            "cols": 100,
            "rows": 32,
            "history_limit": 2000,
            "initial_wait_ms": 500,
            "assert_mode": "strict"
        },
        "macros": {
            "quit": {
                "params": [],
                "steps": [
                    { "key": "Escape" },
                    { "wait": 200 },
                    { "key": "C-q" },
                    { "waitForExit": 3000 }
                ]
            }
        },
        "steps": [
            { "waitFor": "LLxprt Jefe" },
            { "capture": "validate-dashboard" },

            { "key": "N" },
            { "waitFor": "New Repository" },
            { "capture": "validate-new-repo-form" },

            { "type": "ValidateRepo" },
            { "key": "Tab" },
            { "type": "." },
            { "key": "Enter" },
            { "wait": 300 },

            { "key": "n" },
            { "waitFor": "New Agent" },
            { "waitFor": "Agent Runtime" },
            { "capture": "validate-new-agent-form" },

            // Finding #2: assert the full "Agent Runtime" label row is visible
            // (proves the chooser rendered the runtime selector row).
            { "expect": "Agent Runtime" },
            // Finding #2: assert the exact selected runtime label is present.
            { "expect": label },
            // Finding #2: assert the opposite runtime's label is absent.
            // waitForNot is the supported harness step that blocks until a
            // pattern no longer appears on screen.
            { "waitForNot": opposite },
            { "capture": "validate-runtime-chooser" },

            { "macro": "quit", "args": {} }
        ]
    });

    // Pretty-print with 2-space indentation to match the previous format.
    serde_json::to_string_pretty(&scenario).ok()
}

/// The opposite runtime's label for absence assertion.
///
/// **Finding #2**: Used to verify the opposite runtime is NOT shown in the
/// chooser when only one runtime is available.
#[must_use]
fn opposite_runtime_label(profile: RuntimeProfile) -> &'static str {
    match profile {
        RuntimeProfile::RealLlxprt => "code_puppy",
        RuntimeProfile::RealCodePuppy => "LLxprt",
        RuntimeProfile::Shim => "",
    }
}

/// Prepare the validate-runtime scenario atomically under
/// `artifacts/scenarios/` and register it in the manifest as
/// `ArtifactKind::Scenario`.
///
/// **Finding #1**: The scenario is persisted atomically (temp-file + rename)
/// under `artifacts/scenarios/validate-runtime-scenario.json`, registered in
/// the manifest with `ArtifactKind::Scenario`, and the manifest is saved
/// **before** the tmux launch begins. This ensures the scenario artifact is
/// durable and tracked even if the tmux run fails.
///
/// @requirement REQ-TUTORIAL-CAPTURE-003
///
/// # Errors
///
/// Returns [`PersistenceError`] if the scenario cannot be generated,
/// written, or the manifest cannot be persisted.
pub fn prepare_validate_runtime_scenario(
    manifest: &mut RunManifest,
    artifact_dir: &Path,
    run_root: &Path,
) -> Result<std::path::PathBuf, PersistenceError> {
    let profile = manifest.runtime_profile;
    let Some(scenario_json) = generate_validate_runtime_scenario(profile) else {
        return Err(PersistenceError::Json {
            reason: "cannot generate validate-runtime scenario for shim profile".to_string(),
        });
    };
    let relative = std::path::Path::new("scenarios/validate-runtime-scenario.json");
    persistence::write_artifact_atomic(
        artifact_dir,
        relative,
        &scenario_json,
        manifest,
        "validate-runtime-scenario",
        ArtifactKind::Scenario,
    )?;
    // Finding #1: persist manifest BEFORE tmux so the scenario artifact is
    // tracked even if the run fails.
    persistence::save_manifest_atomic(manifest, run_root)?;
    Ok(artifact_dir.join(relative))
}

#[cfg(test)]
#[path = "validate_runtime_tests.rs"]
mod tests;
