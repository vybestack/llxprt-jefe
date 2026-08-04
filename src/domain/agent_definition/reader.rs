//! Bounded JSON reader mapping the closed schema to typed values (issue #382).
//!
//! This module is the deserialize half of [`AgentDefinition::from_bytes`]: it
//! walks the [`BoundedJson`] tree produced by the bounded reader, rejects
//! unknown/duplicate fields at the JSON level, maps each closed field into the
//! typed value, and returns the first [`DefinitionError`] for any violation
//! the reader can detect inline. Cross-field graph invariants are checked by
//! [`super::validation::validate_definition`].

use std::collections::HashSet;

use super::bounded_json::{BoundedJson, parse_definition_json};
use super::diagnostics::DefinitionError;
use super::fields::{Emitter, Field, FieldKind, FieldValue};
use super::limits::{CANDIDATE_LIMIT, DEFINITION_SCHEMA, STRING_VALUE_BYTE_LIMIT};
use super::probe::{AnchoredPattern, IdentityRecognizer, ProbeFraming, ProbeSpec, ProbeStream};
use super::type_id::{AgentTypeId, CandidateKind, ExecutableCandidate};

/// Closed definition field set at the top level (used by the JSON reader).
const TOP_LEVEL_FIELDS: &[&str] = &[
    "agent_type_schema",
    "id",
    "display_name",
    "executable_candidates",
    "probe",
    "operations",
    "targets",
    "repository_fields",
    "agent_fields",
    "emitters",
];

/// Parse bytes into a bounded JSON tree and map it into a typed definition.
///
/// Returns the assembled value after the cross-field validation in
/// [`super::validation::validate_definition`] passes.
pub fn read_definition(
    input: &[u8],
) -> Result<super::definition::AgentDefinition, DefinitionError> {
    let json = parse_definition_json(input)?;
    read_definition_from_json(&json)
}

fn read_definition_from_json(
    json: &BoundedJson,
) -> Result<super::definition::AgentDefinition, DefinitionError> {
    let object = json
        .as_object()
        .ok_or_else(|| unknown("definition must be a JSON object"))?;
    reject_unknown_fields(object, &top_level_field_set())?;
    let schema = require_schema_version(object)?;
    let id_raw = require_string(object, "id")?;
    let id = AgentTypeId::parse(&id_raw)?;
    let display_name = require_string(object, "display_name")?;
    let candidates = read_candidates(object)?;
    let probe = read_probe(object)?;
    let operations = read_operations(object)?;
    let targets = read_targets(object)?;
    let repository_fields = read_fields(object, "repository_fields")?;
    let agent_fields = read_fields(object, "agent_fields")?;
    let emitters = read_emitters(object)?;
    let def = super::definition::AgentDefinition {
        schema,
        id,
        display_name,
        candidates,
        probe,
        operations,
        targets,
        repository_fields,
        agent_fields,
        emitters,
    };
    super::validation::validate_definition(&def)?;
    Ok(def)
}

fn top_level_field_set() -> HashSet<&'static str> {
    TOP_LEVEL_FIELDS.iter().copied().collect()
}

fn reject_unknown_fields(
    object: &[(String, BoundedJson)],
    allowed: &HashSet<&str>,
) -> Result<(), DefinitionError> {
    for (key, _) in object {
        if !allowed.contains(key.as_str()) {
            return Err(unknown(key.clone()));
        }
    }
    Ok(())
}

fn unknown(what: impl Into<String>) -> DefinitionError {
    DefinitionError::UnknownField { field: what.into() }
}

fn require_string(object: &[(String, BoundedJson)], key: &str) -> Result<String, DefinitionError> {
    let value = object
        .iter()
        .find(|(k, _)| k == key)
        .ok_or_else(|| unknown(format!("missing required field {key:?}")))?;
    match &value.1 {
        BoundedJson::Str(s) => {
            if s.len() > super::limits::PATH_LIMIT {
                return Err(unknown(format!("{key} exceeds path limit")));
            }
            Ok(s.clone())
        }
        other => Err(unknown(format!(
            "{key} must be a string, found {kind}",
            kind = bounded_kind(other)
        ))),
    }
}

