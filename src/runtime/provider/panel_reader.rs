//! Closed panel-model and direct panel/migration payload decoding
//! (issue #391).
//!
//! These readers map the shared bounded reader's ordered tree onto the closed
//! panel model in [`super::panel_model`] and the six new direct payloads. Every
//! object lists exactly the keys it admits; any other key is an
//! unknown-field fault, and duplicate keys were already rejected by the shared
//! reader at every nesting level. Cross-field invariants — a disabled
//! affordance carries a reason, snapshot kind matches its body, referenced
//! action ids resolve to declared affordances, progress totals imply a
//! completed count, selected list ids exist — are enforced here, after
//! structural mapping and before the typed DTO is returned.
//!
//! No process, application state, effect, or persistence lives here.

use std::collections::HashSet;

use crate::domain::Id;
use crate::domain::action_registry::ActionId;
use crate::domain::bounded_json::BoundedJson;

use super::error::ProviderError;
use super::object_reader::{
    array, closed_object, find, read_bool, read_enum, read_id, read_string, read_u64, require,
    type_mismatch,
};
use super::panel_model::{
    ActivatePanelPayload, Affordance, BodyKind, DeactivatePanelPayload, DeactivateReason,
    DetailBody, DetailMetadata, DiffLineOrigin, EmptyBody, ErrorBody, FormBody, FormFieldError,
    HostLocal, ListBody, ListItem, MigrateConfigPayload, MigratedConfigPayload, PanelBody,
    PanelEvent, PanelEventPayload, PanelSnapshot, ProgressBody, StatusBody, StatusRow,
    StatusRowState, StructuredDiffBody, StructuredDiffFile, StructuredDiffHunk, StructuredDiffLine,
    TreeBody, TreeNode,
};
use super::typed_value::{read_field_declaration, read_typed_map, read_typed_value};

/// The single closed panel model schema version this layer accepts.
const PANEL_MODEL_SCHEMA: u64 = 1;

/// Maximum action affordances one snapshot may declare.
const AFFORDANCE_LIMIT: usize = 64;

/// Maximum list items one list body may carry.
const LIST_ITEM_LIMIT: usize = 1000;

/// Maximum action ids on one list item.
const LIST_ITEM_ACTION_LIMIT: usize = 64;

/// Maximum nodes in one tree body.
const TREE_NODE_LIMIT: usize = 1000;

/// Maximum files in one structured diff body.
const DIFF_FILE_LIMIT: usize = 256;

/// Maximum hunks in one structured diff file.
const DIFF_HUNK_LIMIT: usize = 1024;

/// Maximum lines in one structured diff hunk.
const DIFF_LINE_LIMIT: usize = 1024;

/// Maximum UTF-8 bytes in one structured diff line.
const DIFF_LINE_BYTE_LIMIT: usize = 262_144;

/// Maximum UTF-8 bytes in a detail document.
const DETAIL_DOCUMENT_BYTE_LIMIT: usize = 262_144;

/// Maximum metadata pairs in a detail body.
const DETAIL_METADATA_LIMIT: usize = 256;

/// Global wire array bound (matches the framing `WIRE_ARRAY_ELEMENTS`).
const GLOBAL_ARRAY_LIMIT: usize = 1024;

/// Maximum form fields in a form body.
const FORM_FIELD_LIMIT: usize = 128;

/// Maximum form field errors in a form body.
const FORM_FIELD_ERROR_LIMIT: usize = 128;

/// Maximum status rows in a status body.
const STATUS_ROW_LIMIT: usize = 256;

/// Maximum migration notes in a migrated-config response.
const MIGRATED_NOTES_LIMIT: usize = 64;

