//! Fail-closed scenario loading and deterministic per-step evaluation.

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

use crate::domain::observation::{EventRecord, HeartbeatRecord};
use crate::jsp::Snapshot;

use super::dto::{ScenarioManifestWire, ScenarioStepKind, ScenarioStepWire, ScenarioWire};
use super::projection::NormalizedProjection;
use super::reducer::{ReducerError, ReferenceReducer};

const CANONICAL_SCENARIOS: [(&str, &str, &str, &str); 15] = [
    (
        "S1",
        "ready_session_no_active_turn",
        "s01_ready_session.json",
        "event_before_snapshot|heartbeat_before_snapshot|heartbeat",
    ),
    (
        "S2",
        "turn_starts_emits_assistant_text",
        "s02_turn_starts_text.json",
        "event|event|event",
    ),
    (
        "S3",
        "tool_proposed_awaiting_approved_executing_succeeded",
        "s03_tool_lifecycle.json",
        "event|event|event|event|event|event_illegal_transition",
    ),
    (
        "S4",
        "explicit_user_question_and_resolution",
        "s04_wait_question.json",
        "event|event",
    ),
    (
        "S5",
        "todo_list_replacement_and_completion",
        "s05_todos.json",
        "event|event|event|event_noop|event_noop",
    ),
    (
        "S6",
        "turn_failure",
        "s06_turn_failure.json",
        "event|event|event|event|event_illegal_transition",
    ),
    (
        "S7",
        "turn_cancellation",
        "s07_turn_cancelled.json",
        "event|event",
    ),
    (
        "S8",
        "two_concurrent_tool_calls_interleaved",
        "s08_concurrent_tools.json",
        "event|event|event",
    ),
    (
        "S9",
        "stream_disconnect_during_assistant_draft",
        "s09_disconnect_draft.json",
        "event|draft_excluded|transport_disconnect|event_after_fresh_required|heartbeat_after_fresh_required|process_liveness|fresh_snapshot",
    ),
    (
        "S10",
        "publisher_overflow_observation_gap",
        "s10_overflow_gap.json",
        "event|event_gap|event_after_fresh_required|heartbeat_after_fresh_required|fresh_snapshot",
    ),
    (
        "S11",
        "jefe_restart_fresh_snapshot_current_state",
        "s11_restart_snapshot_first.json",
        "transport_disconnect|fresh_snapshot",
    ),
    (
        "S12",
        "jefe_restart_fresh_stream_without_retained_events",
        "s12_restart_no_history.json",
        "transport_disconnect|process_liveness|fresh_snapshot",
    ),
    (
        "S13",
        "agent_relaunch_new_generation_and_epoch",
        "s13_agent_relaunch.json",
        "fresh_snapshot|event_epoch_mismatch",
    ),
    (
        "S14",
        "two_instances_same_repo_branch_directory",
        "s14_two_instances.json",
        "parallel_snapshot",
    ),
    (
        "S15",
        "no_content_privacy_mode",
        "s15_privacy_mode.json",
        "heartbeat|malformed_capabilities",
    ),
];
const MAX_SCENARIO_BYTES: u64 = 1024 * 1024;
const MAX_SCENARIO_STEPS: usize = 64;
/// Maximum bytes for a scenario manifest or individual scenario scenario file
/// before deserialization. Prevents unbounded allocation on a corrupted or
/// oversized artifact.
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
pub enum ScenarioStep {
    Event {
        document: EventRecord,
        expected: NormalizedProjection,
    },
    Heartbeat {
        document: HeartbeatRecord,
        expected: NormalizedProjection,
    },
    EventNoop {
        document: EventRecord,
        expected: NormalizedProjection,
    },
    EventBeforeSnapshot {
        document: EventRecord,
        expected: NormalizedProjection,
    },
    HeartbeatBeforeSnapshot {
        document: HeartbeatRecord,
        expected: NormalizedProjection,
    },
    EventAfterFreshRequired {
        document: EventRecord,
        expected: NormalizedProjection,
    },
    HeartbeatAfterFreshRequired {
        document: HeartbeatRecord,
        expected: NormalizedProjection,
    },
    MalformedCapabilities {
        document: Vec<u8>,
        expected: NormalizedProjection,
    },
    EventGap {
        document: EventRecord,
        expected: NormalizedProjection,
    },
    EventEpochMismatch {
        document: EventRecord,
        expected: NormalizedProjection,
    },
    EventIllegalTransition {
        document: EventRecord,
        expected: NormalizedProjection,
    },
    DraftExcluded {
        expected: NormalizedProjection,
    },
    TransportDisconnect {
        permanent: bool,
        expected: NormalizedProjection,
    },
    FreshSnapshot {
        document: Snapshot,
        expected: NormalizedProjection,
    },
    ParallelSnapshot {
        document: Snapshot,
        expected_primary: NormalizedProjection,
        expected_secondary: NormalizedProjection,
    },
    ProcessLiveness {
        alive: bool,
        expected: NormalizedProjection,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScenarioOracleError {
    pub message: String,
}

impl std::fmt::Display for ScenarioOracleError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ScenarioOracleError {}

pub struct Scenario {
    pub id: String,
    pub name: String,
    pub base_snapshot: Snapshot,
    pub steps: Vec<ScenarioStep>,
}

#[derive(Debug, Clone)]
pub struct StepOutcome {
    pub index: usize,
    pub passed: bool,
    pub expected: NormalizedProjection,
    pub actual: NormalizedProjection,
    pub failure: Option<String>,
    /// Typed expected sequence from a [`ReducerError::Gap`] rejection, if any.
    pub expected_sequence: Option<u64>,
    /// Typed actual sequence from a [`ReducerError::Gap`] rejection, if any.
    pub actual_sequence: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct ScenarioResult {
    pub id: String,
    pub name: String,
    pub passed: bool,
    pub steps: Vec<StepOutcome>,
}

pub struct ScenarioEntry {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
}

/// Load a scenario through a closed DTO and validate every JSP document before
/// an evaluable scenario can exist.
pub fn load_scenario(path: &Path) -> Result<Scenario, ScenarioOracleError> {
    let metadata = std::fs::metadata(path).map_err(|_| fail("JSP-C-SCENARIO-READ"))?;
    if metadata.len() > MAX_SCENARIO_BYTES {
        return Err(fail("JSP-C-SCENARIO-BOUND"));
    }
    let bytes = std::fs::read(path).map_err(|_| fail("JSP-C-SCENARIO-READ"))?;
    let wire: ScenarioWire =
        serde_json::from_slice(&bytes).map_err(|_| fail("JSP-C-SCENARIO-SHAPE"))?;
    convert_scenario(wire)
}

fn convert_scenario(wire: ScenarioWire) -> Result<Scenario, ScenarioOracleError> {
    if wire.schema != 1 || wire.steps.len() > MAX_SCENARIO_STEPS {
        return Err(fail("JSP-C-SCENARIO-HEADER"));
    }
    let base_snapshot = parse_snapshot_document(wire.base_snapshot).map_err(|code| {
        fail(format!(
            "base_snapshot snapshot parser rejected with {code}"
        ))
    })?;
    let mut steps = Vec::with_capacity(wire.steps.len());
    for (position, step) in wire.steps.into_iter().enumerate() {
        steps.push(convert_step(step, position)?);
    }
    Ok(Scenario {
        id: wire.id,
        name: wire.name,
        base_snapshot,
        steps,
    })
}

fn convert_step(
    wire: ScenarioStepWire,
    position: usize,
) -> Result<ScenarioStep, ScenarioOracleError> {
    if !step_shape_valid(&wire) {
        return Err(fail(format!(
            "step {position} has fields outside its closed interaction shape"
        )));
    }
    check_index(wire.index, position)?;
    let missing = || fail(format!("step {position} missing required field"));
    let invalid = |code| {
        fail(format!(
            "step {position} document parser rejected with {code}"
        ))
    };
    match wire.kind {
        ScenarioStepKind::Event => Ok(ScenarioStep::Event {
            document: parse_event_document(wire.document.ok_or_else(missing)?).map_err(invalid)?,
            expected: wire.expected.ok_or_else(missing)?,
        }),
        ScenarioStepKind::Heartbeat => Ok(ScenarioStep::Heartbeat {
            document: parse_heartbeat_document(wire.document.ok_or_else(missing)?)
                .map_err(invalid)?,
            expected: wire.expected.ok_or_else(missing)?,
        }),
        ScenarioStepKind::EventDuplicate
        | ScenarioStepKind::EventLower
        | ScenarioStepKind::EventBeforeSnapshot
        | ScenarioStepKind::HeartbeatBeforeSnapshot
        | ScenarioStepKind::EventAfterFreshRequired
        | ScenarioStepKind::HeartbeatAfterFreshRequired
        | ScenarioStepKind::EventAfterSessionEnded
        | ScenarioStepKind::ToolTerminalRegression
        | ScenarioStepKind::MalformedCapabilities => convert_assertion_step(wire, position),
        ScenarioStepKind::EventGap | ScenarioStepKind::EventEpochMismatch => {
            convert_rejected_step(wire, position)
        }
        ScenarioStepKind::DraftExcluded => Ok(ScenarioStep::DraftExcluded {
            expected: wire.expected.ok_or_else(missing)?,
        }),
        ScenarioStepKind::TransportDisconnect
        | ScenarioStepKind::FreshSnapshot
        | ScenarioStepKind::ParallelSnapshot
        | ScenarioStepKind::ProcessLiveness => convert_observer_step(wire, position),
    }
}
fn convert_rejected_step(
    wire: ScenarioStepWire,
    position: usize,
) -> Result<ScenarioStep, ScenarioOracleError> {
    let missing = || fail(format!("step {position} missing required field"));
    let document = parse_event_document(wire.document.ok_or_else(missing)?).map_err(|code| {
        fail(format!(
            "step {position} document parser rejected with {code}"
        ))
    })?;
    let expected = wire.expected.ok_or_else(missing)?;
    match wire.kind {
        ScenarioStepKind::EventGap if wire.expected_gap_signal == Some(true) => {
            Ok(ScenarioStep::EventGap { document, expected })
        }
        ScenarioStepKind::EventEpochMismatch if wire.expected_rejected_identity == Some(true) => {
            Ok(ScenarioStep::EventEpochMismatch { document, expected })
        }
        _ => Err(fail(format!(
            "step {position} is missing its typed rejection expectation"
        ))),
    }
}
fn convert_assertion_step(
    wire: ScenarioStepWire,
    position: usize,
) -> Result<ScenarioStep, ScenarioOracleError> {
    let missing = || fail(format!("step {position} missing required field"));
    let invalid = |code| {
        fail(format!(
            "step {position} document parser rejected with {code}"
        ))
    };
    let expected = wire.expected.ok_or_else(missing)?;
    let document = wire.document.ok_or_else(missing)?;
    match wire.kind {
        ScenarioStepKind::EventDuplicate | ScenarioStepKind::EventLower => {
            Ok(ScenarioStep::EventNoop {
                document: parse_event_document(document).map_err(invalid)?,
                expected,
            })
        }
        ScenarioStepKind::EventBeforeSnapshot => Ok(ScenarioStep::EventBeforeSnapshot {
            document: parse_event_document(document).map_err(invalid)?,
            expected,
        }),
        ScenarioStepKind::HeartbeatBeforeSnapshot => Ok(ScenarioStep::HeartbeatBeforeSnapshot {
            document: parse_heartbeat_document(document).map_err(invalid)?,
            expected,
        }),
        ScenarioStepKind::EventAfterFreshRequired => Ok(ScenarioStep::EventAfterFreshRequired {
            document: parse_event_document(document).map_err(invalid)?,
            expected,
        }),
        ScenarioStepKind::HeartbeatAfterFreshRequired => {
            Ok(ScenarioStep::HeartbeatAfterFreshRequired {
                document: parse_heartbeat_document(document).map_err(invalid)?,
                expected,
            })
        }
        ScenarioStepKind::EventAfterSessionEnded | ScenarioStepKind::ToolTerminalRegression => {
            if wire.expected_illegal_transition != Some(true) {
                return Err(fail(format!(
                    "step {position} must expect an illegal transition"
                )));
            }
            Ok(ScenarioStep::EventIllegalTransition {
                document: parse_event_document(document).map_err(invalid)?,
                expected,
            })
        }
        ScenarioStepKind::MalformedCapabilities => Ok(ScenarioStep::MalformedCapabilities {
            document: document.as_bytes().to_vec(),
            expected,
        }),
        _ => Err(fail(format!("step {position} is not an assertion step"))),
    }
}

fn convert_observer_step(
    wire: ScenarioStepWire,
    position: usize,
) -> Result<ScenarioStep, ScenarioOracleError> {
    let missing = || fail(format!("step {position} missing required field"));
    let invalid = |code| {
        fail(format!(
            "step {position} document parser rejected with {code}"
        ))
    };
    match wire.kind {
        ScenarioStepKind::TransportDisconnect => Ok(ScenarioStep::TransportDisconnect {
            permanent: wire.permanent.ok_or_else(missing)?,
            expected: wire.expected.ok_or_else(missing)?,
        }),
        ScenarioStepKind::FreshSnapshot => Ok(ScenarioStep::FreshSnapshot {
            document: parse_snapshot_document(wire.document.ok_or_else(missing)?)
                .map_err(invalid)?,
            expected: wire.expected.ok_or_else(missing)?,
        }),
        ScenarioStepKind::ParallelSnapshot => Ok(ScenarioStep::ParallelSnapshot {
            document: parse_snapshot_document(wire.document.ok_or_else(missing)?)
                .map_err(invalid)?,
            expected_primary: wire.expected_primary.ok_or_else(missing)?,
            expected_secondary: wire.expected_secondary.ok_or_else(missing)?,
        }),
        ScenarioStepKind::ProcessLiveness => Ok(ScenarioStep::ProcessLiveness {
            alive: wire.alive.ok_or_else(missing)?,
            expected: wire.expected.ok_or_else(missing)?,
        }),
        _ => Err(fail(format!("step {position} is not observer-owned"))),
    }
}

fn step_shape_valid(step: &ScenarioStepWire) -> bool {
    let present = [
        step.document.is_some(),
        step.expected.is_some(),
        step.expected_gap_signal.is_some(),
        step.expected_rejected_identity.is_some(),
        step.expected_illegal_transition.is_some(),
        step.permanent.is_some(),
        step.expected_primary.is_some(),
        step.expected_secondary.is_some(),
        step.alive.is_some(),
    ]
    .into_iter()
    .filter(|value| *value)
    .count();
    match step.kind {
        ScenarioStepKind::Event
        | ScenarioStepKind::Heartbeat
        | ScenarioStepKind::EventDuplicate
        | ScenarioStepKind::EventLower
        | ScenarioStepKind::EventBeforeSnapshot
        | ScenarioStepKind::HeartbeatBeforeSnapshot
        | ScenarioStepKind::EventAfterFreshRequired
        | ScenarioStepKind::HeartbeatAfterFreshRequired
        | ScenarioStepKind::MalformedCapabilities
        | ScenarioStepKind::FreshSnapshot => {
            step.document.is_some() && step.expected.is_some() && present == 2
        }
        ScenarioStepKind::EventGap => {
            step.document.is_some()
                && step.expected.is_some()
                && step.expected_gap_signal == Some(true)
                && present == 3
        }
        ScenarioStepKind::EventEpochMismatch => {
            step.document.is_some()
                && step.expected.is_some()
                && step.expected_rejected_identity == Some(true)
                && present == 3
        }
        ScenarioStepKind::EventAfterSessionEnded | ScenarioStepKind::ToolTerminalRegression => {
            step.document.is_some()
                && step.expected.is_some()
                && step.expected_illegal_transition == Some(true)
                && present == 3
        }
        ScenarioStepKind::DraftExcluded => step.expected.is_some() && present == 1,
        ScenarioStepKind::TransportDisconnect => {
            step.permanent.is_some() && step.expected.is_some() && present == 2
        }
        ScenarioStepKind::ParallelSnapshot => {
            step.document.is_some()
                && step.expected_primary.is_some()
                && step.expected_secondary.is_some()
                && present == 3
        }
        ScenarioStepKind::ProcessLiveness => {
            step.alive.is_some() && step.expected.is_some() && present == 2
        }
    }
}

fn parse_snapshot_document(document: super::dto::DocumentWire) -> Result<Snapshot, &'static str> {
    match document.into_typed() {
        Ok(super::dto::TypedDocument::Snapshot(snapshot)) => Ok(*snapshot),
        Ok(_) => Err("wrong-document-kind"),
        Err(error) => Err(error.code().as_str()),
    }
}

fn parse_event_document(document: super::dto::DocumentWire) -> Result<EventRecord, &'static str> {
    match document.into_typed() {
        Ok(super::dto::TypedDocument::Event(event)) => Ok(event),
        Ok(_) => Err("wrong-document-kind"),
        Err(error) => Err(error.code().as_str()),
    }
}

fn parse_heartbeat_document(
    document: super::dto::DocumentWire,
) -> Result<HeartbeatRecord, &'static str> {
    match document.into_typed() {
        Ok(super::dto::TypedDocument::Heartbeat(heartbeat)) => Ok(heartbeat),
        Ok(_) => Err("wrong-document-kind"),
        Err(error) => Err(error.code().as_str()),
    }
}

