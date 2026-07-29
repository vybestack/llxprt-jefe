//! Executable Draft 2020-12 schema package and parser-parity oracle.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use jsonschema::Draft;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::jsp::v1::{JspError, parse_event, parse_heartbeat, parse_snapshot};

use super::dto::{
    CaseExpectedWire, DocumentKindWire, EventTypeWire, SchemaCaseWire, SchemaCasesWire,
    SchemaManifestWire,
};
use super::report::COMPLIANCE_ARTIFACT_VERSION;

const SCHEMA_FILES: [(&str, DocumentKindWire); 3] = [
    ("snapshot.schema.json", DocumentKindWire::Snapshot),
    ("event.schema.json", DocumentKindWire::Event),
    ("heartbeat.schema.json", DocumentKindWire::Heartbeat),
];
const EVENT_TYPES: [&str; 11] = [
    "activity.changed",
    "wait.opened",
    "wait.resolved",
    "turn.started",
    "turn.ended",
    "todos.replaced",
    "tool_call.created",
    "tool_call.phase_changed",
    "assistant_message.displayed",
    "source.error",
    "session.ended",
];
const CASE_FILES: [&str; 26] = [
    "snapshot_positive.json",
    "snapshot_negative.json",
    "heartbeat_positive.json",
    "heartbeat_negative.json",
    "event_activity_changed_positive.json",
    "event_activity_changed_negative.json",
    "event_wait_opened_positive.json",
    "event_wait_opened_negative.json",
    "event_wait_resolved_positive.json",
    "event_wait_resolved_negative.json",
    "event_turn_started_positive.json",
    "event_turn_started_negative.json",
    "event_turn_ended_positive.json",
    "event_turn_ended_negative.json",
    "event_todos_replaced_positive.json",
    "event_todos_replaced_negative.json",
    "event_tool_call_created_positive.json",
    "event_tool_call_created_negative.json",
    "event_tool_call_phase_changed_positive.json",
    "event_tool_call_phase_changed_negative.json",
    "event_assistant_message_displayed_positive.json",
    "event_assistant_message_displayed_negative.json",
    "event_source_error_positive.json",
    "event_source_error_negative.json",
    "event_session_ended_positive.json",
    "event_session_ended_negative.json",
];
const ROOT_FILES: [&str; 5] = [
    "manifest.json",
    "cases.json",
    "snapshot.schema.json",
    "event.schema.json",
    "heartbeat.schema.json",
];
/// Maximum bytes for any schema-package artifact (schema, manifest, or case)
/// before deserialization. Prevents unbounded allocation from a corrupted or
/// oversized artifact. Each schema file is under 16 KiB and each case is under
/// 8 KiB; 1 MiB is a generous ceiling that rejects adversarial 2 MiB payloads.
const MAX_SCHEMA_ARTIFACT_BYTES: u64 = 1024 * 1024;
/// Maximum length for any metadata string (description, version, adapter
/// version, display name). Prevents an ignored description field from
/// carrying megabytes of unvalidated data through deserialization.
const MAX_METADATA_STRING_BYTES: usize = 4096;

/// One payload-free schema qualification finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaFinding {
    /// Stable finding category.
    pub kind: String,
    /// Canonical artifact identity, never an attacker-controlled path.
    pub document: String,
    /// Stable payload-free explanation.
    pub detail: String,
}

/// Result of executable schema and parser qualification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaReport {
    /// Presence of every canonical document schema.
    pub schemas_present: SchemaPresence,
    /// Positive cases accepted by both engines.
    pub positive_count: usize,
    /// Negative cases rejected by both engines.
    pub negative_count: usize,
    /// Deterministic findings.
    pub findings: Vec<SchemaFinding>,
    /// Whether all qualification checks passed.
    pub passed: bool,
}

/// Compact presence mask for the exact three-schema inventory.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SchemaPresence(u8);

impl SchemaPresence {
    const SNAPSHOT: u8 = 1;
    const EVENT: u8 = 2;
    const HEARTBEAT: u8 = 4;
    const ALL: u8 = 7;

    /// Whether every canonical schema was qualified.
    #[must_use]
    pub const fn all_present(&self) -> bool {
        self.0 == Self::ALL
    }
}

/// Stable schema-package boundary error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaOracleError {
    /// Payload-free boundary code.
    pub message: String,
}

impl std::fmt::Display for SchemaOracleError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}
impl std::error::Error for SchemaOracleError {}