fn bounded_kind(value: &BoundedJson) -> &'static str {
    match value {
        BoundedJson::Null => "null",
        BoundedJson::Bool(_) => "bool",
        BoundedJson::Int(_) => "int",
        BoundedJson::Str(_) => "string",
        BoundedJson::Array(_) => "array",
        BoundedJson::Object(_) => "object",
    }
}

fn require_schema_version(object: &[(String, BoundedJson)]) -> Result<u16, DefinitionError> {
    let value = object
        .iter()
        .find(|(k, _)| k == "agent_type_schema")
        .ok_or_else(|| unknown("missing required field \"agent_type_schema\""))?;
    match &value.1 {
        BoundedJson::Int(i) => {
            let found =
                u16::try_from(*i).map_err(|_| unknown("agent_type_schema must fit in u16"))?;
            if found != DEFINITION_SCHEMA {
                return Err(DefinitionError::SchemaVersion { found });
            }
            Ok(found)
        }
        other => Err(unknown(format!(
            "agent_type_schema must be an integer, found {}",
            bounded_kind(other)
        ))),
    }
}

fn read_candidates(
    object: &[(String, BoundedJson)],
) -> Result<Vec<ExecutableCandidate>, DefinitionError> {
    let raw = object
        .iter()
        .find(|(k, _)| k == "executable_candidates")
        .ok_or_else(|| unknown("missing required field \"executable_candidates\""))?;
    let arr = raw
        .1
        .as_array()
        .ok_or_else(|| unknown("executable_candidates must be an array"))?;
    if arr.is_empty() || arr.len() > CANDIDATE_LIMIT {
        return Err(DefinitionError::CandidateBounds { len: arr.len() });
    }
    let mut candidates = Vec::with_capacity(arr.len());
    for element in arr {
        candidates.push(read_candidate(element)?);
    }
    Ok(candidates)
}

fn read_candidate(value: &BoundedJson) -> Result<ExecutableCandidate, DefinitionError> {
    let object = value
        .as_object()
        .ok_or_else(|| unknown("candidate must be an object"))?;
    let allowed: HashSet<&str> = ["kind", "value", "package", "binary", "name"]
        .into_iter()
        .collect();
    reject_unknown_fields(object, &allowed)?;
    let kind_raw = require_string(object, "kind")?;
    match kind_raw.as_str() {
        "path-name" => {
            let name = optional_string(object, "name")
                .or_else(|| optional_string(object, "value"))
                .ok_or_else(|| unknown("path-name candidate requires \"value\""))?;
            Ok(ExecutableCandidate {
                kind: CandidateKind::PathName { name: name.clone() },
                value: std::path::PathBuf::from(name),
            })
        }
        "repository-llxprt" => {
            let value = require_string(object, "value")?;
            Ok(ExecutableCandidate {
                kind: CandidateKind::RepositoryLlxprt,
                value: std::path::PathBuf::from(value),
            })
        }
        "npm-package" => {
            let package = require_string(object, "package")?;
            let binary = require_string(object, "binary")?;
            Ok(ExecutableCandidate {
                kind: CandidateKind::NpmPackage { package, binary },
                value: std::path::PathBuf::from("npm"),
            })
        }
        "uvx-package" => {
            let package = require_string(object, "package")?;
            let binary = require_string(object, "binary")?;
            Ok(ExecutableCandidate {
                kind: CandidateKind::UvxPackage { package, binary },
                value: std::path::PathBuf::from("uvx"),
            })
        }
        other => Err(unknown(format!("unknown candidate kind {other:?}"))),
    }
}

fn optional_string(object: &[(String, BoundedJson)], key: &str) -> Option<String> {
    object.iter().find_map(|(k, v)| {
        if k == key {
            if let BoundedJson::Str(s) = v {
                Some(s.clone())
            } else {
                None
            }
        } else {
            None
        }
    })
}