fn check_index(index: u64, position: usize) -> Result<(), ScenarioOracleError> {
    if usize::try_from(index) == Ok(position) {
        Ok(())
    } else {
        Err(fail(format!(
            "step index {index} does not match position {position}"
        )))
    }
}

/// Load the exact scenario package. Duplicate entries, path traversal,
/// unlisted JSON files, extra files, and id/name drift all fail.
pub fn load_scenario_package(directory: &Path) -> Result<Vec<ScenarioEntry>, ScenarioOracleError> {
    let manifest_path = directory.join("manifest.json");
    let manifest = read_bounded_json::<ScenarioManifestWire>(
        &manifest_path,
        MAX_MANIFEST_BYTES,
        "JSP-C-SCENARIO-MANIFEST-READ",
        "JSP-C-SCENARIO-MANIFEST-SHAPE",
    )?;
    if manifest.schema != 1
        || manifest.scenario_artifact_version != super::report::COMPLIANCE_ARTIFACT_VERSION
        || manifest.scenarios.len() != CANONICAL_SCENARIOS.len()
    {
        return Err(fail("JSP-C-SCENARIO-MANIFEST"));
    }
    let mut ids = HashSet::new();
    let mut names = HashSet::new();
    let mut files = HashSet::new();
    let mut entries = Vec::with_capacity(15);
    for (position, entry) in manifest.scenarios.into_iter().enumerate() {
        let canonical = CANONICAL_SCENARIOS[position];
        if (entry.id.as_str(), entry.name.as_str(), entry.file.as_str())
            != (canonical.0, canonical.1, canonical.2)
        {
            return Err(fail("JSP-C-SCENARIO-CANONICAL-IDENTITY"));
        }
        check_manifest_entry(
            &entry.file,
            &mut ids,
            &mut names,
            &mut files,
            &entry.id,
            &entry.name,
        )?;
        entries.push(ScenarioEntry {
            id: entry.id,
            name: entry.name,
            path: directory.join(entry.file),
        });
    }
    check_directory_inventory(directory, &files)?;
    for (position, entry) in entries.iter().enumerate() {
        let scenario = load_scenario(&entry.path)?;
        if scenario.id != entry.id
            || scenario.name != entry.name
            || scenario_signature(&scenario) != CANONICAL_SCENARIOS[position].3
        {
            return Err(fail("JSP-C-SCENARIO-SEMANTIC-INVENTORY"));
        }
    }

    Ok(entries)
}

