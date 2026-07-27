//! Validation helpers for the closed definition contract (issue #382 CW-02).
//!
//! Pure functions that check the closed-schema rules after the typed value is
//! assembled: candidate uniqueness, per-scope field bounds, duplicate field
//! ids, emitter field references and duplicate emitter fields, `visible_when`
//! sibling references plus cycle detection, and probe-spec closed bounds.

use super::diagnostics::{DefinitionError, FieldScope};
use super::fields::Emitter;
use super::limits::{CANDIDATE_LIMIT, EMITTER_LIMIT, FIELD_SCOPE_LIMIT, FORM_FIELD_LIMIT};
use super::probe::validate as validate_probe;
use super::type_id::ExecutableCandidate;

/// Validate the full definition against every closed-schema rule.
///
/// Assumes the typed value was assembled by the bounded JSON reader; checks
/// the cross-field and graph invariants the reader cannot enforce inline.
pub fn validate_definition(
    def: &super::definition::AgentDefinition,
) -> Result<(), DefinitionError> {
    if def.schema != super::definition::DEFINITION_SCHEMA {
        return Err(DefinitionError::SchemaVersion { found: def.schema });
    }
    if def.display_name.is_empty()
        || def.display_name.len() > super::limits::DISPLAY_NAME_BYTE_LIMIT
    {
        return Err(DefinitionError::DisplayNameLength {
            bytes: def.display_name.len(),
        });
    }
    if def.candidates.is_empty() || def.candidates.len() > CANDIDATE_LIMIT {
        return Err(DefinitionError::CandidateBounds {
            len: def.candidates.len(),
        });
    }
    validate_unique_candidates(&def.candidates)?;
    for candidate in &def.candidates {
        candidate
            .validate()
            .map_err(|err| DefinitionError::UnknownField {
                field: format!("candidate invalid: {err}"),
            })?;
    }
    if def.repository_fields.len() > FIELD_SCOPE_LIMIT {
        return Err(DefinitionError::RepositoryFieldBounds {
            len: def.repository_fields.len(),
        });
    }
    if def.agent_fields.len() > FIELD_SCOPE_LIMIT {
        return Err(DefinitionError::AgentFieldBounds {
            len: def.agent_fields.len(),
        });
    }
    let total_fields = def.repository_fields.len() + def.agent_fields.len();
    if total_fields > FORM_FIELD_LIMIT {
        return Err(DefinitionError::TotalFieldBounds { len: total_fields });
    }
    validate_fields(&def.repository_fields, FieldScope::Repository)?;
    validate_fields(&def.agent_fields, FieldScope::Agent)?;
    if def.emitters.len() > EMITTER_LIMIT {
        return Err(DefinitionError::EmitterBounds {
            len: def.emitters.len(),
        });
    }
    validate_emitters(&def.emitters, &def.repository_fields, &def.agent_fields)?;
    validate_package_selector_contract(def)?;
    validate_probe(&def.probe).map_err(|err| DefinitionError::Probe(Box::new(err)))?;
    validate_visibility_graph(&def.repository_fields)?;
    validate_visibility_graph(&def.agent_fields)?;
    Ok(())
}

fn validate_unique_candidates(candidates: &[ExecutableCandidate]) -> Result<(), DefinitionError> {
    let mut seen: Vec<(String, std::path::PathBuf)> = Vec::with_capacity(candidates.len());
    for (index, candidate) in candidates.iter().enumerate() {
        let key = candidate_key(candidate);
        if seen.iter().any(|(k, v)| k == &key.0 && v == &key.1) {
            return Err(DefinitionError::DuplicateCandidate { index });
        }
        seen.push(key);
    }
    Ok(())
}