// Closed field tables. Every payload object lists exactly the keys it admits.
const ACTIVATE_PANEL_KEYS: [&str; 6] = [
    "panel_instance_id",
    "screen_instance_id",
    "panel_type",
    "activation",
    "prior_host_local",
    "generation",
];
const DEACTIVATE_PANEL_KEYS: [&str; 3] = ["panel_instance_id", "generation", "reason"];
const PANEL_EVENT_KEYS: [&str; 4] = ["panel_instance_id", "generation", "revision", "event"];
const PANEL_SNAPSHOT_KEYS: [&str; 10] = [
    "model_schema",
    "panel_instance_id",
    "generation",
    "revision",
    "kind",
    "title",
    "description",
    "loading",
    "action_affordances",
    "body",
];
const MIGRATE_CONFIG_KEYS: [&str; 4] = ["from_version", "to_version", "config", "draft_token"];
const MIGRATED_CONFIG_KEYS: [&str; 6] = [
    "from_version",
    "to_version",
    "config",
    "draft_token",
    "target_config",
    "notes",
];
const HOST_LOCAL_KEYS: [&str; 4] = ["focus_target", "scroll_offset", "selected_id", "form_draft"];
const AFFORDANCE_KEYS: [&str; 6] = [
    "id",
    "label",
    "action_id",
    "arguments",
    "enabled",
    "unavailable_reason",
];
const LIST_ITEM_KEYS: [&str; 5] = ["id", "label", "description", "status", "actions"];
const LIST_BODY_KEYS: [&str; 4] = ["kind", "items", "selected_id", "next_page_token"];
const TREE_BODY_KEYS: [&str; 4] = ["kind", "schema_version", "nodes", "selected_id"];
const TREE_NODE_KEYS: [&str; 7] = [
    "id",
    "parent_id",
    "label",
    "semantic_key",
    "depth",
    "expandable",
    "expanded",
];
const STRUCTURED_DIFF_BODY_KEYS: [&str; 4] =
    ["kind", "schema_version", "files", "selected_file_id"];
const STRUCTURED_DIFF_FILE_KEYS: [&str; 7] = [
    "id", "old_path", "new_path", "old_mode", "new_mode", "binary", "hunks",
];
const STRUCTURED_DIFF_HUNK_KEYS: [&str; 6] = [
    "header",
    "old_start",
    "old_lines",
    "new_start",
    "new_lines",
    "lines",
];
const STRUCTURED_DIFF_LINE_KEYS: [&str; 5] =
    ["origin", "old_line", "new_line", "content", "no_newline"];
const DETAIL_BODY_KEYS: [&str; 4] = ["kind", "document", "metadata", "actions"];
const DETAIL_METADATA_KEYS: [&str; 2] = ["label", "value"];
const FORM_BODY_KEYS: [&str; 5] = ["kind", "fields", "values", "field_errors", "submit_action"];
const FORM_FIELD_ERROR_KEYS: [&str; 2] = ["field_id", "message"];
const STATUS_BODY_KEYS: [&str; 2] = ["kind", "rows"];
const STATUS_ROW_KEYS: [&str; 3] = ["label", "value", "state"];
const PROGRESS_BODY_KEYS: [&str; 5] = ["kind", "message", "completed", "total", "cancellable"];
const EMPTY_BODY_KEYS: [&str; 3] = ["kind", "message", "action"];
const ERROR_BODY_KEYS: [&str; 5] = ["kind", "code", "message", "retryable", "retry_action"];

// --- top-level direct payload readers --------------------------------------

/// Read an `activate-panel` payload.
pub(super) fn read_activate_panel(
    payload: &BoundedJson,
) -> Result<ActivatePanelPayload, ProviderError> {
    let members = closed_object(payload, "activate-panel", &ACTIVATE_PANEL_KEYS)?;
    let prior_host_local = match find(members, "prior_host_local") {
        Some(value) => Some(read_host_local(value)?),
        None => None,
    };
    Ok(ActivatePanelPayload {
        panel_instance_id: read_positive_u64(members, "activate-panel", "panel_instance_id")?,
        screen_instance_id: read_positive_u64(members, "activate-panel", "screen_instance_id")?,
        panel_type: read_id(members, "activate-panel", "panel_type")?,
        activation: read_typed_map(
            require(members, "activate-panel", "activation")?,
            "activate-panel.activation",
        )?,
        prior_host_local,
        generation: read_positive_u64(members, "activate-panel", "generation")?,
    })
}

/// Read a `deactivate-panel` payload.
pub(super) fn read_deactivate_panel(
    payload: &BoundedJson,
) -> Result<DeactivatePanelPayload, ProviderError> {
    let members = closed_object(payload, "deactivate-panel", &DEACTIVATE_PANEL_KEYS)?;
    Ok(DeactivatePanelPayload {
        panel_instance_id: read_positive_u64(members, "deactivate-panel", "panel_instance_id")?,
        generation: read_positive_u64(members, "deactivate-panel", "generation")?,
        reason: read_enum(
            members,
            "deactivate-panel",
            "reason",
            DeactivateReason::from_wire,
        )?,
    })
}

/// Read a `panel-event` payload.
pub(super) fn read_panel_event(payload: &BoundedJson) -> Result<PanelEventPayload, ProviderError> {
    let members = closed_object(payload, "panel-event", &PANEL_EVENT_KEYS)?;
    Ok(PanelEventPayload {
        panel_instance_id: read_positive_u64(members, "panel-event", "panel_instance_id")?,
        generation: read_positive_u64(members, "panel-event", "generation")?,
        revision: read_positive_u64(members, "panel-event", "revision")?,
        event: read_panel_event_value(
            require(members, "panel-event", "event")?,
            "panel-event.event",
        )?,
    })
}