fn scenario_signature(scenario: &Scenario) -> String {
    scenario
        .steps
        .iter()
        .map(|step| match step {
            ScenarioStep::Event { .. } => "event",
            ScenarioStep::Heartbeat { .. } => "heartbeat",
            ScenarioStep::EventNoop { .. } => "event_noop",
            ScenarioStep::EventBeforeSnapshot { .. } => "event_before_snapshot",
            ScenarioStep::HeartbeatBeforeSnapshot { .. } => "heartbeat_before_snapshot",
            ScenarioStep::EventAfterFreshRequired { .. } => "event_after_fresh_required",
            ScenarioStep::HeartbeatAfterFreshRequired { .. } => "heartbeat_after_fresh_required",
            ScenarioStep::MalformedCapabilities { .. } => "malformed_capabilities",
            ScenarioStep::EventGap { .. } => "event_gap",
            ScenarioStep::EventEpochMismatch { .. } => "event_epoch_mismatch",
            ScenarioStep::EventIllegalTransition { .. } => "event_illegal_transition",
            ScenarioStep::DraftExcluded { .. } => "draft_excluded",
            ScenarioStep::TransportDisconnect { .. } => "transport_disconnect",
            ScenarioStep::FreshSnapshot { .. } => "fresh_snapshot",
            ScenarioStep::ParallelSnapshot { .. } => "parallel_snapshot",
            ScenarioStep::ProcessLiveness { .. } => "process_liveness",
        })
        .collect::<Vec<_>>()
        .join("|")
}
fn check_manifest_entry(
    file: &str,
    ids: &mut HashSet<String>,
    names: &mut HashSet<String>,
    files: &mut HashSet<String>,
    id: &str,
    name: &str,
) -> Result<(), ScenarioOracleError> {
    let path = Path::new(file);
    if path.components().count() != 1
        || !matches!(path.components().next(), Some(Component::Normal(_)))
    {
        return Err(fail(format!(
            "scenario path `{file}` is not a package-local file"
        )));
    }
    if !ids.insert(id.to_string())
        || !names.insert(name.to_string())
        || !files.insert(file.to_string())
    {
        return Err(fail(format!(
            "duplicate scenario manifest entry `{id}`/`{name}`/`{file}`"
        )));
    }
    Ok(())
}