fn read_probe(object: &[(String, BoundedJson)]) -> Result<ProbeSpec, DefinitionError> {
    let raw = object
        .iter()
        .find(|(k, _)| k == "probe")
        .ok_or_else(|| unknown("missing required field \"probe\""))?;
    map_probe(&raw.1)
}

fn map_probe(value: &BoundedJson) -> Result<ProbeSpec, DefinitionError> {
    let object = value
        .as_object()
        .ok_or_else(|| unknown("probe must be an object"))?;
    let allowed: HashSet<&str> = [
        "argv",
        "stream",
        "framing",
        "identity",
        "normalize",
        "timeout_ms",
        "max_bytes",
    ]
    .into_iter()
    .collect();
    reject_unknown_fields(object, &allowed)?;
    let argv = string_array(object, "argv")?;
    if argv.is_empty() || argv.len() > super::limits::PROBE_ARGV_LIMIT {
        return Err(DefinitionError::Probe(Box::new(
            super::probe::ProbeValidateError::ArgvBounds { len: argv.len() },
        )));
    }
    let stream = read_probe_stream(object)?;
    let framing = read_probe_framing(object)?;
    let identity = read_identity(object)?;
    let normalize = read_normalize(object)?;
    let timeout_ms =
        u64_field(object, "timeout_ms")?.unwrap_or(super::limits::LOCAL_PROBE_TIMEOUT_MS);
    let max_bytes = usize_field(object, "max_bytes")?.unwrap_or(super::limits::PROBE_STREAM_LIMIT);
    Ok(ProbeSpec {
        argv,
        stream,
        framing,
        identity,
        normalize,
        timeout_ms,
        max_bytes,
    })
}

fn read_identity(object: &[(String, BoundedJson)]) -> Result<IdentityRecognizer, DefinitionError> {
    let raw = object
        .iter()
        .find(|(k, _)| k == "identity")
        .ok_or_else(|| unknown("missing required field \"identity\""))?;
    let id_obj = raw
        .1
        .as_object()
        .ok_or_else(|| unknown("identity must be an object"))?;
    let allowed: HashSet<&str> = ["kind", "pointer", "prefix", "anchored_pattern"]
        .into_iter()
        .collect();
    reject_unknown_fields(id_obj, &allowed)?;
    let kind = require_string(id_obj, "kind")?;
    let anchored = read_anchored_pattern(id_obj)?;
    match kind.as_str() {
        "json_pointer" => {
            let pointer = require_string(id_obj, "pointer")?;
            Ok(IdentityRecognizer::JsonPointer {
                pointer,
                anchored_pattern: anchored,
            })
        }
        "line" => {
            let prefix = optional_string(id_obj, "prefix").unwrap_or_default();
            Ok(IdentityRecognizer::Line {
                prefix,
                anchored_pattern: anchored,
            })
        }
        other => Err(unknown(format!("unknown identity kind {other:?}"))),
    }
}

fn read_anchored_pattern(
    object: &[(String, BoundedJson)],
) -> Result<AnchoredPattern, DefinitionError> {
    let raw = object
        .iter()
        .find(|(k, _)| k == "anchored_pattern")
        .ok_or_else(|| unknown("missing required field \"anchored_pattern\""))?;
    let ap_obj = raw
        .1
        .as_object()
        .ok_or_else(|| unknown("anchored_pattern must be an object"))?;
    let allowed: HashSet<&str> = ["kind", "value", "prefix", "suffix"].into_iter().collect();
    reject_unknown_fields(ap_obj, &allowed)?;
    let kind = require_string(ap_obj, "kind")?;
    match kind.as_str() {
        "exact" => {
            let value = require_string(ap_obj, "value")?;
            Ok(AnchoredPattern::Exact { value })
        }
        "prefix" => {
            let prefix = require_string(ap_obj, "prefix")?;
            Ok(AnchoredPattern::Prefix { prefix })
        }
        "suffix" => {
            let suffix = require_string(ap_obj, "suffix")?;
            Ok(AnchoredPattern::Suffix { suffix })
        }
        "prefix_suffix" => {
            let prefix = require_string(ap_obj, "prefix")?;
            let suffix = require_string(ap_obj, "suffix")?;
            Ok(AnchoredPattern::PrefixSuffix { prefix, suffix })
        }
        "version_token" => Ok(AnchoredPattern::VersionToken),
        other => Err(unknown(format!("unknown anchored pattern kind {other:?}"))),
    }
}