/// Read a `panel-snapshot` payload.
pub(super) fn read_panel_snapshot(payload: &BoundedJson) -> Result<PanelSnapshot, ProviderError> {
    let members = closed_object(payload, "panel-snapshot", &PANEL_SNAPSHOT_KEYS)?;
    let model_schema = read_u64(members, "panel-snapshot", "model_schema")?;
    if model_schema != PANEL_MODEL_SCHEMA {
        return Err(ProviderError::InvalidValue {
            path: "panel-snapshot.model_schema".to_owned(),
            reason: format!(
                "model_schema {model_schema} is not the supported version {PANEL_MODEL_SCHEMA}"
            ),
        });
    }
    let kind = read_enum(members, "panel-snapshot", "kind", BodyKind::from_wire)?;
    let description = read_optional_string(members, "panel-snapshot", "description")?;
    let affordances =
        read_affordance_array(require(members, "panel-snapshot", "action_affordances")?)?;
    let body = read_panel_body(require(members, "panel-snapshot", "body")?, kind)?;
    let snapshot = PanelSnapshot {
        model_schema,
        panel_instance_id: read_positive_u64(members, "panel-snapshot", "panel_instance_id")?,
        generation: read_positive_u64(members, "panel-snapshot", "generation")?,
        revision: read_positive_u64(members, "panel-snapshot", "revision")?,
        kind,
        title: read_string(members, "panel-snapshot", "title")?.to_owned(),
        description,
        loading: read_bool(members, "panel-snapshot", "loading")?,
        action_affordances: affordances,
        body,
    };
    validate_snapshot(&snapshot)?;
    Ok(snapshot)
}

/// Read a `migrate-config` payload.
pub(super) fn read_migrate_config(
    payload: &BoundedJson,
) -> Result<MigrateConfigPayload, ProviderError> {
    let members = closed_object(payload, "migrate-config", &MIGRATE_CONFIG_KEYS)?;
    Ok(MigrateConfigPayload {
        from_version: read_positive_u64(members, "migrate-config", "from_version")?,
        to_version: read_positive_u64(members, "migrate-config", "to_version")?,
        config: read_typed_map(
            require(members, "migrate-config", "config")?,
            "migrate-config.config",
        )?,
        draft_token: read_positive_u64(members, "migrate-config", "draft_token")?,
    })
}

/// Read a `migrated-config` direct response payload.
pub(super) fn read_migrated_config(
    payload: &BoundedJson,
) -> Result<MigratedConfigPayload, ProviderError> {
    let members = closed_object(payload, "migrated-config", &MIGRATED_CONFIG_KEYS)?;
    let notes = read_string_array(
        require(members, "migrated-config", "notes")?,
        "migrated-config.notes",
    )?;
    Ok(MigratedConfigPayload {
        from_version: read_positive_u64(members, "migrated-config", "from_version")?,
        to_version: read_positive_u64(members, "migrated-config", "to_version")?,
        config: read_typed_map(
            require(members, "migrated-config", "config")?,
            "migrated-config.config",
        )?,
        draft_token: read_positive_u64(members, "migrated-config", "draft_token")?,
        target_config: read_typed_map(
            require(members, "migrated-config", "target_config")?,
            "migrated-config.target_config",
        )?,
        notes,
    })
}

// --- host-local and event readers ------------------------------------------

/// Read a `prior_host_local` object.
fn read_host_local(value: &BoundedJson) -> Result<HostLocal, ProviderError> {
    let members = closed_object(value, "activate-panel.prior_host_local", &HOST_LOCAL_KEYS)?;
    Ok(HostLocal {
        focus_target: read_optional_id(members, "activate-panel.prior_host_local", "focus_target")?,
        scroll_offset: read_u32_field(members, "activate-panel.prior_host_local", "scroll_offset")?,
        selected_id: read_optional_id(members, "activate-panel.prior_host_local", "selected_id")?,
        form_draft: match find(members, "form_draft") {
            Some(entry) => Some(read_typed_map(
                entry,
                "activate-panel.prior_host_local.form_draft",
            )?),
            None => None,
        },
    })
}

