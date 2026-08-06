//! Panel, route, screen and configuration declarations
//! (issue #389 CW-09, acceptance rows D4 and D5).
//!
//! These are the surfaces a package contributes to the workbench. Each is
//! validated on construction against everything visible from its own
//! declaration; rules that need the whole manifest — owner prefixes, resolving
//! a route's target screen, binding each contributed screen exactly once — stay
//! with manifest validation.
//!
//! The one exception is [`ConfigSchema`], which owns the complete field set of
//! a package's configuration. Because a `visible_when` reference always names a
//! *sibling* field, the schema is the smallest scope that can resolve one, so
//! reference resolution and the acyclic-visibility check live here.

use std::collections::BTreeMap;
use std::fmt;

use super::field::Field;
use super::limits::{
    CONFIG_FIELD_LIMIT, CONFIG_SCHEMA_VERSION_MINIMUM, PANEL_MODEL_KIND_MINIMUM, PANEL_PORT_LIMIT,
    ROUTE_ACTIVATION_FIELD_LIMIT, SCREEN_ID_LIMIT, SCREEN_ID_MINIMUM,
};
use super::values::RelativePath;
use crate::domain::Id;

/// Shapes of model a panel can render.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ModelKind {
    /// A list of rows.
    List,
    /// A detail view of one resource.
    Detail,
    /// An editable form.
    Form,
    /// A status summary.
    Status,
    /// Progress of a running operation.
    Progress,
    /// Nothing to show.
    Empty,
    /// A failure.
    Error,
}

impl ModelKind {
    /// Every model kind, in declaration order.
    pub const ALL: [Self; 7] = [
        Self::List,
        Self::Detail,
        Self::Form,
        Self::Status,
        Self::Progress,
        Self::Empty,
        Self::Error,
    ];

    /// The lower-kebab-case name used on the wire.
    #[must_use]
    pub const fn as_wire(self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Detail => "detail",
            Self::Form => "form",
            Self::Status => "status",
            Self::Progress => "progress",
            Self::Empty => "empty",
            Self::Error => "error",
        }
    }

    /// Resolve a wire name, exactly and case-sensitively.
    #[must_use]
    pub fn from_wire(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_wire() == value)
    }
}

/// Events a panel can receive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EventKind {
    /// A row was selected.
    Selected,
    /// A row was activated.
    Activated,
    /// An action was invoked.
    Action,
    /// A form field changed.
    FieldChanged,
    /// A form was submitted.
    Submit,
    /// Another page was requested.
    PageRequested,
    /// A failed operation was retried.
    Retry,
    /// The operation was cancelled.
    Cancel,
    /// A link was followed.
    LinkSelected,
}

impl EventKind {
    /// Every event kind, in declaration order.
    pub const ALL: [Self; 9] = [
        Self::Selected,
        Self::Activated,
        Self::Action,
        Self::FieldChanged,
        Self::Submit,
        Self::PageRequested,
        Self::Retry,
        Self::Cancel,
        Self::LinkSelected,
    ];

    /// The lower-kebab-case name used on the wire.
    #[must_use]
    pub const fn as_wire(self) -> &'static str {
        match self {
            Self::Selected => "selected",
            Self::Activated => "activated",
            Self::Action => "action",
            Self::FieldChanged => "field-changed",
            Self::Submit => "submit",
            Self::PageRequested => "page-requested",
            Self::Retry => "retry",
            Self::Cancel => "cancel",
            Self::LinkSelected => "link-selected",
        }
    }

    /// Resolve a wire name, exactly and case-sensitively.
    #[must_use]
    pub fn from_wire(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_wire() == value)
    }
}

/// A named data channel a panel exposes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Port {
    id: Id,
}

impl Port {
    /// Name a port.
    #[must_use]
    pub const fn new(id: Id) -> Self {
        Self { id }
    }

    /// The port identifier.
    #[must_use]
    pub const fn id(&self) -> &Id {
        &self.id
    }
}