fn read_normalize(
    object: &[(String, BoundedJson)],
) -> Result<super::normalize::Normalize, DefinitionError> {
    let Some(raw) = object.iter().find(|(k, _)| k == "normalize") else {
        return Ok(super::normalize::Normalize::None);
    };
    match &raw.1 {
        BoundedJson::Str(s) => match s.as_str() {
            "none" => Ok(super::normalize::Normalize::None),
            "strip_ansi" => Ok(super::normalize::Normalize::StripAnsi),
            other => Err(unknown(format!("unknown normalize kind {other:?}"))),
        },
        other => Err(unknown(format!(
            "normalize must be a string, found {}",
            bounded_kind(other)
        ))),
    }
}

fn read_operations(
    object: &[(String, BoundedJson)],
) -> Result<super::types::OperationMatrix, DefinitionError> {
    let raw = object
        .iter()
        .find(|(k, _)| k == "operations")
        .ok_or_else(|| unknown("missing required field \"operations\""))?;
    let ops_obj = raw
        .1
        .as_object()
        .ok_or_else(|| unknown("operations must be an object"))?;
    let allowed: HashSet<&str> = ["normal", "resume", "fresh_issue", "fresh_pull_request"]
        .into_iter()
        .collect();
    reject_unknown_fields(ops_obj, &allowed)?;
    Ok(super::types::OperationMatrix {
        normal: read_operation_support(ops_obj, "normal")?,
        resume: read_operation_support(ops_obj, "resume")?,
        fresh_issue: read_operation_support(ops_obj, "fresh_issue")?,
        fresh_pull_request: read_operation_support(ops_obj, "fresh_pull_request")?,
    })
}

fn read_operation_support(
    object: &[(String, BoundedJson)],
    key: &str,
) -> Result<super::types::OperationSupport, DefinitionError> {
    let Some(raw) = object.iter().find(|(k, _)| k == key) else {
        return Ok(super::types::OperationSupport::default());
    };
    let op_obj = raw
        .1
        .as_object()
        .ok_or_else(|| unknown(format!("operation {key:?} must be an object")))?;
    let allowed: HashSet<&str> = ["supported", "prompt", "reason"].into_iter().collect();
    reject_unknown_fields(op_obj, &allowed)?;
    let support = match op_obj
        .iter()
        .find(|(k, _)| k == "supported")
        .map(|(_, v)| v)
    {
        Some(BoundedJson::Bool(true)) => super::types::Support::supported(),
        Some(BoundedJson::Bool(false)) => {
            let reason = optional_string(op_obj, "reason").unwrap_or_default();
            super::types::Support::unsupported(reason)
        }
        Some(_) => {
            return Err(unknown(format!(
                "operation {key:?} supported must be boolean"
            )));
        }
        None => super::types::Support::unsupported("operation not declared"),
    };
    let prompt = match op_obj.iter().find(|(k, _)| k == "prompt").map(|(_, v)| v) {
        Some(BoundedJson::Str(s)) => match s.as_str() {
            "none" | "" => super::types::PromptShape::None,
            "initial_positional" => super::types::PromptShape::InitialPositional,
            "interactive_option" => super::types::PromptShape::InteractiveOption,
            other => return Err(unknown(format!("unknown prompt shape {other:?}"))),
        },
        Some(_) => {
            return Err(unknown(format!(
                "operation {key:?} prompt must be a string"
            )));
        }
        None => super::types::PromptShape::None,
    };
    Ok(super::types::OperationSupport {
        supported: support,
        prompt,
    })
}

