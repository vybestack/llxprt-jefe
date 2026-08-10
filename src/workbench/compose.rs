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
use crate::persistence::plugin_inventory::{InstalledPackage, selected_packages};
use crate::persistence::screen_files::{
    ScreenFileCandidate, ScreenFileRejection, read_package_screen,
};
use crate::persistence::settings_document::PublishedSettings;

use super::descriptor::{LayoutNode, ScreenDescriptor};
use super::diagnostics::ScreenDiagnostic;
use super::ids::CUSTOM_SCREEN_NAMESPACE;
use super::lowering_error::LoweringError;
use super::screen_file::parse_screen_file;
use super::screen_file_bounds::ScreenSyntaxError;
use super::screen_lowering::{lower_package_screen, lower_screen};
use super::screens::{PackagePanelBinding, RegistryError, ScreenRegistry};
use super::validate::validate_descriptor;

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

/// Compose compiled screens with every enabled definition, as settings ask.
///
/// Delegates to [`compose_screens_with_packages`] with no package sources, so
/// the composition path that does not involve plugins is unchanged.
///
/// # Errors
///
/// Returns [`CompositionRefused`] when any enabled definition cannot be parsed,
/// lowered, or composed. Nothing is published in that case.
pub fn compose_screens(
    compiled: &ScreenRegistry,
    candidates: &[ScreenFileCandidate],
    settings: &PublishedSettings,
) -> Result<ScreenComposition, CompositionRefused> {
    compose_screens_with_packages(compiled, candidates, &[], settings)
}

/// Compose compiled screens with enabled definitions **and** selected package
/// screens, as settings ask.
///
/// Selected packages are resolved by [`selected_packages`], which consults the
/// same `PublishedSettings` that decides user-definition activation.  Each
/// selected package's manifest-declared screen files are loaded from the package
/// directory using the existing bounded, symlink-safe reader, parsed through the
/// sole screen-file parser, and lowered through the sole lowerer.  A package
/// screen may resolve only panel types its own manifest declared, so no dynamic
/// panel ids enter the global built-in registry.
///
/// Composition is transactional: if any selected package screen is malformed,
/// missing, mismatched, or duplicates another screen identity, the whole
/// candidate registry is refused and prior authority is retained.  Unselected
/// package versions contribute nothing.
///
/// # Errors
///
/// Returns [`CompositionRefused`] when any enabled definition or selected
/// package screen cannot be parsed, lowered, or composed.
pub fn compose_screens_with_packages(
    compiled: &ScreenRegistry,
    candidates: &[ScreenFileCandidate],
    packages: &[InstalledPackage],
    settings: &PublishedSettings,
) -> Result<ScreenComposition, CompositionRefused> {
    let enabled: BTreeSet<Id> = settings.workbench.enabled_screens.iter().cloned().collect();
    let mut screens: Vec<ScreenDescriptor> = compiled.screens().to_vec();
    let mut warnings = Vec::new();
    for candidate in candidates {
        let path = DiagnosticPath::new(candidate.path.to_string_lossy());
        let Some(lowered) = compose_one(candidate, &enabled, &path, &mut warnings)? else {
            continue;
        };
        screens.push(lowered);
    }
    let mut panel_bindings = Vec::new();
    compose_package_screens(packages, settings, &mut screens, &mut panel_bindings)?;
    apply_layout_overrides(&mut screens, settings, &mut warnings);
    order_screens(&mut screens, &settings.workbench.screen_order);
    let root = composition_root(candidates, packages, settings);
    let registry = ScreenRegistry::with_panel_bindings(screens, panel_bindings)
        .map_err(|error| registry_refusal(&error, root))?;
    Ok(ScreenComposition { registry, warnings })
}

/// Load, parse, and lower every screen each selected package contributes.
///
/// Each selected package's manifest-declared screen files are read from the
/// package directory, parsed, and lowered.  The lowered descriptors are appended
/// to `screens`.  Any failure — an unreadable file, a syntax error, a lowering
/// error, or an identity that does not match the manifest — refuses the whole
/// composition.
fn compose_package_screens(
    packages: &[InstalledPackage],
    settings: &PublishedSettings,
    screens: &mut Vec<ScreenDescriptor>,
    panel_bindings: &mut Vec<PackagePanelBinding>,
) -> Result<(), CompositionRefused> {
    for package in selected_packages(packages, settings) {
        let manifest = package.manifest();
        let allowed: Vec<&str> = manifest
            .panels()
            .iter()
            .map(|panel| panel.id().as_str())
            .collect();
        for contribution in manifest.screens() {
            let file_path = package.directory().join(contribution.path().as_str());
            let path = DiagnosticPath::new(file_path.to_string_lossy());
            let text = read_package_screen(package.directory(), contribution.path())
                .map_err(|rejection| CandidateFailure::Unreadable(rejection).refuse(&path))?;
            let file = parse_screen_file(&text)
                .map_err(|error| CandidateFailure::Syntax(error).refuse(&path))?;
            let declared = file.id.get_ref();
            let expected = contribution
                .screen_ids()
                .iter()
                .find(|id| id.as_str() == declared.as_str());
            let Some(expected) = expected else {
                let expected = contribution
                    .screen_ids()
                    .iter()
                    .map(Id::as_str)
                    .collect::<Vec<_>>()
                    .join(" or ");
                return Err(CandidateFailure::Lowering(LoweringError::IdentityMismatch {
                    expected,
                })
                .refuse(&path));
            };
            let lowered = lower_package_screen(&file, expected.as_str(), &allowed, &file_path)
                .map_err(|error| CandidateFailure::Lowering(error).refuse(&path))?;
            for panel in &lowered.descriptor.panels {
                let declaration = manifest
                    .panels()
                    .iter()
                    .find(|candidate| candidate.id().as_str() == panel.panel_type.as_str());
                if let Some(declaration) = declaration {
                    panel_bindings.push(PackagePanelBinding {
                        screen: lowered.descriptor.id,
                        panel: panel.id,
                        owner: manifest.id().owner_id().clone(),
                        panel_type: declaration.id().clone(),
                        model_kinds: declaration.model_kinds().to_vec(),
                        event_schema: declaration.event_schema().to_vec(),
                    });
                }
            }
            screens.push(lowered.descriptor);
        }
    }
    Ok(())
}

