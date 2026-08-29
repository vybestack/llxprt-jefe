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
    PackageScreenSources, ScreenFileCandidate, ScreenFileRejection,
};
use crate::persistence::settings_document::PublishedSettings;

use super::config::{CHROME_TOP, insets_config};
use super::descriptor::{LayoutNode, ScreenDescriptor};
use super::diagnostics::ScreenDiagnostic;
use super::geometry::Insets;
use super::ids::{CUSTOM_SCREEN_NAMESPACE, ScreenIdentity};
use super::lowering_error::LoweringError;
use super::panel_types::DEFINABLE_PANEL_TYPES;
use super::resource_schemas::ResourceSchema;
use super::screen_file::parse_screen_file;
use super::screen_file_bounds::ScreenSyntaxError;
use super::screen_lowering::{
    LoweredScreen, lower_package_screen, lower_screen_with_provider_panels,
};
use super::screens::{PTY_PANEL_TYPE, PackagePanelBinding, RegistryError, ScreenRegistry};
use super::validate::validate_descriptor;

/// A published screen registry and the warnings composing it produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenComposition {
    /// Compiled screens followed by lowered ones, in canonical order.
    pub registry: ScreenRegistry,
    /// Immutable schemas contributed by active local and package definitions.
    pub resource_schemas: Vec<ResourceSchema>,
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
/// # Errors
///
/// Returns [`CompositionRefused`] when any enabled definition cannot be parsed,
/// lowered, or composed. Nothing is published in that case.
pub fn compose_screens(
    compiled: &ScreenRegistry,
    candidates: &[ScreenFileCandidate],
    settings: &PublishedSettings,
) -> Result<ScreenComposition, CompositionRefused> {
    compose_screens_with_package_sources(
        compiled,
        candidates,
        &[],
        &PackageScreenSources::default(),
        settings,
    )
}

/// Compose screens from bytes captured before deterministic candidate work.
pub(crate) fn compose_screens_with_package_sources(
    compiled: &ScreenRegistry,
    candidates: &[ScreenFileCandidate],
    packages: &[InstalledPackage],
    package_sources: &PackageScreenSources,
    settings: &PublishedSettings,
) -> Result<ScreenComposition, CompositionRefused> {
    let enabled: BTreeSet<Id> = settings.workbench.enabled_screens.iter().cloned().collect();
    let selected = selected_packages(packages, settings);
    let provider_panel_types = selected
        .iter()
        .flat_map(|package| package.manifest().panels())
        .map(|panel| panel.id().as_str())
        .collect::<Vec<_>>();
    let mut screens: Vec<ScreenDescriptor> = compiled.screens().to_vec();
    let mut resource_schemas = Vec::new();
    let mut warnings = Vec::new();
    let mut panel_bindings = Vec::new();
    let compiled_path = DiagnosticPath::new("<compiled screens>");
    for descriptor in &screens {
        bind_selected_provider_panels(descriptor, &selected, &compiled_path, &mut panel_bindings)?;
    }
    for candidate in candidates {
        let path = DiagnosticPath::new(candidate.path.to_string_lossy());
        let Some(lowered) = compose_one(
            candidate,
            &enabled,
            &provider_panel_types,
            &path,
            &mut warnings,
        )?
        else {
            continue;
        };
        bind_selected_provider_panels(&lowered.descriptor, &selected, &path, &mut panel_bindings)?;
        resource_schemas.extend(lowered.resources);
        screens.push(lowered.descriptor);
    }
    compose_package_screens(
        packages,
        package_sources,
        settings,
        &mut screens,
        &mut resource_schemas,
        &mut panel_bindings,
    )?;
    apply_layout_overrides(&mut screens, settings, &mut warnings);
    order_screens(&mut screens, &settings.workbench.screen_order);
    let root = composition_root(candidates, packages, settings);
    validate_panel_ownership(&screens, &panel_bindings, &root)?;
    let registry = ScreenRegistry::with_panel_bindings(screens, panel_bindings)
        .map_err(|error| registry_refusal(&error, root))?;
    Ok(ScreenComposition {
        registry,
        resource_schemas,
        warnings,
    })
}

/// Bind selected-provider panel declarations referenced by one screen.
///
/// The selected manifest owns model/event/action authority. The compiled,
/// local, or package descriptor owns only placement and activation.
fn bind_selected_provider_panels(
    descriptor: &ScreenDescriptor,
    selected: &[&InstalledPackage],
    path: &DiagnosticPath,
    panel_bindings: &mut Vec<PackagePanelBinding>,
) -> Result<(), CompositionRefused> {
    for panel in &descriptor.panels {
        if panel.host_capability.is_some() {
            continue;
        }
        let declaration = selected.iter().find_map(|package| {
            package
                .manifest()
                .panels()
                .iter()
                .find(|candidate| candidate.id().as_str() == panel.panel_type.as_str())
                .map(|declaration| (*package, declaration))
        });
        let Some((package, declaration)) = declaration else {
            continue;
        };
        panel_bindings.push(PackagePanelBinding {
            screen: descriptor.id,
            panel: panel.id,
            owner: package.manifest().id().owner_id().clone(),
            panel_type: declaration.id().clone(),
            model_kinds: declaration.model_kinds().to_vec(),
            event_schema: declaration.event_schema().to_vec(),
            action_authority: package_action_authority(
                package.manifest(),
                descriptor.id.as_str(),
                path,
            )?,
        });
    }
    Ok(())
}

fn residual_compiled_screen(screen: ScreenIdentity) -> bool {
    matches!(screen, ScreenIdentity::Compiled(_))
}

