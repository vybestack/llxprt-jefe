//! The closed plugin manifest and its cross-declaration validation
//! (issue #389 CW-09, acceptance rows D4, D5 and D7).
//!
//! Every declaration has already validated itself by the time it reaches here.
//! This module owns only what needs the whole manifest in view:
//!
//! * **Ownership.** Every declared id must sit beneath the package's own
//!   namespace, so one package can never claim another's action or panel.
//! * **Provider consistency.** A package that declares no provider may not
//!   declare handlers, because there would be nothing to run them.
//! * **Reference resolution.** A route must target a screen the package
//!   actually contributes, and a default may only enable something declared.
//! * **Single binding.** Each contributed screen id is bound exactly once, so
//!   no descriptor silently shadows another.
//!
//! Validation is pure. It takes declarations and returns declarations or a
//! diagnostic; it never opens a file and never starts a process.

use std::collections::BTreeSet;
use std::fmt;

use super::action::Action;
use super::coordinate::PackageCoordinate;
use super::limits::{
    ACTION_LIMIT, MANIFEST_PROTOCOL, MANIFEST_SCHEMA, PANEL_LIMIT, ROUTE_LIMIT,
    SCREEN_CONTRIBUTION_LIMIT,
};
use super::plugin_id::PluginId;
use super::provider::{Provider, ProviderMode};
use super::surface::{ConfigSchema, Panel, Route, ScreenContribution};
use crate::domain::{CanonicalSemver, Id, TypedValue};

/// What a package enables and configures out of the box.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PluginDefaults {
    /// Actions enabled by default.
    pub actions_enabled: Vec<Id>,
    /// Screens enabled by default.
    pub screens_enabled: Vec<Id>,
    /// Default configuration values, by field id.
    pub config: Vec<(Id, TypedValue)>,
}

/// An unvalidated manifest, as read from `plugin.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestDraft {
    /// Declared manifest schema version.
    pub manifest_schema: u32,
    /// Package identity.
    pub id: PluginId,
    /// Package version.
    pub version: CanonicalSemver,
    /// Operator-facing name.
    pub display_name: String,
    /// Lowest host API this package supports.
    pub host_api_minimum: CanonicalSemver,
    /// Highest host API this package supports.
    pub host_api_maximum: CanonicalSemver,
    /// Declared provider protocol version.
    pub protocol: u32,
    /// Provider declaration.
    pub provider: Provider,
    /// Configuration schema, if the package has one.
    pub config: Option<ConfigSchema>,
    /// Declared actions.
    pub actions: Vec<Action>,
    /// Declared panels.
    pub panels: Vec<Panel>,
    /// Declared routes.
    pub routes: Vec<Route>,
    /// Contributed screen descriptor files.
    pub screens: Vec<ScreenContribution>,
    /// Defaults applied on first enable.
    pub defaults: Option<PluginDefaults>,
}

/// A validated, immutable manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    draft: ManifestDraft,
    coordinate: PackageCoordinate,
}

impl Manifest {
    /// Validate a complete manifest.
    ///
    /// # Errors
    ///
    /// Returns [`ManifestError`] for an unsupported schema or protocol, an
    /// inverted host API range, a blank display name, an exceeded declaration
    /// bound, a foreign or duplicated owner id, a handler declared without a
    /// provider, a screen bound twice, or a reference that does not resolve.
    pub fn parse(draft: ManifestDraft) -> Result<Self, ManifestError> {
        validate_header(&draft)?;
        validate_bounds(&draft)?;
        validate_ownership(&draft)?;
        validate_provider_consistency(&draft)?;
        let screens = validate_screens(&draft)?;
        validate_routes(&draft, &screens)?;
        validate_defaults(&draft, &screens)?;
        let coordinate = PackageCoordinate::new(draft.id.clone(), draft.version.clone());
        Ok(Self { draft, coordinate })
    }

    /// The package identity.
    #[must_use]
    pub const fn id(&self) -> &PluginId {
        &self.draft.id
    }

    /// The package version.
    #[must_use]
    pub const fn version(&self) -> &CanonicalSemver {
        &self.draft.version
    }