/// Read a closed tagged `PanelEvent` `{kind, ...}`.
fn read_panel_event_value(value: &BoundedJson, path: &str) -> Result<PanelEvent, ProviderError> {
    let members = value
        .as_object()
        .ok_or_else(|| type_mismatch(path, "object"))?;
    let kind = read_string(members, path, "kind")?;
    match kind {
        "selected" => read_event_id(value, path, "selected", |id| PanelEvent::Selected { id }),
        "activated" => read_event_id(value, path, "activated", |id| PanelEvent::Activated { id }),
        "action" => read_event_action(value, path),
        "field-changed" => read_event_field_changed(value, path),
        "submit" => read_event_submit(value, path),
        "page-requested" => read_event_page_requested(value, path),
        "retry" => {
            closed_object(value, path, &["kind"])?;
            Ok(PanelEvent::Retry)
        }
        "cancel" => {
            closed_object(value, path, &["kind"])?;
            Ok(PanelEvent::Cancel)
        }
        "link-selected" => read_event_link_selected(value, path),
        "expansion-changed" => read_event_expansion_changed(value, path),
        other => Err(ProviderError::UnknownValue {
            path: format!("{path}.kind"),
            value: other.to_owned(),
        }),
    }
}

fn read_event_id(
    value: &BoundedJson,
    path: &str,
    kind_name: &'static str,
    ctor: impl Fn(Id) -> PanelEvent,
) -> Result<PanelEvent, ProviderError> {
    let members = closed_object(value, path, &["kind", "id"])?;
    let id = read_id(members, path, "id")?;
    // Prove the kind tag matches the branch (defence against a mismatched tag).
    verify_kind(members, path, kind_name)?;
    Ok(ctor(id))
}

fn read_event_action(value: &BoundedJson, path: &str) -> Result<PanelEvent, ProviderError> {
    let members = closed_object(value, path, &["kind", "id", "arguments"])?;
    verify_kind(members, path, "action")?;
    Ok(PanelEvent::Action {
        id: read_id(members, path, "id")?,
        arguments: read_typed_map(
            require(members, path, "arguments")?,
            &format!("{path}.arguments"),
        )?,
    })
}

fn read_event_field_changed(value: &BoundedJson, path: &str) -> Result<PanelEvent, ProviderError> {
    let members = closed_object(value, path, &["kind", "field_id", "value"])?;
    verify_kind(members, path, "field-changed")?;
    Ok(PanelEvent::FieldChanged {
        field_id: read_id(members, path, "field_id")?,
        value: read_typed_value(require(members, path, "value")?, &format!("{path}.value"))?,
    })
}

fn read_event_submit(value: &BoundedJson, path: &str) -> Result<PanelEvent, ProviderError> {
    let members = closed_object(value, path, &["kind", "values"])?;
    verify_kind(members, path, "submit")?;
    Ok(PanelEvent::Submit {
        values: read_typed_map(require(members, path, "values")?, &format!("{path}.values"))?,
    })
}

fn read_event_page_requested(value: &BoundedJson, path: &str) -> Result<PanelEvent, ProviderError> {
    let members = closed_object(value, path, &["kind", "token"])?;
    verify_kind(members, path, "page-requested")?;
    Ok(PanelEvent::PageRequested {
        token: read_string(members, path, "token")?.to_owned(),
    })
}

fn read_event_link_selected(value: &BoundedJson, path: &str) -> Result<PanelEvent, ProviderError> {
    let members = closed_object(value, path, &["kind", "link_id"])?;
    verify_kind(members, path, "link-selected")?;
    Ok(PanelEvent::LinkSelected {
        link_id: read_id(members, path, "link_id")?,
    })
}

fn read_event_expansion_changed(
    value: &BoundedJson,
    path: &str,
) -> Result<PanelEvent, ProviderError> {
    let members = closed_object(value, path, &["kind", "id", "expanded"])?;
    verify_kind(members, path, "expansion-changed")?;
    Ok(PanelEvent::ExpansionChanged {
        id: read_id(members, path, "id")?,
        expanded: read_bool(members, path, "expanded")?,
    })
}

/// Confirm the `kind` tag equals the expected branch name.
fn verify_kind(
    members: &[(String, BoundedJson)],
    path: &str,
    expected: &str,
) -> Result<(), ProviderError> {
    let actual = read_string(members, path, "kind")?;
    if actual == expected {
        Ok(())
    } else {
        Err(ProviderError::InvalidValue {
            path: format!("{path}.kind"),
            reason: format!("kind {actual:?} does not match the {expected:?} body"),
        })
    }
}

// --- body readers ----------------------------------------------------------

