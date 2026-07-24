//! Strict schema-2 state JSON authority.
//!
//! The domain layer owns the durable DTOs. This module owns bounded reads,
//! duplicate/unknown-field rejection, reference validation, and deterministic
//! serialization. It performs no migration and never writes while reading.

use std::collections::BTreeSet;

use serde_json::Value;

use super::diagnostic::{
    ARRAY_LIMIT, CfgCode, DIAGNOSTIC_LIMIT, Diagnostic, DiagnosticPath, FILE_LIMIT, MAP_LIMIT,
    NESTING_LIMIT, STRING_LIMIT, Severity,
};
use crate::domain::{Id, STATE_SCHEMA_V2, StateV2};

/// Parsed and validated schema-2 state candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateDocument {
    state: StateV2,
}

impl StateDocument {
    /// Parse bounded schema-2 JSON without changing the source.
    pub fn parse(bytes: &[u8]) -> Result<Self, Vec<Diagnostic>> {
        if bytes.len() > FILE_LIMIT {
            return Err(vec![limit_diagnostic("/", bytes.len(), FILE_LIMIT)]);
        }
        if let Err(error) = super::state_json::reject_duplicate_keys(bytes) {
            return Err(vec![malformed_diagnostic(error.to_string())]);
        }
        let value: Value = serde_json::from_slice(bytes)
            .map_err(|error| vec![malformed_diagnostic(error.to_string())])?;
        validate_json_bounds(&value, 1, "")?;
        let state: StateV2 = serde_json::from_value(value)
            .map_err(|error| vec![malformed_diagnostic(error.to_string())])?;
        let diagnostics = validate_state(&state);
        if diagnostics.is_empty() {
            Ok(Self { state })
        } else {
            Err(diagnostics)
        }
    }

    /// Borrow the immutable durable candidate.
    #[must_use]
    pub const fn state(&self) -> &StateV2 {
        &self.state
    }

    /// Serialize deterministic pretty JSON with a trailing newline.
    pub fn to_canonical_json(&self) -> Result<Vec<u8>, serde_json::Error> {
        let mut encoded = serde_json::to_vec_pretty(&self.state)?;
        encoded.push(b'\n');
        Ok(encoded)
    }
}

fn validate_state(state: &StateV2) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    if state.state_schema != STATE_SCHEMA_V2 {
        diagnostics.push(malformed_at(
            "/state_schema",
            "use state_schema 2",
            "unsupported state schema",
        ));
    }
    let repository_ids = collect_unique_ids(
        state.repositories.iter().map(|record| &record.id),
        "/repositories",
        &mut diagnostics,
    );
    let agent_ids = collect_unique_ids(
        state.agents.iter().map(|record| &record.id),
        "/agents",
        &mut diagnostics,
    );
    validate_agent_references(state, &repository_ids, &mut diagnostics);
    validate_selection(state, &repository_ids, &agent_ids, &mut diagnostics);
    validate_last_selected(state, &repository_ids, &agent_ids, &mut diagnostics);
    validate_repository_preferences(state, &repository_ids, &mut diagnostics);
    validate_signature_versions(state, &mut diagnostics);
    diagnostics.sort();
    if diagnostics.len() > DIAGNOSTIC_LIMIT {
        vec![limit_diagnostic(
            "/diagnostics",
            diagnostics.len(),
            DIAGNOSTIC_LIMIT,
        )]
    } else {
        diagnostics
    }
}

fn collect_unique_ids<'a>(
    ids: impl Iterator<Item = &'a Id>,
    path: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> BTreeSet<Id> {
    let mut seen = BTreeSet::new();
    for id in ids {
        if !seen.insert(id.clone()) {
            diagnostics.push(reference_diagnostic(
                path,
                format!("duplicate id {id}"),
                "assign a unique stable id",
            ));
        }
    }
    seen
}

fn validate_agent_references(
    state: &StateV2,
    repositories: &BTreeSet<Id>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for (index, agent) in state.agents.iter().enumerate() {
        if !repositories.contains(&agent.repository_id) {
            diagnostics.push(reference_diagnostic(
                &format!("/agents/{index}/repository_id"),
                "agent repository does not exist",
                "reference a repository id in this state document",
            ));
        }
    }
}

fn validate_selection(
    state: &StateV2,
    repositories: &BTreeSet<Id>,
    agents: &BTreeSet<Id>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    validate_optional_reference(
        state.selection.repository_id.as_ref(),
        repositories,
        "/selection/repository_id",
        diagnostics,
    );
    validate_optional_reference(
        state.selection.agent_id.as_ref(),
        agents,
        "/selection/agent_id",
        diagnostics,
    );
    if let Some(agent_id) = &state.selection.agent_id
        && let Some(repository_id) = &state.selection.repository_id
        && state
            .agents
            .iter()
            .any(|agent| &agent.id == agent_id && &agent.repository_id != repository_id)
    {
        diagnostics.push(reference_diagnostic(
            "/selection",
            "selected agent is not owned by the selected repository",
            "select a matching repository and agent",
        ));
    }
}

