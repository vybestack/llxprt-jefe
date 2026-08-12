//! Closed panel model and direct panel/migration payload DTOs
//! (issue #391).
//!
//! Pure data types only: no framing, parsing, process, state, effect, or
//! persistence. Each struct mirrors exactly the closed field set its wire
//! object admits; the readers in [`super::panel_reader`] enforce those sets
//! and the cross-field invariants (disabled affordances carry a reason,
//! referenced action ids resolve to declared affordances, progress totals
//! imply a completed count, and so on).
//!
//! These types replace the CW-10 `PanelSnapshot(TypedMap)` and
//! `MigratedConfig(TypedMap)` placeholders: panel and configuration migration
//! are now direct, strongly typed messages rather than opaque typed maps
//! carried inside `Outcome`.

use crate::domain::action_registry::ActionId;
use crate::domain::plugin::field::Field;
use crate::domain::{Id, TypedMap, TypedValue};

/// Why the host is deactivating a panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeactivateReason {
    /// The panel is suspended but may resume with a fresh generation.
    Suspend,
    /// The panel is disposed permanently.
    Dispose,
    /// The panel is being replaced by a new instance.
    Replace,
}

impl DeactivateReason {
    /// Every reason, in declaration order.
    pub const ALL: [Self; 3] = [Self::Suspend, Self::Dispose, Self::Replace];

    /// The lower-kebab-case wire name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Suspend => "suspend",
            Self::Dispose => "dispose",
            Self::Replace => "replace",
        }
    }

    /// Resolve a wire name, exactly and case-sensitively.
    #[must_use]
    pub fn from_wire(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|reason| reason.as_str() == value)
    }
}

/// Which body kind a panel snapshot carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyKind {
    /// A selectable list.
    List,
    /// A document with metadata and actions.
    Detail,
    /// An editable form.
    Form,
    /// A table of label/value rows.
    Status,
    /// A progress indicator, determinate when completed/total values are present.
    Progress,
    /// A message with an optional action.
    Empty,
    /// A recoverable error.
    Error,
}

impl BodyKind {
    /// Every kind, in declaration order.
    pub const ALL: [Self; 7] = [
        Self::List,
        Self::Detail,
        Self::Form,
        Self::Status,
        Self::Progress,
        Self::Empty,
        Self::Error,
    ];

    /// The lower-kebab-case wire name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
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
        Self::ALL.into_iter().find(|kind| kind.as_str() == value)
    }
}

/// One label/value metadata pair in a detail body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetailMetadata {
    /// The metadata label.
    pub label: String,
    /// The metadata value.
    pub value: String,
}

/// The state of one status row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusRowState {
    /// No special condition.
    Normal,
    /// A cautionary condition.
    Warning,
    /// A failing condition.
    Error,
}

impl StatusRowState {
    /// Every state, in declaration order.
    pub const ALL: [Self; 3] = [Self::Normal, Self::Warning, Self::Error];

    /// The lower-kebab-case wire name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }

    /// Resolve a wire name, exactly and case-sensitively.
    #[must_use]
    pub fn from_wire(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|state| state.as_str() == value)
    }
}

/// One row in a status body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusRow {
    /// The row label.
    pub label: String,
    /// The row value.
    pub value: String,
    /// The row state.
    pub state: StatusRowState,
}

/// One per-field error in a form body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormFieldError {
    /// The field the error applies to.
    pub field_id: Id,
    /// Why the field was rejected.
    pub message: String,
}

/// A declared action affordance on a panel.
///
/// A disabled affordance must carry a nonempty `unavailable_reason`; an enabled
/// affordance must not carry one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Affordance {
    /// The affordance identifier (unique within its snapshot).
    pub id: Id,
    /// Operator-facing label.
    pub label: String,
    /// The action this affordance triggers.
    pub action_id: ActionId,
    /// Fixed arguments, if any.
    pub arguments: Option<TypedMap>,
    /// Whether the affordance is currently usable.
    pub enabled: bool,
    /// Why the affordance is unavailable, required when disabled.
    pub unavailable_reason: Option<String>,
}

/// One selectable item in a list body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListItem {
    /// The item identifier (unique within its list).
    pub id: Id,
    /// Operator-facing label.
    pub label: String,
    /// Longer description, if any.
    pub description: Option<String>,
    /// A short status string, if any.
    pub status: Option<String>,
    /// Action affordance ids available on this item.
    pub actions: Vec<Id>,
}

/// A list body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListBody {
    /// Selectable items.
    pub items: Vec<ListItem>,
    /// The currently selected item, if any; must reference an existing item.
    pub selected_id: Option<Id>,
    /// A pagination cursor, if more pages exist.
    pub next_page_token: Option<String>,
}

/// A detail body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetailBody {
    /// The document text.
    pub document: String,
    /// Label/value metadata pairs.
    pub metadata: Vec<DetailMetadata>,
    /// Action affordance ids available on this detail.
    pub actions: Vec<Id>,
}

/// A form body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormBody {
    /// Declared form fields.
    pub fields: Vec<Field>,
    /// Current field values.
    pub values: TypedMap,
    /// Per-field validation errors.
    pub field_errors: Vec<FormFieldError>,
    /// The action that submits the form.
    pub submit_action: ActionId,
}

/// A status body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusBody {
    /// Label/value/state rows.
    pub rows: Vec<StatusRow>,
}

/// A progress body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgressBody {
    /// Operator-facing progress text.
    pub message: String,
    /// Optional completed count.
    pub completed: Option<u64>,
    /// Optional total count; presence implies `completed` and `completed <= total`.
    pub total: Option<u64>,
    /// Whether the operation can be cancelled.
    pub cancellable: bool,
}