fn check_directory_inventory(
    directory: &Path,
    listed: &HashSet<String>,
) -> Result<(), ScenarioOracleError> {
    let entries = std::fs::read_dir(directory).map_err(|_| fail("JSP-C-SCENARIO-DIR-READ"))?;
    for entry in entries {
        let entry = entry.map_err(|_| fail("JSP-C-SCENARIO-ENTRY"))?;
        // Use symlink-aware metadata so a symlink cannot masquerade as a
        // regular file. `symlink_metadata` does not follow links, so an
        // in-package symlink that points outside the directory is detected as
        // a symlink rather than being resolved to its target type.
        let path = entry.path();
        let metadata =
            std::fs::symlink_metadata(&path).map_err(|_| fail("JSP-C-SCENARIO-ENTRY-METADATA"))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if metadata.is_symlink() {
            return Err(fail("JSP-C-SCENARIO-symlink"));
        }
        if !metadata.is_file() || (name != "manifest.json" && !listed.contains(&name)) {
            return Err(fail("JSP-C-SCENARIO-INVENTORY"));
        }
    }
    Ok(())
}

pub struct ScenarioOracle;

impl ScenarioOracle {
    #[must_use]
    pub fn evaluate(scenario: &Scenario) -> ScenarioResult {
        let mut primary = ReferenceReducer::new();
        primary.apply_snapshot(&scenario.base_snapshot);
        let mut outcomes = Vec::with_capacity(scenario.steps.len());
        for (index, step) in scenario.steps.iter().enumerate() {
            outcomes.push(evaluate_step(&mut primary, index, step));
        }
        ScenarioResult {
            id: scenario.id.clone(),
            name: scenario.name.clone(),
            passed: outcomes.iter().all(|outcome| outcome.passed),
            steps: outcomes,
        }
    }
}

