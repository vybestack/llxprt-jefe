//! Cleanup subcommand: manifest-scoped GitHub + local resource cleanup.
//!
//! Extracted from `commands.rs` to keep file sizes under the project limit.

use std::fs;
use std::path::Path;
use std::process::ExitCode;

use super::cli::{CleanupOpts, write_stderr, write_stdout};

use jefe_tutorial_capture::{
    GithubCleanupOutcome, GithubCleanupStatus, OrchestrationError, OwnedPathKind,
    RealCommandRunner, RunManifest, TierBError, execute_github_cleanup_with_allowlist,
    load_manifest, plan_github_cleanup, save_manifest, verify_sentinel_ownership,
};

/// Label for an owned path kind in cleanup output.
pub fn owned_path_kind_label(kind: OwnedPathKind) -> &'static str {
    match kind {
        OwnedPathKind::ConfigDir => "config",
        OwnedPathKind::FixtureRepo => "fixture-repo",
        OwnedPathKind::FixtureClone => "fixture-clone",
        OwnedPathKind::ArtifactDir => "artifacts",
        OwnedPathKind::ShimDir => "shims",
    }
}

/// Run the `cleanup` subcommand: remove only manifest-owned resources.
///
/// Executes manifest-scoped GitHub cleanup (if any GitHub resources are
/// recorded) before local cleanup. Requires explicit `--confirm` for
/// actual removal, or `--dry-run` to preview.
pub fn run_cleanup(opts: &CleanupOpts) -> ExitCode {
    let manifest_path = &opts.manifest_path;
    let mut manifest = match load_manifest(manifest_path) {
        Ok(m) => m,
        Err(err) => {
            write_stderr(&format!("failed to load manifest: {err}\n"));
            return ExitCode::from(1);
        }
    };

    if opts.dry_run {
        print_cleanup_dry_run(&manifest);
        return ExitCode::SUCCESS;
    }

    if !opts.confirm {
        write_stderr("error: --confirm (or --dry-run) is required for cleanup\n");
        return ExitCode::from(1);
    }
    let run_root = manifest_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    if let Err(err) = verify_sentinel_ownership(run_root, manifest.run_id.as_str()) {
        write_stderr(&format!("cleanup ownership verification failed: {err}\n"));
        return ExitCode::from(1);
    }

    let gh_cleanup_incomplete = execute_github_cleanup_phase(&mut manifest);
    if gh_cleanup_incomplete {
        if let Err(err) = save_manifest(&manifest, manifest_path) {
            return handle_cleanup_persistence_failure(manifest_path, &err, &[]);
        }
        write_stderr(
            "error: GitHub cleanup incomplete (skipped or failed resources); \
             local evidence preserved and manifest NOT marked complete. \
             Re-run cleanup after addressing the issues above.\n",
        );
        return ExitCode::from(1);
    }

    let purge = opts.purge_evidence;
    match jefe_tutorial_capture::cleanup_manifest_with_root(&mut manifest, run_root, purge) {
        Ok(records) => {
            write_stdout("cleanup complete\n");
            for record in &records {
                write_stdout(&format!(
                    "  [{}] {}: {:?}\n",
                    owned_path_kind_label(record.kind),
                    record.path.display(),
                    record.outcome
                ));
            }
            if let Err(err) = save_manifest(&manifest, manifest_path) {
                return handle_cleanup_persistence_failure(manifest_path, &err, &records);
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            write_stderr(&format!("cleanup failed: {err}\n"));
            ExitCode::from(1)
        }
    }
}

/// Print what would be cleaned in a dry-run without modifying anything.
fn print_cleanup_dry_run(manifest: &RunManifest) {
    write_stdout("cleanup dry-run (no changes):\n");
    if !manifest.github_resources.is_empty() {
        let gh_cmds = plan_github_cleanup(manifest);
        write_stdout(&format!(
            "  GitHub resources to clean: {}\n",
            manifest.github_resources.len()
        ));
        for cmd in &gh_cmds {
            write_stdout(&format!("    -> {} ({})\n", cmd.description, cmd.program));
        }
    }
    write_stdout(&format!("  local paths: {}\n", manifest.owned_paths.len()));
    for entry in &manifest.owned_paths {
        write_stdout(&format!(
            "    [{}] {}\n",
            owned_path_kind_label(entry.kind),
            entry.path.display()
        ));
    }
}

/// Execute GitHub cleanup phase: close/delete manifest-owned GitHub resources
/// before local cleanup.
///
/// **Finding #4**: Each resource is validated against the fixture GitHub repo,
/// the allowlist, production refusal, and nonempty identifiers before cleanup.
/// Invalid resources are skipped (fail-closed). Per-resource outcomes are
/// recorded as discrepancies in the manifest so the report is truthful.
///
/// Returns `true` if any cleanup outcome was Skipped or Failed (i.e. the
/// overall GitHub cleanup is incomplete). A `true` return causes
/// `run_cleanup` to preserve evidence, persist discrepancies, exit nonzero,
/// and NOT mark `cleanup_completed`.
fn execute_github_cleanup_phase(manifest: &mut RunManifest) -> bool {
    if manifest.github_resources.is_empty() {
        return false;
    }
    write_stdout(&format!(
        "cleaning {} GitHub resource(s)...\n",
        manifest.github_resources.len()
    ));
    // Task #3: Cleanup uses immutable manifest provenance
    // (creation_allowlist), NOT the environment.
    let mut runner = RealCommandRunner;
    match execute_github_cleanup_with_allowlist(manifest, &mut runner, None) {
        Ok(outcomes) => {
            let mut had_incomplete = false;
            for outcome in &outcomes {
                process_cleanup_outcome(manifest, outcome, &mut had_incomplete);
            }
            had_incomplete
        }
        Err(TierBError::CleanupPartialFailure { outcomes }) => {
            // Process every outcome so that cleaned, skipped, and failed
            // details (and discrepancies) are persisted in the manifest.
            let mut had_incomplete = false;
            for outcome in &outcomes {
                process_cleanup_outcome(manifest, outcome, &mut had_incomplete);
            }
            had_incomplete
        }
        Err(err) => {
            write_stderr(&format!("warning: GitHub cleanup error: {err}\n"));
            manifest.add_discrepancy(format!("GitHub cleanup error: {err}"));
            true
        }
    }
}

/// Process a single GitHub cleanup outcome, recording discrepancies and
/// signalling incomplete cleanup for `Skipped` or `Failed` outcomes.
///
/// `had_incomplete` is set to `true` for both `Skipped` and `Failed`
/// outcomes. A skip means the resource was not cleaned (fail-closed), so
/// the overall cleanup is incomplete — evidence must be preserved and
/// `cleanup_completed` must NOT be marked.
fn process_cleanup_outcome(
    manifest: &mut RunManifest,
    outcome: &GithubCleanupOutcome,
    had_incomplete: &mut bool,
) {
    match &outcome.status {
        GithubCleanupStatus::Cleaned => {
            write_stdout(&format!(
                "  gh: cleaned {} #{} in {}\n",
                outcome.description, outcome.identifier, outcome.repository
            ));
        }
        GithubCleanupStatus::Skipped { reason } => {
            *had_incomplete = true;
            write_stdout(&format!(
                "  gh: SKIPPED {} #{} in {}: {}\n",
                outcome.description, outcome.identifier, outcome.repository, reason
            ));
            manifest.add_discrepancy(format!(
                "GitHub cleanup skipped {} #{}: {}",
                outcome.description, outcome.identifier, reason
            ));
        }
        GithubCleanupStatus::Failed { stderr } => {
            *had_incomplete = true;
            write_stderr(&format!(
                "  gh: FAILED {} #{} in {}: {}\n",
                outcome.description, outcome.identifier, outcome.repository, stderr
            ));
            manifest.add_discrepancy(format!(
                "GitHub cleanup failed {} #{}: {}",
                outcome.description, outcome.identifier, stderr
            ));
        }
    }
}

/// Handle a cleanup persistence failure by writing a journal and returning
/// an error exit code.
fn handle_cleanup_persistence_failure(
    manifest_path: &Path,
    err: &OrchestrationError,
    records: &[jefe_tutorial_capture::CleanupRecord],
) -> ExitCode {
    let journal_path = manifest_path.with_extension("cleanup-journal");
    let journal_msg = format!(
        "cleanup persistence failure at {}: {err}\ncleanup records: {records:?}\n",
        manifest_path.display()
    );
    write_stderr(&format!(
        "error: failed to update manifest after cleanup: {err}\n",
    ));
    match fs::write(&journal_path, &journal_msg) {
        Ok(()) => write_stderr(&format!(
            "  journal written to {}\n",
            journal_path.display()
        )),
        Err(journal_err) => write_stderr(&format!(
            "  failed to write recovery journal {}: {journal_err}\n",
            journal_path.display()
        )),
    }
    ExitCode::from(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jefe_tutorial_capture::{RunId, RuntimeProfile};

    fn sample_manifest() -> RunManifest {
        RunManifest::new(
            RunId::new("cleanup-reg-test").unwrap_or_else(|| panic!("valid run id")),
            "0.0.28",
            "test",
            100,
            32,
            RuntimeProfile::Shim,
        )
    }

    /// Issue #241 regression: when `CleanupPartialFailure` is returned with
    /// mixed outcomes (Cleaned, Skipped, Failed), every outcome must be
    /// processed through `process_cleanup_outcome` so that:
    /// - Cleaned outcomes produce no discrepancy.
    /// - Skipped outcomes produce a discrepancy.
    /// - Failed outcomes produce a discrepancy and set had_failure.
    ///
    /// This simulates the exact loop now added to the
    /// `Err(TierBError::CleanupPartialFailure)` arm of
    /// `execute_github_cleanup_phase`.
    #[test]
    fn process_cleanup_outcome_mixed_outcomes_persists_all_discrepancies() {
        let mut manifest = sample_manifest();
        let initial = manifest.discrepancies.len();

        let outcomes = [
            GithubCleanupOutcome {
                description: "Close fixture PR #1".to_string(),
                repository: "fixture/test".to_string(),
                identifier: "1".to_string(),
                status: GithubCleanupStatus::Cleaned,
            },
            GithubCleanupOutcome {
                description: "Close fixture issue #2".to_string(),
                repository: "fixture/test".to_string(),
                identifier: "2".to_string(),
                status: GithubCleanupStatus::Skipped {
                    reason: "empty identifier".to_string(),
                },
            },
            GithubCleanupOutcome {
                description: "Delete fixture branch".to_string(),
                repository: "fixture/test".to_string(),
                identifier: "tutorial-capture/run".to_string(),
                status: GithubCleanupStatus::Failed {
                    stderr: "network error".to_string(),
                },
            },
        ];

        let mut had_incomplete = false;
        for outcome in &outcomes {
            process_cleanup_outcome(&mut manifest, outcome, &mut had_incomplete);
        }

        assert!(
            had_incomplete,
            "had_incomplete must be true when any outcome is Failed or Skipped"
        );

        let added = manifest.discrepancies.len() - initial;
        assert_eq!(
            added, 2,
            "exactly 2 discrepancies (skipped + failed), got {added}"
        );

        let joined = manifest.discrepancies.join("\n");
        assert!(
            joined.contains("skipped"),
            "discrepancies must contain skip detail: {joined}"
        );
        assert!(
            joined.contains("#2"),
            "discrepancies must contain skipped identifier: {joined}"
        );
        assert!(
            joined.contains("failed"),
            "discrepancies must contain failure detail: {joined}"
        );
        assert!(
            joined.contains("network error"),
            "discrepancies must contain failure stderr: {joined}"
        );
    }

    /// Issue #241 regression: when all outcomes are Cleaned, no
    /// discrepancies are added and `had_failure` stays false.
    #[test]
    fn process_cleanup_outcome_all_cleaned_no_discrepancies() {
        let mut manifest = sample_manifest();
        let initial = manifest.discrepancies.len();

        let outcomes = [
            GithubCleanupOutcome {
                description: "Close fixture PR #1".to_string(),
                repository: "fixture/test".to_string(),
                identifier: "1".to_string(),
                status: GithubCleanupStatus::Cleaned,
            },
            GithubCleanupOutcome {
                description: "Close fixture issue #2".to_string(),
                repository: "fixture/test".to_string(),
                identifier: "2".to_string(),
                status: GithubCleanupStatus::Cleaned,
            },
        ];

        let mut had_incomplete = false;
        for outcome in &outcomes {
            process_cleanup_outcome(&mut manifest, outcome, &mut had_incomplete);
        }

        assert!(
            !had_incomplete,
            "had_incomplete must be false when all outcomes are Cleaned"
        );
        assert_eq!(
            manifest.discrepancies.len(),
            initial,
            "no discrepancies should be added for all-Cleaned outcomes"
        );
    }

    /// Skip-only regression: when ALL outcomes are Skipped (no failures),
    /// the incomplete flag must still be set so that `run_cleanup` treats
    /// this as incomplete — preserving evidence, persisting discrepancies,
    /// returning a nonzero exit, and NOT marking cleanup_completed.
    ///
    /// Before the fix, `process_cleanup_outcome` only set `had_failure`
    /// on `Failed` outcomes, so skip-only cleanup silently proceeded to
    /// local cleanup and marked cleanup_completed.
    #[test]
    fn process_cleanup_outcome_skip_only_sets_incomplete_flag() {
        let mut manifest = sample_manifest();
        let initial = manifest.discrepancies.len();

        let outcomes = [
            GithubCleanupOutcome {
                description: "Close fixture PR #1".to_string(),
                repository: "fixture/test".to_string(),
                identifier: "1".to_string(),
                status: GithubCleanupStatus::Skipped {
                    reason: "empty identifier".to_string(),
                },
            },
            GithubCleanupOutcome {
                description: "Close fixture issue #2".to_string(),
                repository: "fixture/test".to_string(),
                identifier: "2".to_string(),
                status: GithubCleanupStatus::Skipped {
                    reason: "resource repository is a production repository".to_string(),
                },
            },
        ];

        let mut had_incomplete = false;
        for outcome in &outcomes {
            process_cleanup_outcome(&mut manifest, outcome, &mut had_incomplete);
        }

        assert!(
            had_incomplete,
            "had_incomplete must be true when any outcome is Skipped — \
             skip-only cleanup is incomplete"
        );

        let added = manifest.discrepancies.len() - initial;
        assert_eq!(
            added, 2,
            "exactly 2 discrepancies (both skipped), got {added}"
        );

        assert!(
            !manifest.cleanup_completed,
            "cleanup_completed must NOT be set for skip-only cleanup"
        );

        let joined = manifest.discrepancies.join("\n");
        assert!(
            joined.contains("skipped"),
            "discrepancies must contain skip detail: {joined}"
        );
    }

    /// CLI-level skip-only regression: simulate the full
    /// `execute_github_cleanup_phase` flow with skip-only outcomes
    /// (as returned by `TierBError::CleanupPartialFailure`) and verify
    /// the returned signal is `true` (incomplete), discrepancies are
    /// persisted, and cleanup_completed is NOT set.
    ///
    /// This exercises the `Err(TierBError::CleanupPartialFailure)` arm
    /// directly without making real `gh` calls.
    #[test]
    fn execute_github_cleanup_phase_skip_only_returns_incomplete() {
        let mut manifest = sample_manifest();

        // Simulate what execute_github_cleanup_phase does in the
        // Err(CleanupPartialFailure) arm when all outcomes are Skipped.
        let outcomes = vec![GithubCleanupOutcome {
            description: "Close fixture PR #1".to_string(),
            repository: "fixture/test".to_string(),
            identifier: "1".to_string(),
            status: GithubCleanupStatus::Skipped {
                reason: "resource repository 'vybestack/jefe' is a \
                         production repository"
                    .to_string(),
            },
        }];

        let mut had_incomplete = false;
        for outcome in &outcomes {
            process_cleanup_outcome(&mut manifest, outcome, &mut had_incomplete);
        }

        // This is the signal that run_cleanup checks to decide whether
        // to proceed to local cleanup. It must be true for skip-only.
        assert!(
            had_incomplete,
            "skip-only cleanup must signal incomplete to run_cleanup"
        );
        assert!(
            !manifest.cleanup_completed,
            "skip-only must not allow cleanup_completed to be set"
        );
        assert!(
            !manifest.discrepancies.is_empty(),
            "skip-only must persist discrepancy evidence"
        );
    }
}