/// Qualify the exact schema package with an independent semantic oracle,
/// executable Draft 2020-12 validators, custom UTF-8 byte limits, and JSP parsers.
pub fn run_schema_oracle(
    schemas_dir: &Path,
    _legacy_fixtures_dir: &Path,
) -> Result<SchemaReport, SchemaOracleError> {
    check_inventory(schemas_dir)?;
    let manifest = read_closed::<SchemaManifestWire>(&schemas_dir.join("manifest.json"))?;
    validate_manifest(&manifest)?;
    let mut findings = Vec::new();
    let mut presence = SchemaPresence::default();
    let mut schemas = HashMap::new();
    for (file, kind) in SCHEMA_FILES {
        let value = read_value(&schemas_dir.join(file))?;
        inspect_schema(&value, kind, file, &mut findings);
        if compile(&value).is_none() {
            findings.push(finding(
                "schema_compile",
                file,
                "Draft 2020-12 compilation failed",
            ));
        }
        presence.0 |= schema_bit(kind);
        schemas.insert(kind_name(kind), value);
    }
    let cases = read_closed::<SchemaCasesWire>(&schemas_dir.join("cases.json"))?;
    let (positive_count, negative_count) =
        inspect_cases(schemas_dir, cases, &schemas, &mut findings);
    inspect_boundary_parity(schemas_dir, &schemas, &mut findings);
    let passed = presence.all_present() && findings.is_empty();
    Ok(SchemaReport {
        schemas_present: presence,
        positive_count,
        negative_count,
        findings,
        passed,
    })
}

fn validate_manifest(manifest: &SchemaManifestWire) -> Result<(), SchemaOracleError> {
    if manifest.schema != 1
        || manifest.schema_artifact_version != COMPLIANCE_ARTIFACT_VERSION
        || manifest.cases != "cases.json"
        || manifest.schemas.len() != SCHEMA_FILES.len()
        || manifest.description.len() > MAX_METADATA_STRING_BYTES
    {
        return Err(error("JSP-C-SCHEMA-MANIFEST"));
    }
    for ((expected_file, expected_kind), actual) in SCHEMA_FILES.iter().zip(&manifest.schemas) {
        if actual.file != *expected_file || actual.kind != *expected_kind {
            return Err(error("JSP-C-SCHEMA-INVENTORY"));
        }
    }
    Ok(())
}

fn inspect_schema(
    schema: &Value,
    kind: DocumentKindWire,
    file: &str,
    findings: &mut Vec<SchemaFinding>,
) {
    let expected_id = format!("https://jefe.dev/schema/jsp/1/{file}");
    let required = required_fields(kind);
    let root_ok = schema_root_ok(schema, &expected_id, kind_name(kind), required);
    if !root_ok || !semantic_bounds(schema, None) {
        findings.push(finding(
            "schema_semantics",
            file,
            "strongly typed root, closure, numeric, or UTF-8 bound invariant failed",
        ));
    }
    if kind == DocumentKindWire::Event {
        check_event_inventory(schema, file, findings);
    }
}

fn required_fields(kind: DocumentKindWire) -> &'static [&'static str] {
    match kind {
        DocumentKindWire::Snapshot => &[
            "schema",
            "kind",
            "agent_id",
            "lifecycle_generation",
            "source_epoch",
            "source_sequence",
            "cursor",
            "bridge_observed_ms",
            "native_session",
            "process_binding",
            "native_activity",
            "current_wait",
            "current_turn",
            "todos",
            "last_displayed_assistant_message",
            "last_created_tool_call",
            "source_terminal_state",
            "source_error_state",
        ],
        DocumentKindWire::Event => &[
            "schema",
            "kind",
            "agent_id",
            "lifecycle_generation",
            "source_epoch",
            "source_sequence",
            "bridge_observed_ms",
            "event",
        ],
        DocumentKindWire::Heartbeat => &[
            "schema",
            "kind",
            "agent_id",
            "lifecycle_generation",
            "source_epoch",
            "bridge_observed_ms",
        ],
    }
}

fn schema_root_ok(schema: &Value, expected_id: &str, kind: &str, required: &[&str]) -> bool {
    schema.get("$schema").and_then(Value::as_str)
        == Some("https://json-schema.org/draft/2020-12/schema")
        && schema.get("$id").and_then(Value::as_str) == Some(expected_id)
        && schema
            .pointer("/properties/kind/const")
            .and_then(Value::as_str)
            == Some(kind)
        && string_set(schema.get("required")) == required.iter().copied().collect()
        && schema.get("additionalProperties") == Some(&Value::Bool(false))
}