fn evaluate_step(primary: &mut ReferenceReducer, index: usize, step: &ScenarioStep) -> StepOutcome {
    let mut secondary = ReferenceReducer::new();
    let (expected, rejection, secondary_check, gap) = apply_step(primary, &mut secondary, step);
    let actual = primary.projection();
    let failure = rejection
        .or(secondary_check)
        .or_else(|| (actual != expected).then(|| format!("projection mismatch at step {index}")));
    StepOutcome {
        index,
        passed: failure.is_none(),
        expected,
        actual,
        failure,
        expected_sequence: gap.map(|gap| gap.0),
        actual_sequence: gap.map(|gap| gap.1),
    }
}

/// The typed gap values returned from an applied step, if the reducer reported
/// a sequence gap.
type GapTuple = (u64, u64);

/// The complete return shape of an applied scenario step: expected projection,
/// rejection failure, secondary failure, and typed gap values.
type StepTuple = (
    NormalizedProjection,
    Option<String>,
    Option<String>,
    Option<GapTuple>,
);

fn apply_step(
    primary: &mut ReferenceReducer,
    secondary: &mut ReferenceReducer,
    step: &ScenarioStep,
) -> StepTuple {
    match step {
        ScenarioStep::Event { document, expected } => apply_event_step(primary, document, expected),
        ScenarioStep::Heartbeat { document, expected } => (
            expected.clone(),
            primary
                .apply_heartbeat(document)
                .err()
                .map(|error| format!("unexpected rejection: {error}")),
            None,
            None,
        ),
        ScenarioStep::EventNoop { .. }
        | ScenarioStep::EventBeforeSnapshot { .. }
        | ScenarioStep::HeartbeatBeforeSnapshot { .. }
        | ScenarioStep::EventAfterFreshRequired { .. }
        | ScenarioStep::HeartbeatAfterFreshRequired { .. }
        | ScenarioStep::EventIllegalTransition { .. }
        | ScenarioStep::MalformedCapabilities { .. } => {
            apply_assertion_step(primary, secondary, step)
        }
        ScenarioStep::EventGap { document, expected } => {
            let result = primary.apply_event(document);
            let gap = reducer_gap(result.as_ref().err());
            let error = expected_rejection(result, true);
            (expected.clone(), error, None, gap)
        }
        ScenarioStep::EventEpochMismatch { document, expected } => {
            let error = expected_rejection(primary.apply_event(document), false);
            (expected.clone(), error, None, None)
        }
        ScenarioStep::DraftExcluded { expected } => apply_draft_excluded(primary, expected),
        ScenarioStep::TransportDisconnect {
            permanent,
            expected,
        } => {
            primary.apply_disconnect(*permanent);
            (expected.clone(), None, None, None)
        }
        ScenarioStep::FreshSnapshot { document, expected } => {
            primary.apply_snapshot(document);
            (expected.clone(), None, None, None)
        }
        ScenarioStep::ParallelSnapshot {
            document,
            expected_primary,
            expected_secondary,
        } => apply_parallel_snapshot(secondary, document, expected_primary, expected_secondary),
        ScenarioStep::ProcessLiveness { alive, expected } => {
            primary.set_process_alive(*alive);
            (expected.clone(), None, None, None)
        }
    }
}