/// Read the typed panel body, dispatching on the snapshot's declared kind.
fn read_panel_body(value: &BoundedJson, kind: BodyKind) -> Result<PanelBody, ProviderError> {
    let members = value
        .as_object()
        .ok_or_else(|| ProviderError::TypeMismatch {
            path: "panel-snapshot.body".to_owned(),
            expected: "object",
        })?;
    let body_kind = read_enum(members, "panel-snapshot.body", "kind", BodyKind::from_wire)?;
    if body_kind != kind {
        return Err(ProviderError::InvalidValue {
            path: "panel-snapshot.body.kind".to_owned(),
            reason: format!("must match panel-snapshot.kind {kind:?}"),
        });
    }
    let body = match kind {
        BodyKind::List => PanelBody::List(read_list_body(value)?),
        BodyKind::Tree => PanelBody::Tree(read_tree_body(value)?),
        BodyKind::Detail => PanelBody::Detail(read_detail_body(value)?),
        BodyKind::StructuredDiff => PanelBody::StructuredDiff(read_structured_diff_body(value)?),
        BodyKind::Form => PanelBody::Form(read_form_body(value)?),
        BodyKind::Status => PanelBody::Status(read_status_body(value)?),
        BodyKind::Progress => PanelBody::Progress(read_progress_body(value)?),
        BodyKind::Empty => PanelBody::Empty(read_empty_body(value)?),
        BodyKind::Error => PanelBody::Error(read_error_body(value)?),
    };
    Ok(body)
}

fn read_list_body(value: &BoundedJson) -> Result<ListBody, ProviderError> {
    let members = closed_object(value, "panel-snapshot.body", &LIST_BODY_KEYS)?;
    let items = array(
        require(members, "panel-snapshot.body", "items")?,
        "panel-snapshot.body.items",
        LIST_ITEM_LIMIT,
    )?;
    let items = items
        .iter()
        .map(|entry| read_list_item(entry, "panel-snapshot.body.items"))
        .collect::<Result<Vec<_>, _>>()?;
    reject_unique_ids(
        items.iter().map(|item| &item.id),
        "panel-snapshot.body.items",
        "list item id",
    )?;
    let selected_id = read_optional_id(members, "panel-snapshot.body", "selected_id")?;
    if let Some(selected) = selected_id.as_ref()
        && !items.iter().any(|item| &item.id == selected)
    {
        return Err(ProviderError::InvalidValue {
            path: "panel-snapshot.body.selected_id".to_owned(),
            reason: "selected_id does not reference a list item".to_owned(),
        });
    }
    Ok(ListBody {
        items,
        selected_id,
        next_page_token: read_optional_string(members, "panel-snapshot.body", "next_page_token")?,
    })
}

fn read_list_item(value: &BoundedJson, path: &str) -> Result<ListItem, ProviderError> {
    let members = closed_object(value, path, &LIST_ITEM_KEYS)?;
    let actions = read_id_array(
        require(members, path, "actions")?,
        &format!("{path}.actions"),
        LIST_ITEM_ACTION_LIMIT,
    )?;
    Ok(ListItem {
        id: read_id(members, path, "id")?,
        label: read_string(members, path, "label")?.to_owned(),
        description: read_optional_string(members, path, "description")?,
        status: read_optional_string(members, path, "status")?,
        actions,
    })
}

include!("panel_reader_tree_diff.rs");

fn read_detail_body(value: &BoundedJson) -> Result<DetailBody, ProviderError> {
    let members = closed_object(value, "panel-snapshot.body", &DETAIL_BODY_KEYS)?;
    let document = read_string(members, "panel-snapshot.body", "document")?.to_owned();
    if document.len() > DETAIL_DOCUMENT_BYTE_LIMIT {
        return Err(ProviderError::InvalidValue {
            path: "panel-snapshot.body.document".to_owned(),
            reason: format!(
                "document is {} bytes, over the {DETAIL_DOCUMENT_BYTE_LIMIT} limit",
                document.len()
            ),
        });
    }
    let metadata = array(
        require(members, "panel-snapshot.body", "metadata")?,
        "panel-snapshot.body.metadata",
        DETAIL_METADATA_LIMIT,
    )?;
    let metadata = metadata
        .iter()
        .map(|entry| read_detail_metadata(entry, "panel-snapshot.body.metadata"))
        .collect::<Result<Vec<_>, _>>()?;
    let actions = read_id_array(
        require(members, "panel-snapshot.body", "actions")?,
        "panel-snapshot.body.actions",
        GLOBAL_ARRAY_LIMIT,
    )?;
    Ok(DetailBody {
        document,
        metadata,
        actions,
    })
}

fn read_detail_metadata(value: &BoundedJson, path: &str) -> Result<DetailMetadata, ProviderError> {
    let members = closed_object(value, path, &DETAIL_METADATA_KEYS)?;
    Ok(DetailMetadata {
        label: read_string(members, path, "label")?.to_owned(),
        value: read_string(members, path, "value")?.to_owned(),
    })
}

