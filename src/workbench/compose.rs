//! Transactional composition of compiled and lowered screens (issue #385,
//! CW05-03, CW05-04).
//!
//! Composition is all-or-nothing. Every active definition is parsed, lowered,
//! and validated into a candidate registry, and only a complete candidate is
//! published; one broken active definition refuses the whole registry and leaves
//! whatever authority was already in place untouched. A partially composed
//! registry would mean the program was running a screen set no one wrote, which
//! is worse than not starting.
//!
//! Activation is decided from settings before a file is lowered, and it decides
//! how loudly a failure is reported:
//!
//! - a definition whose owner is **not** enabled is never lowered. If it is also
//!   invalid, that is a warning naming the file. Its bytes stay exactly as they
//!   are, because an author's half-finished screen is not the program's to edit;
//! - a definition whose owner **is** enabled must be usable. A failure refuses
//!   the candidate registry and names both the screen rule and the configuration
//!   rule that was broken.
//!
//! Order is the discovery order, which is canonical path order, so the first
//! failure a machine reports is the first failure every machine reports.

use std::collections::BTreeSet;

use crate::domain::Id;
use crate::persistence::diagnostic::{CfgCode, Diagnostic, DiagnosticPath, Severity};
use crate::persistence::screen_files::{ScreenFileCandidate, ScreenFileRejection};

use super::descriptor::ScreenDescriptor;
use super::diagnostics::ScreenDiagnostic;
use super::ids::CUSTOM_SCREEN_NAMESPACE;
use super::lowering_error::LoweringError;
use super::screen_file::parse_screen_file;
use super::screen_file_bounds::ScreenSyntaxError;
use super::screen_lowering::lower_screen;
use super::screens::{RegistryError, ScreenRegistry};

/// A published screen registry and the warnings composing it produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenComposition {
    /// Compiled screens followed by lowered ones, in canonical order.
    pub registry: ScreenRegistry,
    /// Warnings about definitions that were preserved and omitted.
    pub warnings: Vec<Diagnostic>,
}

/// The candidate registry was refused; prior authority is retained.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompositionRefused {
    /// Why the registry was refused.
    ///
    /// Both halves are boxed because a refusal travels in the error arm of
    /// every composition result, and a diagnostic pair is far larger than the
    /// registry a success carries.
    pub screen: Box<ScreenDiagnostic>,
    /// Which configuration rule the offending definition broke.
    pub configuration: Box<Diagnostic>,
}

impl std::fmt::Display for CompositionRefused {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.screen)
    }
}

impl std::error::Error for CompositionRefused {}

/// Compose compiled screens with every enabled definition.
///
/// `enabled` is the schema-2 `workbench.enabled_screens` set. A definition whose
/// `local.<member>` identity is absent from it is dormant.
///
/// # Errors
///
/// Returns [`CompositionRefused`] when any enabled definition cannot be parsed,
/// lowered, or composed. Nothing is published in that case.
pub fn compose_screens(
    compiled: &ScreenRegistry,
    candidates: &[ScreenFileCandidate],
    enabled: &BTreeSet<Id>,
) -> Result<ScreenComposition, CompositionRefused> {
    let mut screens: Vec<ScreenDescriptor> = compiled.screens().to_vec();
    let mut warnings = Vec::new();
    for candidate in candidates {
        let path = DiagnosticPath::new(candidate.path.to_string_lossy());
        let Some(lowered) = compose_one(candidate, enabled, &path, &mut warnings)? else {
            continue;
        };
        screens.push(lowered);
    }
    let registry = ScreenRegistry::new(screens)
        .map_err(|error| registry_refusal(&error, candidates_root(candidates)))?;
    Ok(ScreenComposition { registry, warnings })
}

/// Compose one candidate, or explain why it contributes nothing.
fn compose_one(
    candidate: &ScreenFileCandidate,
    enabled: &BTreeSet<Id>,
    path: &DiagnosticPath,
    warnings: &mut Vec<Diagnostic>,
) -> Result<Option<ScreenDescriptor>, CompositionRefused> {
    let active = is_enabled(&candidate.member, enabled);
    match lower_candidate(candidate) {
        Ok(descriptor) if active => Ok(Some(descriptor)),
        Ok(_) => Ok(None),
        Err(failure) if active => Err(failure.refuse(path)),
        Err(failure) => {
            warnings.push(failure.warning(path));
            Ok(None)
        }
    }
}