/// An empty body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmptyBody {
    /// Operator-facing message.
    pub message: String,
    /// An optional action affordance id.
    pub action: Option<Id>,
}

/// An error body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorBody {
    /// Provider error code.
    pub code: String,
    /// Operator-facing message.
    pub message: String,
    /// Whether retrying might succeed.
    pub retryable: bool,
    /// An optional retry action affordance id.
    pub retry_action: Option<Id>,
}

/// The closed seven-kind panel body, tagged by `kind`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelBody {
    /// A selectable list.
    List(ListBody),
    /// A document with metadata and actions.
    Detail(DetailBody),
    /// An editable form.
    Form(FormBody),
    /// A table of label/value rows.
    Status(StatusBody),
    /// A progress indicator, determinate when completed/total values are present.
    Progress(ProgressBody),
    /// A message with an optional action.
    Empty(EmptyBody),
    /// A recoverable error.
    Error(ErrorBody),
}

impl PanelBody {
    /// The body kind tag this variant carries.
    #[must_use]
    pub const fn kind(&self) -> BodyKind {
        match self {
            Self::List(_) => BodyKind::List,
            Self::Detail(_) => BodyKind::Detail,
            Self::Form(_) => BodyKind::Form,
            Self::Status(_) => BodyKind::Status,
            Self::Progress(_) => BodyKind::Progress,
            Self::Empty(_) => BodyKind::Empty,
            Self::Error(_) => BodyKind::Error,
        }
    }
}

/// Bounded host-owned presentation state forwarded on activate/resume.
///
/// Exact closed shape: panels are host-rendered, so only the deterministic
/// presentation state a provider needs to resume travels on the wire.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostLocal {
    /// The focused affordance or item, if any.
    pub focus_target: Option<Id>,
    /// The current scroll offset.
    pub scroll_offset: u32,
    /// The currently selected id, if any.
    pub selected_id: Option<Id>,
    /// The in-progress form draft, if any.
    pub form_draft: Option<TypedMap>,
}

/// A provider-originated panel snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelSnapshot {
    /// The panel model schema version (must be `1`).
    pub model_schema: u64,
    /// The panel instance this snapshot belongs to.
    pub panel_instance_id: u64,
    /// The panel activation generation.
    pub generation: u64,
    /// The monotonically increasing snapshot revision.
    pub revision: u64,
    /// The body kind; must match `body`.
    pub kind: BodyKind,
    /// Snapshot title.
    pub title: String,
    /// Longer description, if any.
    pub description: Option<String>,
    /// Whether the panel is still loading.
    pub loading: bool,
    /// Declared action affordances.
    pub action_affordances: Vec<Affordance>,
    /// The typed body.
    pub body: PanelBody,
}

/// A `panel-event` payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelEventPayload {
    /// The panel instance the event targets.
    pub panel_instance_id: u64,
    /// The panel activation generation.
    pub generation: u64,
    /// The snapshot revision the event was raised against.
    pub revision: u64,
    /// The semantic event.
    pub event: PanelEvent,
}

/// A semantic panel input event, tagged by `kind`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelEvent {
    /// An item was selected.
    Selected {
        /// The selected item id.
        id: Id,
    },
    /// An item was activated (opened).
    Activated {
        /// The activated item id.
        id: Id,
    },
    /// An affordance action was triggered.
    Action {
        /// The affordance id.
        id: Id,
        /// Action arguments.
        arguments: TypedMap,
    },
    /// A form field value changed.
    FieldChanged {
        /// The changed field id.
        field_id: Id,
        /// The new value.
        value: TypedValue,
    },
    /// The form was submitted.
    Submit {
        /// The submitted values.
        values: TypedMap,
    },
    /// A new page was requested.
    PageRequested {
        /// The pagination token.
        token: String,
    },
    /// Retry was requested.
    Retry,
    /// Cancel was requested.
    Cancel,
    /// A link was selected.
    LinkSelected {
        /// The selected link id.
        link_id: Id,
    },
}

/// An `activate-panel` payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivatePanelPayload {
    /// The panel instance being activated.
    pub panel_instance_id: u64,
    /// The owning screen instance.
    pub screen_instance_id: u64,
    /// The owner-declared panel type.
    pub panel_type: Id,
    /// Activation parameters.
    pub activation: TypedMap,
    /// Prior bounded host-local state, if resuming.
    pub prior_host_local: Option<HostLocal>,
    /// The fresh panel activation generation.
    pub generation: u64,
}

/// A `deactivate-panel` payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeactivatePanelPayload {
    /// The panel instance being deactivated.
    pub panel_instance_id: u64,
    /// The panel activation generation.
    pub generation: u64,
    /// Why the panel is being deactivated.
    pub reason: DeactivateReason,
}

/// A `migrate-config` payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrateConfigPayload {
    /// The prior configuration schema version.
    pub from_version: u64,
    /// The target configuration schema version.
    pub to_version: u64,
    /// The exact prior typed configuration.
    pub config: TypedMap,
    /// The host-issued draft token.
    pub draft_token: u64,
}

/// A `migrated-config` direct response payload.
///
/// Despite travelling provider-to-host, this echoes the **same** host-originated
/// `migrate-config` request id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigratedConfigPayload {
    /// The prior configuration schema version (echoed).
    pub from_version: u64,
    /// The target configuration schema version (echoed).
    pub to_version: u64,
    /// The exact prior typed configuration (echoed).
    pub config: TypedMap,
    /// The host-issued draft token (echoed).
    pub draft_token: u64,
    /// The proposed target configuration.
    pub target_config: TypedMap,
    /// Display-only migration notes.
    pub notes: Vec<String>,
}