fn check_event_inventory(schema: &Value, file: &str, findings: &mut Vec<SchemaFinding>) {
    let actual: HashSet<&str> = schema
        .pointer("/properties/event/oneOf")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|branch| {
            branch
                .pointer("/properties/type/const")
                .and_then(Value::as_str)
        })
        .collect();
    if actual != EVENT_TYPES.into_iter().collect() {
        findings.push(finding(
            "event_inventory",
            file,
            "semantic event discriminator inventory differs from frozen JSP/1",
        ));
    }
}

fn semantic_bounds(node: &Value, property: Option<&str>) -> bool {
    match node {
        Value::Object(object) => {
            let is_string = object.get("type").and_then(Value::as_str) == Some("string");
            let string_ok = match object.get("maxLength") {
                None => !is_string && object.get("x-jsp-maxUtf8Bytes").is_none(),
                Some(maximum) => {
                    object.get("x-jsp-maxUtf8Bytes") == Some(maximum)
                        && maximum.as_u64().is_some_and(|value| value > 0)
                }
            };
            let integer_ok = object.get("type").and_then(Value::as_str) != Some("integer")
                || object.get("maximum").and_then(Value::as_u64)
                    == Some(if property == Some("pid") {
                        u64::from(u32::MAX)
                    } else {
                        u64::MAX
                    });
            string_ok
                && integer_ok
                && object
                    .iter()
                    .all(|(key, value)| semantic_bounds(value, Some(key)))
        }
        Value::Array(values) => values.iter().all(|value| semantic_bounds(value, property)),
        _ => true,
    }
}

fn inspect_cases(
    schemas_dir: &Path,
    cases: SchemaCasesWire,
    schemas: &HashMap<&'static str, Value>,
    findings: &mut Vec<SchemaFinding>,
) -> (usize, usize) {
    if cases.schema != 1 || cases.cases.len() != CASE_FILES.len() {
        findings.push(finding(
            "schema_cases",
            "cases.json",
            "exact case inventory is required",
        ));
        return (0, 0);
    }
    let mut positive = 0;
    let mut negative = 0;
    let mut event_positive = HashSet::new();
    let mut event_negative = HashSet::new();
    for (index, case) in cases.cases.iter().enumerate() {
        let canonical = format!("cases/{}", CASE_FILES[index]);
        let (result, discriminator) =
            evaluate_case(case, index, &canonical, schemas_dir, schemas, findings);
        match result {
            CaseResult::Positive => {
                positive += 1;
                if let Some(d) = discriminator {
                    event_positive.insert(d);
                }
            }
            CaseResult::Negative => {
                negative += 1;
                if let Some(d) = discriminator {
                    event_negative.insert(d);
                }
            }
            CaseResult::Skipped => {}
        }
    }
    check_event_case_coverage(&event_positive, &event_negative, findings);
    (positive, negative)
}

enum CaseResult {
    Positive,
    Negative,
    Skipped,
}

fn evaluate_case(
    case: &SchemaCaseWire,
    index: usize,
    canonical: &str,
    schemas_dir: &Path,
    schemas: &HashMap<&'static str, Value>,
    findings: &mut Vec<SchemaFinding>,
) -> (CaseResult, Option<String>) {
    if !canonical_case(case, CASE_FILES[index]) {
        findings.push(finding(
            "schema_cases",
            "cases.json",
            "canonical case identity drifted",
        ));
        return (CaseResult::Skipped, None);
    }
    let Ok(value) = read_value(&schemas_dir.join(canonical)) else {
        findings.push(finding("missing_case", canonical, "case read failed"));
        return (CaseResult::Skipped, None);
    };
    let observed_discriminator = value.pointer("/event/type").and_then(Value::as_str);
    let event_discriminator =
        match check_event_coverage(case, observed_discriminator, canonical, findings) {
            EventCoverage::Skip => return (CaseResult::Skipped, None),
            EventCoverage::Event(d) => Some(d),
            EventCoverage::NonEvent => None,
        };
    let schema = &schemas[schema_name(case.document_kind)];
    let schema_result = validate_instance(schema, &value);
    let Ok(bytes) = serde_json::to_vec(&value) else {
        findings.push(finding("case_encoding", canonical, "case encoding failed"));
        return (CaseResult::Skipped, None);
    };
    let parser_result = parse_case(case.document_kind, &bytes);
    let result = match (case.expected, schema_result, parser_result) {
        (CaseExpectedWire::Ok, true, Ok(())) => CaseResult::Positive,
        (CaseExpectedWire::Error, false, Err(actual))
            if case.expected_code.map(super::dto::ExpectedCodeWire::as_str)
                == Some(actual.code().as_str()) =>
        {
            CaseResult::Negative
        }
        _ => {
            findings.push(finding(
                "schema_parser_parity",
                canonical,
                "schema and parser did not produce the pinned result",
            ));
            CaseResult::Skipped
        }
    };
    (result, event_discriminator)
}