/// Whether settings enable the owner this file declares.
fn is_enabled(member: &str, enabled: &BTreeSet<Id>) -> bool {
    Id::parse(&format!("{CUSTOM_SCREEN_NAMESPACE}{member}"))
        .is_ok_and(|owner| enabled.contains(&owner))
}

/// Read, parse, and lower one candidate.
fn lower_candidate(candidate: &ScreenFileCandidate) -> Result<ScreenDescriptor, CandidateFailure> {
    let text = candidate
        .text
        .as_ref()
        .map_err(|rejection| CandidateFailure::Unreadable(rejection.clone()))?;
    let file = parse_screen_file(text).map_err(CandidateFailure::Syntax)?;
    let lowered = lower_screen(&file, &candidate.member, &candidate.path)
        .map_err(CandidateFailure::Lowering)?;
    Ok(lowered.descriptor)
}

/// Why one candidate produced no descriptor.
enum CandidateFailure {
    /// Discovery could not hand over usable bytes.
    Unreadable(ScreenFileRejection),
    /// The bytes are not this syntax.
    Syntax(ScreenSyntaxError),
    /// The syntax is not a usable screen.
    Lowering(LoweringError),
}

impl CandidateFailure {
    /// The configuration rule family this failure belongs to.
    const fn cfg_code(&self) -> CfgCode {
        match self {
            Self::Unreadable(_) | Self::Syntax(_) => CfgCode::E006,
            Self::Lowering(error) => error.cfg_code(),
        }
    }

    /// The byte range the failure is attributable to, if any.
    fn span(&self) -> Option<crate::domain::ByteSpan> {
        match self {
            Self::Syntax(error) => error.span,
            _ => None,
        }
    }

    /// The violated rule, with no value from the file.
    fn detail(&self) -> String {
        match self {
            Self::Unreadable(rejection) => rejection.to_string(),
            Self::Syntax(error) => error.to_string(),
            Self::Lowering(error) => error.to_string(),
        }
    }

    /// Refuse the whole candidate registry over this failure.
    fn refuse(&self, path: &DiagnosticPath) -> CompositionRefused {
        let detail = self.detail();
        let mut configuration = Diagnostic::new(
            self.cfg_code(),
            Severity::Error,
            path.clone(),
            self.span(),
            "correct or disable the named screen definition, then restart",
        );
        configuration.redacted_detail.clone_from(&detail);
        CompositionRefused {
            screen: Box::new(ScreenDiagnostic::refused(path.clone(), self.span(), detail)),
            configuration: Box::new(configuration),
        }
    }

    /// Report this failure without refusing anything, because nothing enabled
    /// depends on the file.
    fn warning(&self, path: &DiagnosticPath) -> Diagnostic {
        let mut warning = Diagnostic::new(
            CfgCode::W004,
            Severity::Warning,
            path.clone(),
            self.span(),
            "correct the definition before enabling it; its bytes are unchanged",
        );
        warning.redacted_detail = self.detail();
        warning
    }
}

/// Turn a registry-level refusal into a diagnostic.
///
/// These are failures of the composed set rather than of one file — a duplicate
/// identity or too many screens — so they are reported against the definitions
/// directory.
fn registry_refusal(error: &RegistryError, root: DiagnosticPath) -> CompositionRefused {
    let detail = error.to_string();
    let mut configuration = Diagnostic::new(
        CfgCode::E005,
        Severity::Error,
        root.clone(),
        None,
        "remove or disable one of the conflicting screen definitions, then restart",
    );
    configuration.redacted_detail.clone_from(&detail);
    CompositionRefused {
        screen: Box::new(ScreenDiagnostic::refused(root, None, detail)),
        configuration: Box::new(configuration),
    }
}

/// The directory the candidates came from, for registry-level diagnostics.
fn candidates_root(candidates: &[ScreenFileCandidate]) -> DiagnosticPath {
    candidates
        .first()
        .and_then(|candidate| candidate.path.parent())
        .map_or_else(
            || DiagnosticPath::new("definitions"),
            |root| DiagnosticPath::new(root.to_string_lossy()),
        )
}