    /// The package's exact `(id, version)` coordinate.
    #[must_use]
    pub const fn coordinate(&self) -> &PackageCoordinate {
        &self.coordinate
    }

    /// The operator-facing name.
    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.draft.display_name
    }

    /// The provider declaration.
    #[must_use]
    pub const fn provider(&self) -> &Provider {
        &self.draft.provider
    }

    /// The configuration schema, if any.
    #[must_use]
    pub const fn config(&self) -> Option<&ConfigSchema> {
        self.draft.config.as_ref()
    }

    /// Declared actions.
    #[must_use]
    pub fn actions(&self) -> &[Action] {
        &self.draft.actions
    }

    /// Declared panels.
    #[must_use]
    pub fn panels(&self) -> &[Panel] {
        &self.draft.panels
    }

    /// Declared routes.
    #[must_use]
    pub fn routes(&self) -> &[Route] {
        &self.draft.routes
    }

    /// Contributed screen descriptor files.
    #[must_use]
    pub fn screens(&self) -> &[ScreenContribution] {
        &self.draft.screens
    }

    /// Defaults applied on first enable.
    #[must_use]
    pub const fn defaults(&self) -> Option<&PluginDefaults> {
        self.draft.defaults.as_ref()
    }
}

fn validate_header(draft: &ManifestDraft) -> Result<(), ManifestError> {
    if draft.manifest_schema != MANIFEST_SCHEMA {
        return Err(ManifestError::UnsupportedSchema {
            found: draft.manifest_schema,
        });
    }
    if draft.protocol != MANIFEST_PROTOCOL {
        return Err(ManifestError::UnsupportedProtocol {
            found: draft.protocol,
        });
    }
    if draft.display_name.trim().is_empty() {
        return Err(ManifestError::BlankDisplayName);
    }
    if draft
        .host_api_minimum
        .precedence_cmp(&draft.host_api_maximum)
        == std::cmp::Ordering::Greater
    {
        return Err(ManifestError::InvertedHostApiRange);
    }
    Ok(())
}

fn validate_bounds(draft: &ManifestDraft) -> Result<(), ManifestError> {
    for (kind, len, limit) in [
        ("action", draft.actions.len(), ACTION_LIMIT),
        ("panel", draft.panels.len(), PANEL_LIMIT),
        ("route", draft.routes.len(), ROUTE_LIMIT),
        (
            "screen contribution",
            draft.screens.len(),
            SCREEN_CONTRIBUTION_LIMIT,
        ),
    ] {
        if len > limit {
            return Err(ManifestError::TooManyDeclarations { kind, len });
        }
    }
    Ok(())
}

/// Whether `candidate` sits strictly beneath the package's namespace.
///
/// The comparison ends at a label boundary, so `vendor.pkgx.run` does not
/// belong to `vendor.pkg` even though it shares the same leading text, and the
/// bare package id is not itself an owned declaration.
fn is_owned(owner: &PluginId, candidate: &Id) -> bool {
    candidate
        .as_str()
        .strip_prefix(owner.as_str())
        .is_some_and(|rest| rest.starts_with('.') && rest.len() > 1)
}