fn read_targets(
    object: &[(String, BoundedJson)],
) -> Result<super::types::TargetMatrix, DefinitionError> {
    let raw = object
        .iter()
        .find(|(k, _)| k == "targets")
        .ok_or_else(|| unknown("missing required field \"targets\""))?;
    let tgt_obj = raw
        .1
        .as_object()
        .ok_or_else(|| unknown("targets must be an object"))?;
    let allowed: HashSet<&str> = ["local", "remote"].into_iter().collect();
    reject_unknown_fields(tgt_obj, &allowed)?;
    Ok(super::types::TargetMatrix {
        local: read_target_support(tgt_obj, "local")?,
        remote: read_target_support(tgt_obj, "remote")?,
    })
}

fn read_target_support(
    object: &[(String, BoundedJson)],
    key: &str,
) -> Result<super::types::TargetSupport, DefinitionError> {
    let Some(raw) = object.iter().find(|(k, _)| k == key) else {
        return Ok(super::types::TargetSupport::default());
    };
    let t_obj = raw
        .1
        .as_object()
        .ok_or_else(|| unknown(format!("target {key:?} must be an object")))?;
    let allowed: HashSet<&str> = ["supported", "reason"].into_iter().collect();
    reject_unknown_fields(t_obj, &allowed)?;
    let support = match t_obj.iter().find(|(k, _)| k == "supported").map(|(_, v)| v) {
        Some(BoundedJson::Bool(true)) => super::types::Support::supported(),
        Some(BoundedJson::Bool(false)) => {
            let reason = optional_string(t_obj, "reason").unwrap_or_default();
            super::types::Support::unsupported(reason)
        }
        Some(_) => return Err(unknown(format!("target {key:?} supported must be boolean"))),
        None => super::types::Support::unsupported("target not declared"),
    };
    Ok(super::types::TargetSupport { supported: support })
}

fn read_fields(object: &[(String, BoundedJson)], key: &str) -> Result<Vec<Field>, DefinitionError> {
    let Some(raw) = object.iter().find(|(k, _)| k == key) else {
        return Ok(Vec::new());
    };
    if raw.1.is_null() {
        return Ok(Vec::new());
    }
    let arr = raw
        .1
        .as_array()
        .ok_or_else(|| unknown(format!("{key} must be an array")))?;
    let mut fields = Vec::with_capacity(arr.len());
    for (index, element) in arr.iter().enumerate() {
        fields.push(read_field(element, index)?);
    }
    Ok(fields)
}

fn read_field(value: &BoundedJson, index: usize) -> Result<Field, DefinitionError> {
    let object = value
        .as_object()
        .ok_or_else(|| unknown(format!("field at index {index} must be an object")))?;
    let allowed: HashSet<&str> = [
        "id",
        "kind",
        "required",
        "default",
        "minimum",
        "maximum",
        "choices",
        "visible_when",
        "launch_signature",
    ]
    .into_iter()
    .collect();
    reject_unknown_fields(object, &allowed)?;
    let id = require_string(object, "id")?;
    let kind_str = require_string(object, "kind")?;
    let kind = match kind_str.as_str() {
        "boolean" => FieldKind::Boolean,
        "optional_boolean" => FieldKind::OptionalBoolean,
        "string" => FieldKind::String,
        "integer" => FieldKind::Integer,
        "enum" => FieldKind::Enum,
        "path" => FieldKind::Path,
        "string_list" => FieldKind::StringList,
        other => return Err(unknown(format!("unknown field kind {other:?}"))),
    };
    let required = bool_field(object, "required")?.unwrap_or(false);
    let default = read_default(object, kind)?;
    let minimum = i64_field(object, "minimum")?;
    let maximum = i64_field(object, "maximum")?;
    let choices = string_array(object, "choices")?;
    let visible_when = optional_string(object, "visible_when");
    let launch_signature = bool_field(object, "launch_signature")?.unwrap_or(false);
    let field = Field {
        id,
        kind,
        required,
        default,
        minimum,
        maximum,
        choices,
        visible_when,
        launch_signature,
    };
    field
        .validate()
        .map_err(|error| DefinitionError::Field { index, error })?;
    Ok(field)
}