fn validate_panel_ownership(
    screens: &[ScreenDescriptor],
    bindings: &[PackagePanelBinding],
    root: &DiagnosticPath,
) -> Result<(), CompositionRefused> {
    for screen in screens {
        for panel in &screen.panels {
            let panel_type = panel.panel_type.as_str();
            let host_owned = panel.host_capability.is_some()
                || panel_type == PTY_PANEL_TYPE
                || DEFINABLE_PANEL_TYPES.contains(&panel_type)
                || residual_compiled_screen(screen.id);
            let provider_owned = bindings.iter().any(|binding| {
                binding.screen == screen.id
                    && binding.panel == panel.id
                    && binding.panel_type.as_str() == panel_type
            });
            if !host_owned && !provider_owned {
                return Err(unowned_panel_refusal(
                    root,
                    screen.id.as_str(),
                    panel.id.as_str(),
                    panel_type,
                ));
            }
        }
    }
    Ok(())
}

fn unowned_panel_refusal(
    root: &DiagnosticPath,
    screen: &str,
    panel: &str,
    panel_type: &str,
) -> CompositionRefused {
    let detail = format!(
        "screen {screen} panel {panel} type {panel_type} lacks host or selected-provider ownership"
    );
    let mut configuration = Diagnostic::new(
        CfgCode::E005,
        Severity::Error,
        root.clone(),
        None,
        "select the panel provider or remove the unowned panel, then restart",
    );
    detail.clone_into(&mut configuration.redacted_detail);
    CompositionRefused {
        screen: Box::new(ScreenDiagnostic::refused(root.clone(), None, detail)),
        configuration: Box::new(configuration),
    }
}

/// Load, parse, and lower every screen each selected package contributes.
///
/// Each selected package's manifest-declared screen files are read from the
/// package directory, parsed, and lowered. The lowered descriptors are appended
/// to `screens`. Any unreadable file, syntax error, lowering error, or identity
/// mismatch refuses the whole composition.
fn compose_package_screens(
    packages: &[InstalledPackage],
    sources: &PackageScreenSources,
    settings: &PublishedSettings,
    screens: &mut Vec<ScreenDescriptor>,
    resource_schemas: &mut Vec<ResourceSchema>,
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
            let text = sources
                .get(package, contribution)
                .map_err(|rejection| CandidateFailure::Unreadable(rejection).refuse(&path))?;
            let file = parse_screen_file(text)
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
            let mut lowered = lower_package_screen(&file, expected.as_str(), &allowed, &file_path)
                .map_err(|error| CandidateFailure::Lowering(error).refuse(&path))?;
            apply_package_panel_chrome(&mut lowered.descriptor, &path)?;
            for panel in &lowered.descriptor.panels {
                let declaration = manifest
                    .panels()
                    .iter()
                    .find(|candidate| candidate.id().as_str() == panel.panel_type.as_str());
                if let Some(declaration) = declaration {
                    let action_authority =
                        package_action_authority(manifest, expected.as_str(), &path)?;
                    panel_bindings.push(PackagePanelBinding {
                        screen: lowered.descriptor.id,
                        panel: panel.id,
                        owner: manifest.id().owner_id().clone(),
                        panel_type: declaration.id().clone(),
                        model_kinds: declaration.model_kinds().to_vec(),
                        event_schema: declaration.event_schema().to_vec(),
                        action_authority,
                    });
                }
            }
            resource_schemas.extend(lowered.resources);
            screens.push(lowered.descriptor);
        }
    }
    Ok(())
}

fn apply_package_panel_chrome(
    descriptor: &mut ScreenDescriptor,
    path: &DiagnosticPath,
) -> Result<(), CompositionRefused> {
    let panel_config = insets_config(Insets::new(1, 1, 1, 1)).ok_or_else(|| {
        CandidateFailure::Lowering(LoweringError::ConfigKey {
            key: CHROME_TOP.to_owned(),
        })
        .refuse(path)
    })?;
    descriptor
        .panels
        .iter_mut()
        .for_each(|panel| panel.config.clone_from(&panel_config));
    Ok(())
}

fn package_action_authority(
    manifest: &crate::domain::plugin::Manifest,
    screen: &str,
    path: &DiagnosticPath,
) -> Result<Vec<crate::domain::action_registry::ActionId>, CompositionRefused> {
    manifest
        .actions()
        .iter()
        .filter(|action| {
            action
                .contexts()
                .iter()
                .any(|context| context.as_str() == screen)
        })
        .map(|action| crate::domain::action_registry::ActionId::parse(action.id().as_str()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            CandidateFailure::Lowering(LoweringError::IdentityMismatch {
                expected: error.to_string(),
            })
            .refuse(path)
        })
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
            .unwrap_or(usize::MAX)
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
    provider_panel_types: &[&str],
    path: &DiagnosticPath,
    warnings: &mut Vec<Diagnostic>,
) -> Result<Option<LoweredScreen>, CompositionRefused> {
    if !is_enabled(&candidate.member, enabled) {
        if let Err(failure) = inspect_candidate(candidate) {
            warnings.push(failure.warning(path));
        }
        return Ok(None);
    }
    lower_candidate(candidate, provider_panel_types)
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
fn lower_candidate(
    candidate: &ScreenFileCandidate,
    provider_panel_types: &[&str],
) -> Result<LoweredScreen, CandidateFailure> {
    let text = candidate
        .text
        .as_ref()
        .map_err(|rejection| CandidateFailure::Unreadable(rejection.clone()))?;
    let file = parse_screen_file(text).map_err(CandidateFailure::Syntax)?;
    lower_screen_with_provider_panels(
        &file,
        &candidate.member,
        &candidate.path,
        provider_panel_types,
    )
    .map_err(CandidateFailure::Lowering)
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