enum EventCoverage {
    Event(String),
    NonEvent,
    Skip,
}

fn check_event_coverage(
    case: &SchemaCaseWire,
    observed: Option<&str>,
    canonical: &str,
    findings: &mut Vec<SchemaFinding>,
) -> EventCoverage {
    if case.document_kind == DocumentKindWire::Event {
        let declared = case.event_type.map(event_name);
        if declared != observed {
            findings.push(finding(
                "event_discriminator",
                canonical,
                "declared and document event types differ",
            ));
            return EventCoverage::Skip;
        }
        match observed {
            Some(d) => EventCoverage::Event(d.to_string()),
            None => EventCoverage::NonEvent,
        }
    } else if case.event_type.is_some() || observed.is_some() {
        findings.push(finding(
            "event_coverage_guard",
            canonical,
            "only event documents may declare event coverage",
        ));
        EventCoverage::Skip
    } else {
        EventCoverage::NonEvent
    }
}

fn check_event_case_coverage(
    event_positive: &HashSet<String>,
    event_negative: &HashSet<String>,
    findings: &mut Vec<SchemaFinding>,
) {
    let expected: HashSet<String> = EVENT_TYPES
        .iter()
        .map(|value| (*value).to_string())
        .collect();
    if event_positive != &expected || event_negative != &expected {
        findings.push(finding(
            "event_case_inventory",
            "cases.json",
            "all 11 event discriminators require positive and negative event documents",
        ));
    }
}

fn inspect_boundary_parity(
    schemas_dir: &Path,
    schemas: &HashMap<&'static str, Value>,
    findings: &mut Vec<SchemaFinding>,
) {
    let event_schema = &schemas["event"];
    let base = serde_json::json!({
        "schema": 1, "kind": "event", "agent_id": "a", "lifecycle_generation": 1,
        "source_epoch": "e", "source_sequence": 1, "bridge_observed_ms": 1,
        "event": { "type": "assistant_message.displayed", "content": "", "committed_ms": 1 }
    });
    for (chars, expected) in [(8192usize, true), (8193usize, false)] {
        let mut probe = base.clone();
        probe["event"]["content"] = Value::String("é".repeat(chars));
        if validate_instance(event_schema, &probe) != expected
            || parser_accepts(DocumentKindWire::Event, &probe) != expected
        {
            findings.push(finding(
                "utf8_byte_parity",
                "event.schema.json",
                "multibyte byte-bound probe failed",
            ));
        }
    }
    let mut u64_max = base.clone();
    u64_max["lifecycle_generation"] = Value::from(u64::MAX);
    if !validate_instance(event_schema, &u64_max)
        || !parser_accepts(DocumentKindWire::Event, &u64_max)
    {
        findings.push(finding(
            "u64_maximum",
            "event.schema.json",
            "u64 maximum edge was rejected",
        ));
    }
    inspect_u32_pid_parity(schemas_dir, schemas, findings);
    let Ok(overflow) = serde_json::from_str::<Value>(
        r#"{"schema":1,"kind":"heartbeat","agent_id":"a","lifecycle_generation":18446744073709551616,"source_epoch":"e","bridge_observed_ms":1}"#,
    ) else {
        findings.push(finding(
            "u64_maximum",
            "heartbeat.schema.json",
            "u64 overflow probe could not be represented",
        ));
        return;
    };
    if validate_instance(&schemas["heartbeat"], &overflow)
        || parser_accepts(DocumentKindWire::Heartbeat, &overflow)
    {
        findings.push(finding(
            "u64_maximum",
            "heartbeat.schema.json",
            "u64 limit-plus-one was accepted",
        ));
    }
    inspect_degraded_field_alignment(schemas_dir, schemas, findings);
}