/// An unvalidated panel declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelDraft {
    /// Panel identifier, owned by the declaring package.
    pub id: Id,
    /// Model shapes the panel renders.
    pub model_kinds: Vec<ModelKind>,
    /// Events the panel accepts.
    pub event_kinds: Vec<EventKind>,
    /// Provider-side handler name.
    pub handler: Id,
    /// Data channels the panel exposes.
    pub ports: Vec<Port>,
}

/// A validated panel declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Panel {
    draft: PanelDraft,
}

impl Panel {
    /// Validate a panel declaration.
    ///
    /// # Errors
    ///
    /// Returns [`PanelError`] when no model kind is declared, a kind or port
    /// repeats, or the port bound is exceeded.
    pub fn parse(draft: PanelDraft) -> Result<Self, PanelError> {
        if draft.model_kinds.len() < PANEL_MODEL_KIND_MINIMUM {
            return Err(PanelError::NoModelKinds);
        }
        for (index, kind) in draft.model_kinds.iter().enumerate() {
            if draft.model_kinds[..index].contains(kind) {
                return Err(PanelError::DuplicateModelKind {
                    kind: kind.as_wire().to_owned(),
                });
            }
        }
        for (index, kind) in draft.event_kinds.iter().enumerate() {
            if draft.event_kinds[..index].contains(kind) {
                return Err(PanelError::DuplicateEventKind {
                    kind: kind.as_wire().to_owned(),
                });
            }
        }
        if draft.ports.len() > PANEL_PORT_LIMIT {
            return Err(PanelError::TooManyPorts {
                len: draft.ports.len(),
            });
        }
        for (index, port) in draft.ports.iter().enumerate() {
            if draft.ports[..index]
                .iter()
                .any(|earlier| earlier.id() == port.id())
            {
                return Err(PanelError::DuplicatePort {
                    id: port.id().as_str().to_owned(),
                });
            }
        }
        Ok(Self { draft })
    }

    /// The panel identifier.
    #[must_use]
    pub const fn id(&self) -> &Id {
        &self.draft.id
    }

    /// Model shapes the panel renders.
    #[must_use]
    pub fn model_kinds(&self) -> &[ModelKind] {
        &self.draft.model_kinds
    }

    /// Events the panel accepts.
    #[must_use]
    pub fn event_kinds(&self) -> &[EventKind] {
        &self.draft.event_kinds
    }

    /// The provider-side handler name.
    #[must_use]
    pub const fn handler(&self) -> &Id {
        &self.draft.handler
    }

    /// Data channels the panel exposes.
    #[must_use]
    pub fn ports(&self) -> &[Port] {
        &self.draft.ports
    }
}

/// Why a panel declaration is invalid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelError {
    /// No model kind was declared.
    NoModelKinds,
    /// The same model kind was declared twice.
    DuplicateModelKind { kind: String },
    /// The same event kind was declared twice.
    DuplicateEventKind { kind: String },
    /// More than [`PANEL_PORT_LIMIT`] ports.
    TooManyPorts { len: usize },
    /// Two ports share an identifier.
    DuplicatePort { id: String },
}

impl fmt::Display for PanelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoModelKinds => {
                formatter.write_str("a panel must declare at least one model kind")
            }
            Self::DuplicateModelKind { kind } => {
                write!(formatter, "model kind {kind:?} is declared twice")
            }
            Self::DuplicateEventKind { kind } => {
                write!(formatter, "event kind {kind:?} is declared twice")
            }
            Self::TooManyPorts { len } => {
                write!(
                    formatter,
                    "{len} ports exceeds the {PANEL_PORT_LIMIT} limit"
                )
            }
            Self::DuplicatePort { id } => write!(formatter, "port {id:?} is declared twice"),
        }
    }
}

impl std::error::Error for PanelError {}

/// An unvalidated route declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteDraft {
    /// Route identifier, owned by the declaring package.
    pub id: Id,
    /// Fields collected to activate the route.
    pub activation_fields: Vec<Field>,
    /// Screen the route opens.
    pub target_screen: Id,
}

/// A validated route declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    draft: RouteDraft,
}

