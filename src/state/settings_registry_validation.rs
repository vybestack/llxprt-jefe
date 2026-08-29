//! Candidate-wide Settings validation across independently owned registries.

use crate::persistence::SettingsCandidate;
use crate::persistence::diagnostic::{CfgCode, Diagnostic, DiagnosticPath, Severity};
use crate::published_workbench::PublishedWorkbench;
use crate::startup_candidate::WorkbenchStaticFailure;
use crate::startup_screens::ScreenStartupError;

use super::screens_editor::{self, CompositionStatus};

/// Every reason a registry owner refuses this candidate.
///
/// The document publishing is not the whole of "this candidate is valid": the
/// registries composed from it have their own rules, and a candidate that
/// publishes but composes into no keymap or an unusable screen is one a save
/// would make the session unable to start from. Each owner is asked, and each
/// answers in its own words.
pub(super) fn registry_refusals(
    candidate: &SettingsCandidate,
    workbench: &PublishedWorkbench,
) -> Vec<Diagnostic> {
    let (workbench_failures, candidate_screens) =
        crate::startup_candidate::recompose_workbench_candidate_failures_with_screens(
            workbench,
            candidate.published(),
        );
    let mut refusals = workbench_failures
        .into_iter()
        .map(workbench_failure_diagnostic)
        .collect::<Vec<_>>();
    if let Some(registry) = candidate_screens {
        refusals.extend(screen_refusals(candidate, &registry));
    }
    refusals.sort();
    refusals.dedup();
    refusals
}

fn workbench_failure_diagnostic(failure: WorkbenchStaticFailure) -> Diagnostic {
    match failure {
        WorkbenchStaticFailure::Screens(error) => screen_startup_diagnostic(error),
        WorkbenchStaticFailure::Actions(error) => error.as_settings_diagnostic(),
        failure => {
            let mut diagnostic = Diagnostic::new(
                CfgCode::E005,
                Severity::Error,
                DiagnosticPath::root(),
                None,
                "correct the candidate Settings so the complete workbench can publish",
            );
            diagnostic.redacted_detail = failure.to_string();
            diagnostic
        }
    }
}

fn screen_startup_diagnostic(error: ScreenStartupError) -> Diagnostic {
    match error {
        ScreenStartupError::Refused(refusal) => *refusal.configuration,
        ScreenStartupError::Definitions(error) => {
            let mut diagnostic = Diagnostic::new(
                CfgCode::E001,
                Severity::Error,
                DiagnosticPath::new(error.path.display().to_string()),
                None,
                "make the definitions directory readable, then retry",
            );
            diagnostic.redacted_detail = error.to_string();
            diagnostic
        }
        ScreenStartupError::Compiled(error) => {
            let mut diagnostic = Diagnostic::new(
                CfgCode::E005,
                Severity::Error,
                DiagnosticPath::root(),
                None,
                "update jefe or report the invalid compiled screen inventory",
            );
            diagnostic.redacted_detail = format!("internal compiled screen defect: {error}");
            diagnostic
        }
    }
}

fn screen_refusals(
    candidate: &SettingsCandidate,
    registry: &crate::workbench::ScreenRegistry,
) -> Vec<Diagnostic> {
    screens_editor::project_screens(registry, candidate.published())
        .into_iter()
        .filter_map(|row| match row.composition {
            CompositionStatus::Valid => None,
            CompositionStatus::Invalid { code, reason } => {
                Some(layout_diagnostic(row.screen_id.as_str(), &code, &reason))
            }
        })
        .collect()
}

fn layout_diagnostic(screen: &str, code: &str, reason: &str) -> Diagnostic {
    let mut diagnostic = Diagnostic::new(
        CfgCode::E005,
        Severity::Error,
        DiagnosticPath::new(format!("/workbench/layout_overrides/{screen}")),
        None,
        "correct the layout override, or reset it to the compiled layout",
    );
    diagnostic.redacted_detail = format!("{code}: {reason}");
    diagnostic
}