fn validate_ownership(draft: &ManifestDraft) -> Result<(), ManifestError> {
    let declared: [(&'static str, Vec<&Id>); 3] = [
        ("action", draft.actions.iter().map(Action::id).collect()),
        ("panel", draft.panels.iter().map(Panel::id).collect()),
        ("route", draft.routes.iter().map(Route::id).collect()),
    ];
    for (kind, ids) in declared {
        let mut seen = BTreeSet::new();
        for id in ids {
            if !is_owned(&draft.id, id) {
                return Err(ManifestError::ForeignOwner {
                    kind,
                    id: id.as_str().to_owned(),
                });
            }
            if !seen.insert(id) {
                return Err(ManifestError::DuplicateDeclaration {
                    kind,
                    id: id.as_str().to_owned(),
                });
            }
        }
    }
    Ok(())
}

fn validate_provider_consistency(draft: &ManifestDraft) -> Result<(), ManifestError> {
    if draft.provider.mode() == ProviderMode::Persistent {
        return Ok(());
    }
    if !draft.provider.is_executable()
        && let Some(action) = draft.actions.first()
    {
        return Err(ManifestError::ProviderFreeDeclaresHandler {
            kind: "action",
            id: action.id().as_str().to_owned(),
        });
    }
    if let Some(panel) = draft.panels.first() {
        return if draft.provider.is_executable() {
            Err(ManifestError::PanelRequiresPersistentProvider {
                id: panel.id().as_str().to_owned(),
            })
        } else {
            Err(ManifestError::ProviderFreeDeclaresHandler {
                kind: "panel",
                id: panel.id().as_str().to_owned(),
            })
        };
    }
    Ok(())
}

/// Collect every contributed screen id, proving each is bound exactly once and
/// each descriptor path is contributed once.
fn validate_screens(draft: &ManifestDraft) -> Result<BTreeSet<&Id>, ManifestError> {
    let mut bound: BTreeSet<&Id> = BTreeSet::new();
    let mut paths: BTreeSet<&str> = BTreeSet::new();
    for contribution in &draft.screens {
        if contribution.screen_ids().len() != 1 {
            return Err(ManifestError::ScreenDescriptorCoverage {
                path: contribution.path().as_str().to_owned(),
                declared: contribution.screen_ids().len(),
            });
        }
        if !paths.insert(contribution.path().as_str()) {
            return Err(ManifestError::DuplicateScreenPath {
                path: contribution.path().as_str().to_owned(),
            });
        }
        for screen in contribution.screen_ids() {
            if !is_owned(&draft.id, screen) {
                return Err(ManifestError::ForeignOwner {
                    kind: "screen",
                    id: screen.as_str().to_owned(),
                });
            }
            if !bound.insert(screen) {
                return Err(ManifestError::ScreenBoundTwice {
                    id: screen.as_str().to_owned(),
                });
            }
        }
    }
    Ok(bound)
}

fn validate_routes(draft: &ManifestDraft, screens: &BTreeSet<&Id>) -> Result<(), ManifestError> {
    for route in &draft.routes {
        if !screens.contains(route.target_screen()) {
            return Err(ManifestError::UnresolvedRouteTarget {
                route: route.id().as_str().to_owned(),
                screen: route.target_screen().as_str().to_owned(),
            });
        }
    }
    Ok(())
}

fn validate_defaults(draft: &ManifestDraft, screens: &BTreeSet<&Id>) -> Result<(), ManifestError> {
    let Some(defaults) = &draft.defaults else {
        return Ok(());
    };
    for enabled in &defaults.actions_enabled {
        if !draft.actions.iter().any(|action| action.id() == enabled) {
            return Err(ManifestError::UnknownDefault {
                kind: "action",
                id: enabled.as_str().to_owned(),
            });
        }
    }
    for enabled in &defaults.screens_enabled {
        if !screens.contains(enabled) {
            return Err(ManifestError::UnknownDefault {
                kind: "screen",
                id: enabled.as_str().to_owned(),
            });
        }
    }
    for (key, value) in &defaults.config {
        let field = draft
            .config
            .as_ref()
            .and_then(|schema| schema.fields().iter().find(|field| field.id() == key))
            .ok_or_else(|| ManifestError::UnknownDefault {
                kind: "config field",
                id: key.as_str().to_owned(),
            })?;
        if !value_matches(value, field) {
            return Err(ManifestError::DefaultKindMismatch {
                field: key.as_str().to_owned(),
            });
        }
    }
    Ok(())
}

/// Whether a default value is legal for the field it configures.
///
/// The field itself owns the kind rules, so this reuses them by building the
/// same check the field applies to its own declared default rather than
/// restating the kind table here.
fn value_matches(value: &TypedValue, field: &super::field::Field) -> bool {
    super::field::Field::parse(super::field::FieldDraft {
        id: field.id().clone(),
        label: field.label().to_owned(),
        description: field.description().map(str::to_owned),
        kind: field.kind(),
        required: field.required(),
        default: Some(value.clone()),
        // The field's own bounds must travel with it: without them the
        // reconstructed draft skips the bounds check and a default outside the
        // field's declared range would be accepted.
        min: field.min().cloned(),
        max: field.max().cloned(),
        choices: field.choices().to_vec(),
        unique: field.unique(),
        visible_when: None,
        restart: field.restart(),
    })
    .is_ok()
}

/// Why a manifest is invalid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestError {
    /// The manifest schema is not the supported one.
    UnsupportedSchema { found: u32 },
    /// The provider protocol is not the supported one.
    UnsupportedProtocol { found: u32 },
    /// The display name is empty or only whitespace.
    BlankDisplayName,
    /// The host API minimum exceeds the maximum.
    InvertedHostApiRange,
    /// A declaration array exceeds its bound.
    TooManyDeclarations { kind: &'static str, len: usize },
    /// A declared id is not beneath the package's namespace.
    ForeignOwner { kind: &'static str, id: String },
    /// Two declarations of one kind share an id.
    DuplicateDeclaration { kind: &'static str, id: String },
    /// A package with no provider declared a handler.
    ProviderFreeDeclaresHandler { kind: &'static str, id: String },
    /// A panel was declared by a package whose provider is not persistent.
    PanelRequiresPersistentProvider { id: String },
    /// One descriptor file declares anything other than one screen identity.
    ScreenDescriptorCoverage { path: String, declared: usize },
    /// Two contributions declare the same descriptor path.
    DuplicateScreenPath { path: String },
    /// One screen id is bound by more than one contribution.
    ScreenBoundTwice { id: String },
    /// A route targets a screen the package does not contribute.
    UnresolvedRouteTarget { route: String, screen: String },
    /// A default enables something the manifest does not declare.
    UnknownDefault { kind: &'static str, id: String },
    /// A default configuration value does not match its field kind.
    DefaultKindMismatch { field: String },
}

impl fmt::Display for ManifestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedSchema { found } => write!(
                formatter,
                "manifest schema {found} is not the supported schema {MANIFEST_SCHEMA}"
            ),
            Self::UnsupportedProtocol { found } => write!(
                formatter,
                "protocol {found} is not the supported protocol {MANIFEST_PROTOCOL}"
            ),
            Self::BlankDisplayName => formatter.write_str("a display name may not be blank"),
            Self::InvertedHostApiRange => {
                formatter.write_str("the host API minimum exceeds the maximum")
            }
            Self::TooManyDeclarations { kind, len } => {
                write!(formatter, "{len} {kind} declarations exceeds the limit")
            }
            Self::ForeignOwner { kind, id } => write!(
                formatter,
                "{kind} {id:?} is not owned by this package's namespace"
            ),
            Self::DuplicateDeclaration { kind, id } => {
                write!(formatter, "{kind} {id:?} is declared twice")
            }
            Self::ProviderFreeDeclaresHandler { kind, id } => write!(
                formatter,
                "{kind} {id:?} declares a handler but the package declares no provider"
            ),
            Self::PanelRequiresPersistentProvider { id } => {
                write!(formatter, "panel {id:?} requires a persistent provider")
            }
            Self::ScreenDescriptorCoverage { path, declared } => write!(
                formatter,
                "screen descriptor {path:?} declares {declared} identities; exactly one is required"
            ),
            Self::DuplicateScreenPath { path } => {
                write!(formatter, "screen descriptor {path:?} is contributed twice")
            }
            Self::ScreenBoundTwice { id } => {
                write!(
                    formatter,
                    "screen {id:?} is bound by more than one descriptor"
                )
            }
            Self::UnresolvedRouteTarget { route, screen } => write!(
                formatter,
                "route {route:?} targets screen {screen:?}, which this package does not contribute"
            ),
            Self::UnknownDefault { kind, id } => write!(
                formatter,
                "default enables {kind} {id:?}, which this manifest does not declare"
            ),
            Self::DefaultKindMismatch { field } => write!(
                formatter,
                "default for config field {field:?} does not match its kind"
            ),
        }
    }
}

impl std::error::Error for ManifestError {}

#[cfg(test)]
#[path = "manifest_tests.rs"]
mod tests;