impl Route {
    /// Validate a route declaration.
    ///
    /// # Errors
    ///
    /// Returns [`RouteError`] when the activation-field bound is exceeded or an
    /// activation field identifier repeats.
    pub fn parse(draft: RouteDraft) -> Result<Self, RouteError> {
        if draft.activation_fields.len() > ROUTE_ACTIVATION_FIELD_LIMIT {
            return Err(RouteError::TooManyActivationFields {
                len: draft.activation_fields.len(),
            });
        }
        for (index, activation) in draft.activation_fields.iter().enumerate() {
            if draft.activation_fields[..index]
                .iter()
                .any(|earlier| earlier.id() == activation.id())
            {
                return Err(RouteError::DuplicateActivationField {
                    id: activation.id().as_str().to_owned(),
                });
            }
        }
        Ok(Self { draft })
    }

    /// The route identifier.
    #[must_use]
    pub const fn id(&self) -> &Id {
        &self.draft.id
    }

    /// Fields collected to activate the route.
    #[must_use]
    pub fn activation_fields(&self) -> &[Field] {
        &self.draft.activation_fields
    }

    /// The screen this route opens.
    #[must_use]
    pub const fn target_screen(&self) -> &Id {
        &self.draft.target_screen
    }
}

/// Why a route declaration is invalid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteError {
    /// More than [`ROUTE_ACTIVATION_FIELD_LIMIT`] activation fields.
    TooManyActivationFields { len: usize },
    /// Two activation fields share an identifier.
    DuplicateActivationField { id: String },
}

impl fmt::Display for RouteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyActivationFields { len } => write!(
                formatter,
                "{len} activation fields exceeds the {ROUTE_ACTIVATION_FIELD_LIMIT} limit"
            ),
            Self::DuplicateActivationField { id } => {
                write!(formatter, "activation field {id:?} is declared twice")
            }
        }
    }
}

impl std::error::Error for RouteError {}

/// A screen descriptor file a package contributes, and the screens it binds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenContribution {
    path: RelativePath,
    screen_ids: Vec<Id>,
}

impl ScreenContribution {
    /// Validate a screen contribution.
    ///
    /// # Errors
    ///
    /// Returns [`ScreenContributionError`] when no screen is bound, the bound
    /// is exceeded, or a screen identifier repeats.
    pub fn parse(path: RelativePath, screen_ids: Vec<Id>) -> Result<Self, ScreenContributionError> {
        if screen_ids.len() < SCREEN_ID_MINIMUM {
            return Err(ScreenContributionError::NoScreenIds);
        }
        if screen_ids.len() > SCREEN_ID_LIMIT {
            return Err(ScreenContributionError::TooManyScreenIds {
                len: screen_ids.len(),
            });
        }
        for (index, screen) in screen_ids.iter().enumerate() {
            if screen_ids[..index].contains(screen) {
                return Err(ScreenContributionError::DuplicateScreenId {
                    id: screen.as_str().to_owned(),
                });
            }
        }
        Ok(Self { path, screen_ids })
    }

    /// The descriptor file, relative to the package directory.
    #[must_use]
    pub const fn path(&self) -> &RelativePath {
        &self.path
    }

    /// The screens this file binds.
    #[must_use]
    pub fn screen_ids(&self) -> &[Id] {
        &self.screen_ids
    }
}

/// Why a screen contribution is invalid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScreenContributionError {
    /// The contribution binds no screen.
    NoScreenIds,
    /// More than [`SCREEN_ID_LIMIT`] screens.
    TooManyScreenIds { len: usize },
    /// The same screen was bound twice.
    DuplicateScreenId { id: String },
}

impl fmt::Display for ScreenContributionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoScreenIds => {
                formatter.write_str("a screen contribution must bind at least one screen")
            }
            Self::TooManyScreenIds { len } => {
                write!(
                    formatter,
                    "{len} screens exceeds the {SCREEN_ID_LIMIT} limit"
                )
            }
            Self::DuplicateScreenId { id } => write!(formatter, "screen {id:?} is bound twice"),
        }
    }
}

impl std::error::Error for ScreenContributionError {}

/// A package's configuration schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigSchema {
    schema_version: u32,
    fields: Vec<Field>,
}