fn read_default(
    object: &[(String, BoundedJson)],
    kind: FieldKind,
) -> Result<Option<FieldValue>, DefinitionError> {
    let Some(raw) = object.iter().find(|(k, _)| k == "default") else {
        return Ok(None);
    };
    if raw.1.is_null() {
        return Ok(None);
    }
    let value = match &raw.1 {
        BoundedJson::Bool(b) => FieldValue::Boolean(*b),
        BoundedJson::Int(i) => FieldValue::Integer(*i),
        BoundedJson::Str(s) => match kind {
            FieldKind::Path => FieldValue::Path(s.clone()),
            _ => FieldValue::String(s.clone()),
        },
        BoundedJson::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                let BoundedJson::Str(s) = item else {
                    return Err(unknown("string_list default must contain only strings"));
                };
                out.push(s.clone());
            }
            FieldValue::StringList(out)
        }
        BoundedJson::Object(_) | BoundedJson::Null => {
            return Err(unknown("default must be a scalar or array"));
        }
    };
    if !value.matches_kind(kind) {
        return Err(unknown("default does not match field kind"));
    }
    Ok(Some(value))
}

fn read_emitters(object: &[(String, BoundedJson)]) -> Result<Vec<Emitter>, DefinitionError> {
    let Some(raw) = object.iter().find(|(k, _)| k == "emitters") else {
        return Ok(Vec::new());
    };
    if raw.1.is_null() {
        return Ok(Vec::new());
    }
    let arr = raw
        .1
        .as_array()
        .ok_or_else(|| unknown("emitters must be an array"))?;
    let mut emitters = Vec::with_capacity(arr.len());
    for element in arr {
        emitters.push(read_emitter(element)?);
    }
    Ok(emitters)
}

fn read_emitter(value: &BoundedJson) -> Result<Emitter, DefinitionError> {
    let object = value
        .as_object()
        .ok_or_else(|| unknown("emitter must be an object"))?;
    let allowed: HashSet<&str> = [
        "kind",
        "value",
        "field",
        "name",
        "true_value",
        "false_value",
    ]
    .into_iter()
    .collect();
    reject_unknown_fields(object, &allowed)?;
    let kind = require_string(object, "kind")?;
    let emitter = match kind.as_str() {
        "fixed" => Emitter::Fixed {
            value: require_string(object, "value")?,
        },
        "flag" => Emitter::Flag {
            name: require_string(object, "name")?,
            field: require_string(object, "field")?,
        },
        "option" => Emitter::Option {
            name: require_string(object, "name")?,
            field: require_string(object, "field")?,
        },
        "boolean_option" => {
            let name = require_string(object, "name")?;
            let field = require_string(object, "field")?;
            let true_value = require_string(object, "true_value")?;
            let false_value = optional_string(object, "false_value");
            Emitter::BooleanOption {
                name,
                field,
                true_value,
                false_value,
            }
        }
        "repeated_option" => Emitter::RepeatedOption {
            name: require_string(object, "name")?,
            field: require_string(object, "field")?,
        },
        "positional" => Emitter::Positional {
            field: require_string(object, "field")?,
        },
        "environment" => Emitter::Environment {
            name: require_string(object, "name")?,
            field: require_string(object, "field")?,
        },
        other => return Err(unknown(format!("unknown emitter kind {other:?}"))),
    };
    emitter
        .validate()
        .map_err(|err| unknown(format!("emitter invalid: {err}")))?;
    Ok(emitter)
}