fn apply_event_step(
    primary: &mut ReferenceReducer,
    document: &EventRecord,
    expected: &NormalizedProjection,
) -> StepTuple {
    let result = primary.apply_event(document);
    let gap = reducer_gap(result.as_ref().err());
    (
        expected.clone(),
        result
            .err()
            .map(|error| format!("unexpected rejection: {error}")),
        None,
        gap,
    )
}

fn apply_draft_excluded(primary: &ReferenceReducer, expected: &NormalizedProjection) -> StepTuple {
    let challenge =
        super::challenge::RunnerChallenge::reference(super::challenge::AdapterKind::Producer, 9);
    let evidence = serde_json::to_vec(&challenge)
        .ok()
        .and_then(|bytes| super::reference_adapter::run(&bytes));
    let executable_passed = evidence.as_ref().is_some_and(|bytes| {
        super::profile::validate_producer_trace_with_challenge(bytes, &challenge).passed
    });
    let projection_clean = serde_json::to_vec(&primary.projection()).is_ok_and(|projection| {
        !projection
            .windows(challenge.draft.marker.len())
            .any(|window| window == challenge.draft.marker.as_bytes())
    });
    (
        expected.clone(),
        (!(executable_passed && projection_clean))
            .then(|| "runner-owned draft exclusion evidence failed".to_string()),
        None,
        None,
    )
}