fn candidate_key(candidate: &ExecutableCandidate) -> (String, std::path::PathBuf) {
    let kind = match &candidate.kind {
        super::type_id::CandidateKind::PathName { name } => format!("path-name:{name}"),
        super::type_id::CandidateKind::RepositoryLlxprt => "repository-llxprt".to_string(),
        super::type_id::CandidateKind::NpmPackage { package, binary } => {
            format!("npm-package:{package}/{binary}")
        }
        super::type_id::CandidateKind::UvxPackage { package, binary } => {
            format!("uvx-package:{package}/{binary}")
        }
    };
    (kind, candidate.value.clone())
}

fn validate_fields(
    fields: &[super::fields::Field],
    scope: FieldScope,
) -> Result<(), DefinitionError> {
    let mut seen: Vec<String> = Vec::with_capacity(fields.len());
    for (index, field) in fields.iter().enumerate() {
        if seen.iter().any(|existing| existing == &field.id) {
            return Err(DefinitionError::DuplicateFieldId {
                scope,
                id: field.id.clone(),
                index,
            });
        }
        seen.push(field.id.clone());
    }
    Ok(())
}

fn validate_emitters(
    emitters: &[Emitter],
    repository_fields: &[super::fields::Field],
    agent_fields: &[super::fields::Field],
) -> Result<(), DefinitionError> {
    let all_ids: Vec<&str> = repository_fields
        .iter()
        .map(|f| f.id.as_str())
        .chain(agent_fields.iter().map(|f| f.id.as_str()))
        .collect();
    let mut referenced: Vec<String> = Vec::new();
    for (index, emitter) in emitters.iter().enumerate() {
        if let Some(field) = emitter.field() {
            if !all_ids.contains(&field) {
                return Err(DefinitionError::UnknownEmitterField {
                    index,
                    field: field.to_string(),
                });
            }
            if referenced.iter().any(|existing| existing == field) {
                return Err(DefinitionError::DuplicateEmitterField {
                    index,
                    field: field.to_string(),
                });
            }
            referenced.push(field.to_string());
        }
    }
    Ok(())
}

const PACKAGE_SELECTOR_FIELD_ID: &str = "version_selector";

fn validate_package_selector_contract(
    def: &super::definition::AgentDefinition,
) -> Result<(), DefinitionError> {
    let has_package_candidate = def
        .candidates
        .iter()
        .any(|candidate| candidate.kind.is_package_runner());
    let selector_fields = def
        .agent_fields
        .iter()
        .filter(|field| field.id == PACKAGE_SELECTOR_FIELD_ID)
        .collect::<Vec<_>>();
    let selector_emitted = def
        .emitters
        .iter()
        .any(|emitter| emitter.field() == Some(PACKAGE_SELECTOR_FIELD_ID));
    let valid_selector = match selector_fields.as_slice() {
        [field] => {
            field.kind == super::fields::FieldKind::String
                && field.launch_signature
                && !field.required
                && field.default.is_none()
                && !selector_emitted
        }
        _ => false,
    };
    if has_package_candidate == valid_selector
        && (has_package_candidate || selector_fields.is_empty())
    {
        return Ok(());
    }
    Err(DefinitionError::UnknownField {
        field: "package candidates require one optional, non-emitting, signature-bearing agent string field named version_selector".to_string(),
    })
}

fn validate_visibility_graph(fields: &[super::fields::Field]) -> Result<(), DefinitionError> {
    let ids: Vec<String> = fields.iter().map(|f| f.id.clone()).collect();
    for (index, field) in fields.iter().enumerate() {
        if let Some(visible_when) = &field.visible_when
            && !ids.iter().any(|id| id == visible_when)
        {
            return Err(DefinitionError::UnknownVisibleWhen {
                index,
                id: visible_when.clone(),
            });
        }
    }
    for field in fields {
        let mut path = Vec::new();
        let mut current = Some(field.id.clone());
        while let Some(node) = current {
            if path.iter().any(|visited: &String| visited == &node) {
                path.push(node);
                return Err(DefinitionError::VisibilityCycle { path });
            }
            path.push(node.clone());
            let next = fields
                .iter()
                .find(|f| f.id == node)
                .and_then(|f| f.visible_when.clone());
            current = next;
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "validation_tests.rs"]
mod tests;