fn inspect_u32_pid_parity(
    schemas_dir: &Path,
    schemas: &HashMap<&'static str, Value>,
    findings: &mut Vec<SchemaFinding>,
) {
    let Ok(mut snapshot) = read_value(&schemas_dir.join("cases/snapshot_positive.json")) else {
        findings.push(finding(
            "u32_maximum",
            "snapshot.schema.json",
            "caller snapshot_positive case unavailable for pid probe",
        ));
        return;
    };
    snapshot["native_session"]["pid"] = Value::from(u32::MAX);
    let maximum_ok = validate_instance(&schemas["snapshot"], &snapshot)
        && parser_accepts(DocumentKindWire::Snapshot, &snapshot);
    snapshot["native_session"]["pid"] = Value::from(u64::from(u32::MAX) + 1);
    let overflow_rejected = !validate_instance(&schemas["snapshot"], &snapshot)
        && !parser_accepts(DocumentKindWire::Snapshot, &snapshot);
    if !(maximum_ok && overflow_rejected) {
        findings.push(finding(
            "u32_maximum",
            "snapshot.schema.json",
            "u32 pid maximum parity failed",
        ));
    }
}

/// Verify that fields without meaningful last-known values reject `degraded`
/// in both the standard schema and typed parser.
fn inspect_degraded_field_alignment(
    schemas_dir: &Path,
    schemas: &HashMap<&'static str, Value>,
    findings: &mut Vec<SchemaFinding>,
) {
    let Ok(snapshot) = read_value(&schemas_dir.join("cases/snapshot_positive.json")) else {
        return;
    };
    for field in ["source_terminal_state", "current_wait"] {
        let mut probe = snapshot.clone();
        probe[field] = serde_json::json!({
            "provenance": "authoritative",
            "availability": "degraded",
            "last_value": null,
            "as_of_ms": 1,
            "diagnostic_code": "x"
        });
        let schema_rejects = !validate_instance(&schemas["snapshot"], &probe);
        let parser_rejects = !parser_accepts(DocumentKindWire::Snapshot, &probe);
        if !schema_rejects || !parser_rejects {
            findings.push(finding(
                "degraded_field_alignment",
                "snapshot.schema.json",
                &format!("{field} must reject degraded in both schema and parser"),
            ));
        }
    }
}

fn validate_instance(schema: &Value, instance: &Value) -> bool {
    compile(schema).is_some_and(|validator| validator.is_valid(instance))
        && custom_utf8_valid(schema, schema, instance)
}

fn compile(schema: &Value) -> Option<jsonschema::Validator> {
    jsonschema::options()
        .with_draft(Draft::Draft202012)
        .build(schema)
        .ok()
}

use super::schema_utf8::custom_utf8_valid;

fn canonical_case(case: &SchemaCaseWire, file: &str) -> bool {
    let expected_path = format!("cases/{file}");
    let expected_name = file.strip_suffix(".json").unwrap_or(file);
    let kind = if file.starts_with("snapshot_") {
        DocumentKindWire::Snapshot
    } else if file.starts_with("heartbeat_") {
        DocumentKindWire::Heartbeat
    } else {
        DocumentKindWire::Event
    };
    let expected = if file.ends_with("_positive.json") {
        CaseExpectedWire::Ok
    } else {
        CaseExpectedWire::Error
    };
    case.file == expected_path
        && case.name == expected_name
        && case.document_kind == kind
        && case.expected == expected
        && ((expected == CaseExpectedWire::Ok && case.expected_code.is_none())
            || (expected == CaseExpectedWire::Error && case.expected_code.is_some()))
}

fn check_inventory(directory: &Path) -> Result<(), SchemaOracleError> {
    let root: HashSet<&str> = ROOT_FILES.into_iter().collect();
    let cases: HashSet<&str> = CASE_FILES.into_iter().collect();
    for entry in std::fs::read_dir(directory).map_err(|_| error("JSP-C-SCHEMA-READ"))? {
        let entry = entry.map_err(|_| error("JSP-C-SCHEMA-ENTRY"))?;
        let metadata =
            std::fs::symlink_metadata(entry.path()).map_err(|_| error("JSP-C-SCHEMA-METADATA"))?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            return Err(error("JSP-C-SCHEMA-NAME"));
        };
        if metadata.is_symlink()
            || (!metadata.is_file() && name != "cases")
            || (metadata.is_file() && !root.contains(name))
        {
            return Err(error("JSP-C-SCHEMA-INVENTORY"));
        }
    }
    let cases_dir = directory.join("cases");
    for entry in std::fs::read_dir(cases_dir).map_err(|_| error("JSP-C-CASES-READ"))? {
        let entry = entry.map_err(|_| error("JSP-C-CASES-ENTRY"))?;
        let metadata =
            std::fs::symlink_metadata(entry.path()).map_err(|_| error("JSP-C-CASES-METADATA"))?;
        let name = entry.file_name();
        if metadata.is_symlink()
            || !metadata.is_file()
            || !name.to_str().is_some_and(|name| cases.contains(name))
        {
            return Err(error("JSP-C-CASES-INVENTORY"));
        }
    }
    Ok(())
}