fn read_form_body(value: &BoundedJson) -> Result<FormBody, ProviderError> {
    let members = closed_object(value, "panel-snapshot.body", &FORM_BODY_KEYS)?;
    let fields = array(
        require(members, "panel-snapshot.body", "fields")?,
        "panel-snapshot.body.fields",
        FORM_FIELD_LIMIT,
    )?;
    let fields = fields
        .iter()
        .map(|entry| read_field_declaration(entry, "panel-snapshot.body.fields"))
        .collect::<Result<Vec<_>, _>>()?;
    let field_errors = array(
        require(members, "panel-snapshot.body", "field_errors")?,
        "panel-snapshot.body.field_errors",
        FORM_FIELD_ERROR_LIMIT,
    )?;
    let field_errors = field_errors
        .iter()
        .map(|entry| read_form_field_error(entry, "panel-snapshot.body.field_errors"))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(FormBody {
        fields,
        values: read_typed_map(
            require(members, "panel-snapshot.body", "values")?,
            "panel-snapshot.body.values",
        )?,
        field_errors,
        submit_action: super::object_reader::read_with(
            members,
            "panel-snapshot.body",
            "submit_action",
            ActionId::parse,
        )?,
    })
}

fn read_form_field_error(value: &BoundedJson, path: &str) -> Result<FormFieldError, ProviderError> {
    let members = closed_object(value, path, &FORM_FIELD_ERROR_KEYS)?;
    Ok(FormFieldError {
        field_id: read_id(members, path, "field_id")?,
        message: read_string(members, path, "message")?.to_owned(),
    })
}

fn read_status_body(value: &BoundedJson) -> Result<StatusBody, ProviderError> {
    let members = closed_object(value, "panel-snapshot.body", &STATUS_BODY_KEYS)?;
    let rows = array(
        require(members, "panel-snapshot.body", "rows")?,
        "panel-snapshot.body.rows",
        STATUS_ROW_LIMIT,
    )?;
    let rows = rows
        .iter()
        .map(|entry| read_status_row(entry, "panel-snapshot.body.rows"))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(StatusBody { rows })
}

fn read_status_row(value: &BoundedJson, path: &str) -> Result<StatusRow, ProviderError> {
    let members = closed_object(value, path, &STATUS_ROW_KEYS)?;
    Ok(StatusRow {
        label: read_string(members, path, "label")?.to_owned(),
        value: read_string(members, path, "value")?.to_owned(),
        state: read_enum(members, path, "state", StatusRowState::from_wire)?,
    })
}

fn read_progress_body(value: &BoundedJson) -> Result<ProgressBody, ProviderError> {
    let members = closed_object(value, "panel-snapshot.body", &PROGRESS_BODY_KEYS)?;
    let completed =
        super::object_reader::read_optional_u64(members, "panel-snapshot.body", "completed")?;
    let total = super::object_reader::read_optional_u64(members, "panel-snapshot.body", "total")?;
    validate_progress_counts(completed, total)?;
    Ok(ProgressBody {
        message: read_string(members, "panel-snapshot.body", "message")?.to_owned(),
        completed,
        total,
        cancellable: read_bool(members, "panel-snapshot.body", "cancellable")?,
    })
}

fn read_empty_body(value: &BoundedJson) -> Result<EmptyBody, ProviderError> {
    let members = closed_object(value, "panel-snapshot.body", &EMPTY_BODY_KEYS)?;
    Ok(EmptyBody {
        message: read_string(members, "panel-snapshot.body", "message")?.to_owned(),
        action: read_optional_id(members, "panel-snapshot.body", "action")?,
    })
}

fn read_error_body(value: &BoundedJson) -> Result<ErrorBody, ProviderError> {
    let members = closed_object(value, "panel-snapshot.body", &ERROR_BODY_KEYS)?;
    Ok(ErrorBody {
        code: read_string(members, "panel-snapshot.body", "code")?.to_owned(),
        message: read_string(members, "panel-snapshot.body", "message")?.to_owned(),
        retryable: read_bool(members, "panel-snapshot.body", "retryable")?,
        retry_action: read_optional_id(members, "panel-snapshot.body", "retry_action")?,
    })
}

// --- affordances ------------------------------------------------------------

/// Read the bounded array of action affordances.
fn read_affordance_array(value: &BoundedJson) -> Result<Vec<Affordance>, ProviderError> {
    let elements = array(value, "panel-snapshot.action_affordances", AFFORDANCE_LIMIT)?;
    elements
        .iter()
        .map(|entry| read_affordance(entry, "panel-snapshot.action_affordances"))
        .collect()
}