/// Replace the layout of every screen settings override, or say why not.
///
/// Each candidate descriptor is validated on its own before it is kept, so an
/// override that breaks an invariant is reported against the screen it names
/// rather than failing the whole registry with a message about something else.
fn apply_layout_overrides(
    screens: &mut [ScreenDescriptor],
    settings: &PublishedSettings,
    warnings: &mut Vec<Diagnostic>,
) {
    for (owner, values) in &settings.workbench.layout_overrides {
        let Some(index) = screens
            .iter()
            .position(|screen| screen.id.as_str() == owner.as_str())
        else {
            warnings.push(layout_warning(
                owner,
                "no screen of this name is composed, so its layout override does nothing",
            ));
            continue;
        };
        match candidate_layout(&screens[index], values) {
            Ok(layout) => screens[index].layout = layout,
            Err(reason) => warnings.push(layout_warning(owner, &reason)),
        }
    }
}

/// The layout one override describes, if this screen can be given it.
fn candidate_layout(
    screen: &ScreenDescriptor,
    values: &crate::domain::TypedMap,
) -> Result<LayoutNode, String> {
    let layout = super::screen_lowering_layout::lower_settings_layout(values)?;
    let mut candidate = screen.clone();
    candidate.layout = layout;
    validate_descriptor(&candidate).map_err(|error| error.to_string())?;
    Ok(candidate.layout)
}

/// Put the screens settings name first, in the order they are named.
///
/// A screen the order does not name keeps the position it already had, so an
/// order that names one screen moves that one and nothing else.
fn order_screens(screens: &mut [ScreenDescriptor], order: &[Id]) {
    screens.sort_by_key(|screen| {
        order
            .iter()
            .position(|named| named.as_str() == screen.id.as_str())
            .map_or(usize::MAX, |position| position)
    });
}

/// One warning about a layout override, naming the screen it was written for.
fn layout_warning(owner: &Id, detail: &str) -> Diagnostic {
    let mut diagnostic = Diagnostic::new(
        CfgCode::E006,
        Severity::Warning,
        DiagnosticPath::new(format!("/workbench/layout_overrides/{}", owner.as_str())),
        None,
        "correct the override in Settings, or reset it to the compiled layout",
    );
    detail.clone_into(&mut diagnostic.redacted_detail);
    diagnostic
}

/// Compose one candidate, or explain why it contributes nothing.
///
/// A dormant candidate is read and parsed but never lowered. That is what
/// "preserve and omit" means in practice: the file is inspected far enough to
/// say whether it is well formed, and no further, so a screen nobody enabled
/// cannot consume identifier capacity or resolve anything against the compiled
/// registries.
fn compose_one(
    candidate: &ScreenFileCandidate,
    enabled: &BTreeSet<Id>,
    path: &DiagnosticPath,
    warnings: &mut Vec<Diagnostic>,
) -> Result<Option<ScreenDescriptor>, CompositionRefused> {
    if !is_enabled(&candidate.member, enabled) {
        if let Err(failure) = inspect_candidate(candidate) {
            warnings.push(failure.warning(path));
        }
        return Ok(None);
    }
    lower_candidate(candidate)
        .map(Some)
        .map_err(|failure| failure.refuse(path))
}

/// Read and parse a candidate without lowering it.
fn inspect_candidate(candidate: &ScreenFileCandidate) -> Result<(), CandidateFailure> {
    let text = candidate
        .text
        .as_ref()
        .map_err(|rejection| CandidateFailure::Unreadable(rejection.clone()))?;
    parse_screen_file(text)
        .map(|_| ())
        .map_err(CandidateFailure::Syntax)
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

/// The directory the registry-level diagnostic is attributed to.
///
/// When user-definition candidates exist, the definitions directory is used;
/// otherwise, the first selected package's root stands in.  Either way, a
/// duplicate-identity or overflow refusal names the directory the conflicting
/// screens came from.
fn composition_root(
    candidates: &[ScreenFileCandidate],
    packages: &[InstalledPackage],
    settings: &PublishedSettings,
) -> DiagnosticPath {
    if let Some(root) = candidates
        .first()
        .and_then(|candidate| candidate.path.parent())
    {
        return DiagnosticPath::new(root.to_string_lossy());
    }
    selected_packages(packages, settings).first().map_or_else(
        || DiagnosticPath::new("definitions"),
        |package| DiagnosticPath::new(package.root().to_string_lossy()),
    )
}