fn string_array(
    object: &[(String, BoundedJson)],
    key: &str,
) -> Result<Vec<String>, DefinitionError> {
    let Some(raw) = object.iter().find(|(k, _)| k == key) else {
        return Ok(Vec::new());
    };
    if raw.1.is_null() {
        return Ok(Vec::new());
    }
    let arr = raw
        .1
        .as_array()
        .ok_or_else(|| unknown(format!("{key} must be an array")))?;
    let mut out = Vec::with_capacity(arr.len());
    for element in arr {
        let BoundedJson::Str(s) = element else {
            return Err(unknown(format!("{key} must contain only strings")));
        };
        if s.len() > STRING_VALUE_BYTE_LIMIT {
            return Err(unknown(format!("{key} element exceeds string limit")));
        }
        out.push(s.clone());
    }
    Ok(out)
}

fn enum_str(
    object: &[(String, BoundedJson)],
    key: &str,
    allowed: &[&str],
) -> Result<Option<String>, DefinitionError> {
    let Some(raw) = object.iter().find(|(k, _)| k == key) else {
        return Ok(None);
    };
    if raw.1.is_null() {
        return Ok(None);
    }
    let BoundedJson::Str(s) = &raw.1 else {
        return Err(unknown(format!("{key} must be a string")));
    };
    if !allowed.contains(&s.as_str()) {
        return Err(unknown(format!("unknown {key} value {s:?}")));
    }
    Ok(Some(s.clone()))
}

fn read_probe_stream(object: &[(String, BoundedJson)]) -> Result<ProbeStream, DefinitionError> {
    let raw = enum_str(object, "stream", &["stdout", "stderr", "combined"])?;
    match raw.as_deref() {
        Some("stderr") => Ok(ProbeStream::Stderr),
        Some("combined") => Ok(ProbeStream::Combined),
        Some("stdout") | None => Ok(ProbeStream::Stdout),
        // enum_str restricts to the allowed set; any other value is a bug.
        Some(_) => Err(unknown("invalid probe stream")),
    }
}

fn read_probe_framing(object: &[(String, BoundedJson)]) -> Result<ProbeFraming, DefinitionError> {
    let raw = enum_str(
        object,
        "framing",
        &["single_json", "json_lines", "utf8_text"],
    )?;
    match raw.as_deref() {
        Some("single_json") => Ok(ProbeFraming::SingleJson),
        Some("json_lines") => Ok(ProbeFraming::JsonLines),
        Some("utf8_text") | None => Ok(ProbeFraming::Utf8Text),
        // enum_str restricts to the allowed set; any other value is a bug.
        Some(_) => Err(unknown("invalid probe framing")),
    }
}

fn bool_field(
    object: &[(String, BoundedJson)],
    key: &str,
) -> Result<Option<bool>, DefinitionError> {
    let Some(raw) = object.iter().find(|(k, _)| k == key) else {
        return Ok(None);
    };
    if raw.1.is_null() {
        return Ok(None);
    }
    match &raw.1 {
        BoundedJson::Bool(b) => Ok(Some(*b)),
        _ => Err(unknown(format!("{key} must be a boolean"))),
    }
}

fn i64_field(object: &[(String, BoundedJson)], key: &str) -> Result<Option<i64>, DefinitionError> {
    let Some(raw) = object.iter().find(|(k, _)| k == key) else {
        return Ok(None);
    };
    if raw.1.is_null() {
        return Ok(None);
    }
    match &raw.1 {
        BoundedJson::Int(i) => Ok(Some(*i)),
        _ => Err(unknown(format!("{key} must be an integer"))),
    }
}

fn u64_field(object: &[(String, BoundedJson)], key: &str) -> Result<Option<u64>, DefinitionError> {
    let Some(i) = i64_field(object, key)? else {
        return Ok(None);
    };
    u64::try_from(i)
        .map(Some)
        .map_err(|_| unknown(format!("{key} must be non-negative")))
}

fn usize_field(
    object: &[(String, BoundedJson)],
    key: &str,
) -> Result<Option<usize>, DefinitionError> {
    let Some(i) = i64_field(object, key)? else {
        return Ok(None);
    };
    usize::try_from(i)
        .map(Some)
        .map_err(|_| unknown(format!("{key} must be non-negative")))
}