fn apply_parallel_snapshot(
    secondary: &mut ReferenceReducer,
    document: &Snapshot,
    expected_primary: &NormalizedProjection,
    expected_secondary: &NormalizedProjection,
) -> StepTuple {
    secondary.apply_snapshot(document);
    let mismatch = (secondary.projection() != *expected_secondary)
        .then(|| "secondary projection mismatch".to_string());
    (expected_primary.clone(), None, mismatch, None)
}

fn apply_assertion_step(
    primary: &mut ReferenceReducer,
    secondary: &mut ReferenceReducer,
    step: &ScenarioStep,
) -> StepTuple {
    let (expected, error) = match step {
        ScenarioStep::EventNoop { document, expected } => {
            (expected, apply_noop_assertion(primary, document))
        }
        ScenarioStep::EventBeforeSnapshot { document, expected } => (
            expected,
            (!matches!(
                secondary.apply_event(document),
                Err(ReducerError::SnapshotRequired)
            ))
            .then(|| "event before snapshot was not rejected".to_string()),
        ),
        ScenarioStep::HeartbeatBeforeSnapshot { document, expected } => (
            expected,
            (!matches!(
                secondary.apply_heartbeat(document),
                Err(ReducerError::SnapshotRequired)
            ))
            .then(|| "heartbeat before snapshot was not rejected".to_string()),
        ),
        ScenarioStep::EventAfterFreshRequired { document, expected } => (
            expected,
            (!matches!(
                primary.apply_event(document),
                Err(ReducerError::FreshSnapshotRequired)
            ))
            .then(|| "event applied before fresh snapshot".to_string()),
        ),
        ScenarioStep::HeartbeatAfterFreshRequired { document, expected } => (
            expected,
            (!matches!(
                primary.apply_heartbeat(document),
                Err(ReducerError::FreshSnapshotRequired)
            ))
            .then(|| "heartbeat applied before fresh snapshot".to_string()),
        ),
        ScenarioStep::EventIllegalTransition { document, expected } => {
            (expected, apply_illegal_assertion(primary, document))
        }
        ScenarioStep::MalformedCapabilities { document, expected } => (
            expected,
            crate::jsp::v1::parse_snapshot(document)
                .is_ok()
                .then(|| "malformed capabilities were accepted".to_string()),
        ),
        _ => {
            return (
                primary.projection(),
                Some("wrong assertion step".to_string()),
                None,
                None,
            );
        }
    };
    (expected.clone(), error, None, None)
}