fn parse_case(kind: DocumentKindWire, bytes: &[u8]) -> Result<(), JspError> {
    match kind {
        DocumentKindWire::Snapshot => parse_snapshot(bytes).map(|_| ()),
        DocumentKindWire::Event => parse_event(bytes).map(|_| ()),
        DocumentKindWire::Heartbeat => parse_heartbeat(bytes).map(|_| ()),
    }
}
fn parser_accepts(kind: DocumentKindWire, value: &Value) -> bool {
    serde_json::to_vec(value).is_ok_and(|bytes| parse_case(kind, &bytes).is_ok())
}
fn read_closed<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, SchemaOracleError> {
    let metadata = std::fs::metadata(path).map_err(|_| error("JSP-C-ARTIFACT-READ"))?;
    if metadata.len() > MAX_SCHEMA_ARTIFACT_BYTES {
        return Err(error("JSP-C-ARTIFACT-BOUND"));
    }
    let bytes = std::fs::read(path).map_err(|_| error("JSP-C-ARTIFACT-READ"))?;
    serde_json::from_slice(&bytes).map_err(|_| error("JSP-C-ARTIFACT-SHAPE"))
}
fn read_value(path: &Path) -> Result<Value, SchemaOracleError> {
    read_closed(path)
}
fn string_set(value: Option<&Value>) -> HashSet<&str> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect()
}
const fn schema_bit(kind: DocumentKindWire) -> u8 {
    match kind {
        DocumentKindWire::Snapshot => SchemaPresence::SNAPSHOT,
        DocumentKindWire::Event => SchemaPresence::EVENT,
        DocumentKindWire::Heartbeat => SchemaPresence::HEARTBEAT,
    }
}
const fn kind_name(kind: DocumentKindWire) -> &'static str {
    match kind {
        DocumentKindWire::Snapshot => "snapshot",
        DocumentKindWire::Event => "event",
        DocumentKindWire::Heartbeat => "heartbeat",
    }
}
const fn schema_name(kind: DocumentKindWire) -> &'static str {
    kind_name(kind)
}
const fn event_name(kind: EventTypeWire) -> &'static str {
    match kind {
        EventTypeWire::ActivityChanged => "activity.changed",
        EventTypeWire::WaitOpened => "wait.opened",
        EventTypeWire::WaitResolved => "wait.resolved",
        EventTypeWire::TurnStarted => "turn.started",
        EventTypeWire::TurnEnded => "turn.ended",
        EventTypeWire::TodosReplaced => "todos.replaced",
        EventTypeWire::ToolCallCreated => "tool_call.created",
        EventTypeWire::ToolCallPhaseChanged => "tool_call.phase_changed",
        EventTypeWire::AssistantMessageDisplayed => "assistant_message.displayed",
        EventTypeWire::SourceError => "source.error",
        EventTypeWire::SessionEnded => "session.ended",
    }
}
fn finding(kind: &str, document: &str, detail: &str) -> SchemaFinding {
    SchemaFinding {
        kind: kind.to_string(),
        document: document.to_string(),
        detail: detail.to_string(),
    }
}
fn error(code: &str) -> SchemaOracleError {
    SchemaOracleError {
        message: code.to_string(),
    }
}

/// Canonical schema package directory beneath a workspace root.
#[must_use]
pub fn default_schemas_dir(workspace_root: &Path) -> PathBuf {
    workspace_root.join("dev-docs/jsp/v1/compliance/schemas")
}
/// Legacy fixture directory accepted for CLI compatibility but never used as a schema oracle.
#[must_use]
pub fn default_fixtures_dir(workspace_root: &Path) -> PathBuf {
    workspace_root.join("dev-docs/jsp/v1/fixtures")
}