fn read_affordance(value: &BoundedJson, path: &str) -> Result<Affordance, ProviderError> {
    let members = closed_object(value, path, &AFFORDANCE_KEYS)?;
    let enabled = read_bool(members, path, "enabled")?;
    let unavailable_reason = read_optional_string(members, path, "unavailable_reason")?;
    validate_affordance_availability(enabled, unavailable_reason.as_deref(), path)?;
    Ok(Affordance {
        id: read_id(members, path, "id")?,
        label: read_string(members, path, "label")?.to_owned(),
        action_id: super::object_reader::read_with(members, path, "action_id", ActionId::parse)?,
        arguments: match find(members, "arguments") {
            Some(entry) => Some(read_typed_map(entry, &format!("{path}.arguments"))?),
            None => None,
        },
        enabled,
        unavailable_reason,
    })
}

/// A disabled affordance requires a nonempty reason; an enabled one must not carry one.
fn validate_affordance_availability(
    enabled: bool,
    reason: Option<&str>,
    path: &str,
) -> Result<(), ProviderError> {
    match (enabled, reason) {
        (false, Some(text)) if !text.is_empty() => Ok(()),
        (false, _) => Err(ProviderError::InvalidValue {
            path: format!("{path}.unavailable_reason"),
            reason: "a disabled affordance requires a nonempty unavailable_reason".to_owned(),
        }),
        (true, Some(_)) => Err(ProviderError::InvalidValue {
            path: format!("{path}.unavailable_reason"),
            reason: "an enabled affordance must not carry an unavailable_reason".to_owned(),
        }),
        (true, None) => Ok(()),
    }
}

// --- snapshot cross-field validation ---------------------------------------

/// Enforce the snapshot-wide invariants after structural mapping.
fn validate_snapshot(snapshot: &PanelSnapshot) -> Result<(), ProviderError> {
    if snapshot.kind != snapshot.body.kind() {
        return Err(ProviderError::InvalidValue {
            path: "panel-snapshot.kind".to_owned(),
            reason: "snapshot kind does not match its body kind".to_owned(),
        });
    }
    let _ids = collect_unique(
        snapshot.action_affordances.iter().map(|a| &a.id),
        "panel-snapshot.action_affordances",
        "affordance id",
    )?;
    let _action_ids = collect_unique(
        snapshot.action_affordances.iter().map(|a| &a.action_id),
        "panel-snapshot.action_affordances",
        "affordance action_id",
    )?;
    let enabled_ids = snapshot
        .action_affordances
        .iter()
        .filter(|affordance| affordance.enabled)
        .map(|affordance| &affordance.id)
        .collect();
    let enabled_action_ids = snapshot
        .action_affordances
        .iter()
        .filter(|affordance| affordance.enabled)
        .map(|affordance| &affordance.action_id)
        .collect();
    validate_body_action_refs(&snapshot.body, &enabled_ids, &enabled_action_ids)
}

/// Reject a referenced affordance id that no declared affordance names.
fn validate_body_action_refs(
    body: &PanelBody,
    affordance_ids: &HashSet<&Id>,
    action_ids: &HashSet<&ActionId>,
) -> Result<(), ProviderError> {
    match body {
        PanelBody::List(list) => {
            for item in &list.items {
                for action in &item.actions {
                    require_affordance_id(
                        action,
                        affordance_ids,
                        "panel-snapshot.body.items.actions",
                    )?;
                }
            }
        }
        PanelBody::Detail(detail) => {
            for action in &detail.actions {
                require_affordance_id(action, affordance_ids, "panel-snapshot.body.actions")?;
            }
        }
        PanelBody::Empty(empty) => {
            if let Some(action) = empty.action.as_ref() {
                require_affordance_id(action, affordance_ids, "panel-snapshot.body.action")?;
            }
        }
        PanelBody::Error(error) => {
            if let Some(action) = error.retry_action.as_ref() {
                require_affordance_id(action, affordance_ids, "panel-snapshot.body.retry_action")?;
            }
        }
        PanelBody::Form(form) => {
            if !action_ids.contains(&form.submit_action) {
                return Err(ProviderError::InvalidValue {
                    path: "panel-snapshot.body.submit_action".to_owned(),
                    reason: "submit_action does not resolve to a declared affordance action_id"
                        .to_owned(),
                });
            }
        }
        PanelBody::Tree(_)
        | PanelBody::StructuredDiff(_)
        | PanelBody::Status(_)
        | PanelBody::Progress(_) => {}
    }
    Ok(())
}