fn apply_noop_assertion(primary: &mut ReferenceReducer, document: &EventRecord) -> Option<String> {
    let before = primary.projection();
    let rejection = primary
        .apply_event(document)
        .err()
        .map(|error| error.to_string());
    let mutation = (primary.projection() != before)
        .then(|| "duplicate/lower event mutated projection".to_string());
    rejection.or(mutation)
}

fn apply_illegal_assertion(
    primary: &mut ReferenceReducer,
    document: &EventRecord,
) -> Option<String> {
    let before = primary.projection();
    let rejected = matches!(
        primary.apply_event(document),
        Err(ReducerError::IllegalTransition { .. })
    );
    let atomic = primary.projection() == before;
    (!(rejected && atomic))
        .then(|| "illegal lifecycle transition mutated state or consumed cursor".to_string())
}

fn expected_rejection(result: Result<(), ReducerError>, gap: bool) -> Option<String> {
    match result {
        Ok(()) => Some("unexpected success; required rejection did not occur".to_string()),
        Err(ReducerError::Gap { .. }) if gap => None,
        Err(ReducerError::IdentityMismatch) if !gap => None,
        Err(error) => Some(format!("wrong rejection: {error}")),
    }
}

/// Extract typed expected/actual sequence values from a gap error without
/// string parsing.
fn reducer_gap(error: Option<&ReducerError>) -> Option<GapTuple> {
    match error {
        Some(ReducerError::Gap { expected, actual }) => Some((*expected, *actual)),
        _ => None,
    }
}

/// Read a bounded JSON artifact with payload-free error codes.
///
/// Checks the file size against `max_bytes` before reading, then deserializes
/// into `T`. Both read and serde errors are mapped to stable payload-free
/// codes so OS error strings and paths never leak into compliance reports.
fn read_bounded_json<T: serde::de::DeserializeOwned>(
    path: &Path,
    max_bytes: u64,
    read_code: &str,
    shape_code: &str,
) -> Result<T, ScenarioOracleError> {
    let metadata = std::fs::metadata(path).map_err(|_| fail(read_code))?;
    if metadata.len() > max_bytes {
        return Err(fail("JSP-C-ARTIFACT-BOUND"));
    }
    let bytes = std::fs::read(path).map_err(|_| fail(read_code))?;
    serde_json::from_slice(&bytes).map_err(|_| fail(shape_code))
}

fn fail(message: impl Into<String>) -> ScenarioOracleError {
    ScenarioOracleError {
        message: message.into(),
    }
}