fn validate_optional_reference(
    value: Option<&Id>,
    known: &BTreeSet<Id>,
    path: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if value.is_some_and(|id| !known.contains(id)) {
        diagnostics.push(reference_diagnostic(
            path,
            "referenced id does not exist",
            "reference an id in this state document or remove the selection",
        ));
    }
}

fn validate_last_selected(
    state: &StateV2,
    repositories: &BTreeSet<Id>,
    agents: &BTreeSet<Id>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for (repository_id, agent_id) in &state.last_selected_agent_by_repo {
        let path = format!("/last_selected_agent_by_repo/{repository_id}");
        if !repositories.contains(repository_id) || !agents.contains(agent_id) {
            diagnostics.push(reference_diagnostic(
                &path,
                "remembered selection references a missing id",
                "reference existing repository and agent ids",
            ));
        } else if state
            .agents
            .iter()
            .any(|agent| &agent.id == agent_id && &agent.repository_id != repository_id)
        {
            diagnostics.push(reference_diagnostic(
                &path,
                "remembered agent belongs to a different repository",
                "reference an agent owned by the map key repository",
            ));
        }
    }
}

fn validate_repository_preferences(
    state: &StateV2,
    repositories: &BTreeSet<Id>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for repository_id in state.preferences.repository_preferences.keys() {
        if !repositories.contains(repository_id) {
            diagnostics.push(reference_diagnostic(
                &format!("/preferences/repository_preferences/{repository_id}"),
                "preferences reference a missing repository",
                "remove the entry or restore the repository",
            ));
        }
    }
}

fn validate_signature_versions(state: &StateV2, diagnostics: &mut Vec<Diagnostic>) {
    for (index, agent) in state.agents.iter().enumerate() {
        if agent.launch_signature.version != 1 {
            diagnostics.push(malformed_at(
                &format!("/agents/{index}/launch_signature/version"),
                "use launch signature version 1",
                "unsupported launch signature version",
            ));
        }
    }
}

pub(super) fn validate_json_bounds(
    value: &Value,
    depth: usize,
    path: &str,
) -> Result<(), Vec<Diagnostic>> {
    if depth > NESTING_LIMIT {
        return Err(vec![limit_diagnostic(
            path_or_root(path),
            depth,
            NESTING_LIMIT,
        )]);
    }
    match value {
        Value::String(text) if text.len() > STRING_LIMIT => Err(vec![limit_diagnostic(
            path_or_root(path),
            text.len(),
            STRING_LIMIT,
        )]),
        Value::Array(items) => {
            if items.len() > ARRAY_LIMIT {
                return Err(vec![limit_diagnostic(
                    path_or_root(path),
                    items.len(),
                    ARRAY_LIMIT,
                )]);
            }
            for (index, item) in items.iter().enumerate() {
                validate_json_bounds(item, depth + 1, &format!("{path}/{index}"))?;
            }
            Ok(())
        }
        Value::Object(items) => {
            if items.len() > MAP_LIMIT {
                return Err(vec![limit_diagnostic(
                    path_or_root(path),
                    items.len(),
                    MAP_LIMIT,
                )]);
            }
            for (key, item) in items {
                if key.len() > STRING_LIMIT {
                    return Err(vec![limit_diagnostic(
                        &format!("{path}/{key}"),
                        key.len(),
                        STRING_LIMIT,
                    )]);
                }
                validate_json_bounds(item, depth + 1, &format!("{path}/{key}"))?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn path_or_root(path: &str) -> &str {
    if path.is_empty() { "/" } else { path }
}

fn malformed_diagnostic(detail: String) -> Diagnostic {
    malformed_at("/", "provide a strict schema-2 state document", detail)
}

fn malformed_at(path: &str, correction: &str, detail: impl Into<String>) -> Diagnostic {
    let mut diagnostic = Diagnostic::new(
        CfgCode::E103,
        Severity::Error,
        DiagnosticPath::new(path),
        None,
        correction,
    );
    diagnostic.redacted_detail = detail.into();
    diagnostic
}

fn reference_diagnostic(path: &str, detail: impl Into<String>, correction: &str) -> Diagnostic {
    let mut diagnostic = Diagnostic::new(
        CfgCode::E006,
        Severity::Error,
        DiagnosticPath::new(path),
        None,
        correction,
    );
    diagnostic.redacted_detail = detail.into();
    diagnostic
}

fn limit_diagnostic(path: &str, actual: usize, limit: usize) -> Diagnostic {
    let mut diagnostic = Diagnostic::new(
        CfgCode::E008,
        Severity::Error,
        DiagnosticPath::new(path),
        None,
        format!("reduce the value to at most {limit}"),
    );
    diagnostic.redacted_detail = format!("observed {actual}; inclusive limit {limit}");
    diagnostic
}