impl ConfigSchema {
    /// Validate a configuration schema.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigSchemaError`] when the version is below
    /// [`CONFIG_SCHEMA_VERSION_MINIMUM`], the field bound is exceeded, a field
    /// identifier repeats, a `visible_when` reference names no sibling, or the
    /// visibility graph contains a cycle.
    pub fn parse(schema_version: u32, fields: Vec<Field>) -> Result<Self, ConfigSchemaError> {
        if schema_version < CONFIG_SCHEMA_VERSION_MINIMUM {
            return Err(ConfigSchemaError::VersionTooLow {
                version: schema_version,
            });
        }
        if fields.len() > CONFIG_FIELD_LIMIT {
            return Err(ConfigSchemaError::TooManyFields { len: fields.len() });
        }
        let gates = index_fields(&fields)?;
        check_visibility(&fields, &gates)?;
        Ok(Self {
            schema_version,
            fields,
        })
    }

    /// The declared schema version.
    #[must_use]
    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    /// The declared fields.
    #[must_use]
    pub fn fields(&self) -> &[Field] {
        &self.fields
    }
}

/// Map each field identifier to the sibling that gates it, rejecting
/// duplicates on the way.
fn index_fields(fields: &[Field]) -> Result<BTreeMap<&Id, Option<&Id>>, ConfigSchemaError> {
    let mut gates = BTreeMap::new();
    for field in fields {
        if gates.insert(field.id(), field.visible_when()).is_some() {
            return Err(ConfigSchemaError::DuplicateField {
                id: field.id().as_str().to_owned(),
            });
        }
    }
    Ok(gates)
}

/// Resolve every visibility reference and prove the graph is acyclic.
///
/// A cycle would make a field's visibility depend on itself, so no consistent
/// rendering exists and the schema is rejected rather than resolved
/// arbitrarily.
fn check_visibility(
    fields: &[Field],
    gates: &BTreeMap<&Id, Option<&Id>>,
) -> Result<(), ConfigSchemaError> {
    for field in fields {
        let Some(reference) = field.visible_when() else {
            continue;
        };
        if !gates.contains_key(reference) {
            return Err(ConfigSchemaError::UnresolvedVisibility {
                field: field.id().as_str().to_owned(),
                references: reference.as_str().to_owned(),
            });
        }
        let mut path = vec![field.id().as_str().to_owned()];
        let mut cursor = reference;
        // Each step consumes one distinct field, so the walk cannot exceed the
        // field count without revisiting a node already on the path.
        for _ in 0..fields.len() {
            if path.iter().any(|seen| seen == cursor.as_str()) {
                path.push(cursor.as_str().to_owned());
                return Err(ConfigSchemaError::VisibilityCycle { path });
            }
            path.push(cursor.as_str().to_owned());
            match gates.get(cursor).copied().flatten() {
                Some(next) => cursor = next,
                None => break,
            }
        }
    }
    Ok(())
}

/// Why a configuration schema is invalid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigSchemaError {
    /// The schema version is below the minimum.
    VersionTooLow { version: u32 },
    /// More than [`CONFIG_FIELD_LIMIT`] fields.
    TooManyFields { len: usize },
    /// Two fields share an identifier.
    DuplicateField { id: String },
    /// A `visible_when` reference names no sibling field.
    UnresolvedVisibility { field: String, references: String },
    /// The visibility graph contains a cycle.
    VisibilityCycle { path: Vec<String> },
}

impl fmt::Display for ConfigSchemaError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::VersionTooLow { version } => write!(
                formatter,
                "config schema version {version} is below {CONFIG_SCHEMA_VERSION_MINIMUM}"
            ),
            Self::TooManyFields { len } => {
                write!(
                    formatter,
                    "{len} fields exceeds the {CONFIG_FIELD_LIMIT} limit"
                )
            }
            Self::DuplicateField { id } => write!(formatter, "field {id:?} is declared twice"),
            Self::UnresolvedVisibility { field, references } => write!(
                formatter,
                "field {field:?} is gated by {references:?}, which is not a sibling field"
            ),
            Self::VisibilityCycle { path } => {
                write!(formatter, "visibility cycle: {}", path.join(" -> "))
            }
        }
    }
}

impl std::error::Error for ConfigSchemaError {}

#[cfg(test)]
#[path = "surface_tests.rs"]
mod tests;