fn require_affordance_id(
    action: &Id,
    affordance_ids: &HashSet<&Id>,
    path: &str,
) -> Result<(), ProviderError> {
    if affordance_ids.contains(action) {
        Ok(())
    } else {
        Err(ProviderError::InvalidValue {
            path: path.to_owned(),
            reason: "an action reference does not resolve to a declared affordance".to_owned(),
        })
    }
}

/// Collect a sequence into a set, rejecting duplicates.
fn collect_unique<'a, T>(
    values: impl Iterator<Item = &'a T>,
    path: &str,
    label: &str,
) -> Result<HashSet<&'a T>, ProviderError>
where
    T: PartialEq + Eq + std::hash::Hash + 'a,
{
    let mut seen = HashSet::new();
    for value in values {
        if !seen.insert(value) {
            return Err(ProviderError::InvalidValue {
                path: path.to_owned(),
                reason: format!("a {label} is declared twice"),
            });
        }
    }
    Ok(seen)
}

/// Reject a duplicate id in an arbitrary ordered sequence.
fn reject_unique_ids<'a>(
    values: impl Iterator<Item = &'a Id>,
    path: &str,
    label: &str,
) -> Result<(), ProviderError> {
    collect_unique(values, path, label).map(|_| ())
}

/// Enforce progress-body count invariants: `total` implies `completed`,
/// and `completed <= total`.
fn validate_progress_counts(
    completed: Option<u64>,
    total: Option<u64>,
) -> Result<(), ProviderError> {
    match (completed, total) {
        (_, Some(_)) if completed.is_none() => Err(ProviderError::InvalidValue {
            path: "panel-snapshot.body.total".to_owned(),
            reason: "progress total requires a completed count".to_owned(),
        }),
        (Some(completed), Some(total)) if completed > total => Err(ProviderError::InvalidValue {
            path: "panel-snapshot.body.completed".to_owned(),
            reason: format!("progress completed {completed} exceeds total {total}"),
        }),
        _ => Ok(()),
    }
}

// --- small scalar helpers --------------------------------------------------

/// Read a required positive `u64` (zero is rejected).
fn read_positive_u64(
    members: &[(String, BoundedJson)],
    path: &str,
    key: &str,
) -> Result<u64, ProviderError> {
    let value = read_u64(members, path, key)?;
    if value == 0 {
        return Err(ProviderError::InvalidValue {
            path: format!("{path}.{key}"),
            reason: "value must be positive".to_owned(),
        });
    }
    Ok(value)
}

/// Read a required `u32` (scroll offset).
fn read_u32_field(
    members: &[(String, BoundedJson)],
    path: &str,
    key: &str,
) -> Result<u32, ProviderError> {
    let raw = require(members, path, key)?
        .as_int()
        .ok_or_else(|| type_mismatch(&format!("{path}.{key}"), "integer"))?;
    u32::try_from(raw).map_err(|_| ProviderError::InvalidValue {
        path: format!("{path}.{key}"),
        reason: format!("{raw} is not a 0..=4294967295 integer"),
    })
}

/// Read an optional closed string field.
fn read_optional_string(
    members: &[(String, BoundedJson)],
    path: &str,
    key: &str,
) -> Result<Option<String>, ProviderError> {
    find(members, key)
        .map(|entry| {
            entry
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| type_mismatch(&format!("{path}.{key}"), "string"))
        })
        .transpose()
}

/// Read an optional closed id field.
fn read_optional_id(
    members: &[(String, BoundedJson)],
    path: &str,
    key: &str,
) -> Result<Option<Id>, ProviderError> {
    find(members, key)
        .map(|entry| {
            let text = entry
                .as_str()
                .ok_or_else(|| type_mismatch(&format!("{path}.{key}"), "string"))?;
            Id::parse(text).map_err(|error| ProviderError::InvalidValue {
                path: format!("{path}.{key}"),
                reason: error.to_string(),
            })
        })
        .transpose()
}

/// Read a bounded array of `Id` values.
fn read_id_array(value: &BoundedJson, path: &str, limit: usize) -> Result<Vec<Id>, ProviderError> {
    array(value, path, limit)?
        .iter()
        .map(|entry| {
            let text = entry
                .as_str()
                .ok_or_else(|| type_mismatch(path, "string"))?;
            Id::parse(text).map_err(|error| ProviderError::InvalidValue {
                path: path.to_owned(),
                reason: error.to_string(),
            })
        })
        .collect()
}

/// Read a bounded array of strings.
fn read_string_array(value: &BoundedJson, path: &str) -> Result<Vec<String>, ProviderError> {
    array(value, path, MIGRATED_NOTES_LIMIT)?
        .iter()
        .map(|entry| {
            entry
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| type_mismatch(path, "string"))
        })
        .collect()
}
