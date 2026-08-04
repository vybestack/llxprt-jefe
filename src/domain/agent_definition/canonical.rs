//! Canonical sorted-key JSON serialization for the definition SHA-256 digest.
//!
//! The digest must be independent of source literal field order, so this
//! module converts the typed value into a [`BoundedJson`] tree with sorted
//! object keys and then writes canonical bytes.

use super::bounded_json::BoundedJson;
use super::definition::{AgentDefinition, DEFINITION_SCHEMA};
use super::fields::{Emitter, Field, FieldKind, FieldValue};
use super::probe::{AnchoredPattern, IdentityRecognizer, ProbeFraming, ProbeSpec, ProbeStream};
use super::type_id::{CandidateKind, ExecutableCandidate};
use super::types::{
    OperationMatrix, OperationSupport, PromptShape, Support, TargetMatrix, TargetSupport,
};

/// Convert a typed definition into a sorted-key bounded JSON tree.
#[must_use]
pub fn definition_to_json(def: &AgentDefinition) -> BoundedJson {
    let mut top = vec![
        (
            "agent_type_schema".to_string(),
            BoundedJson::Int(i64::from(def.schema)),
        ),
        (
            "id".to_string(),
            BoundedJson::Str(def.id.as_str().to_string()),
        ),
        (
            "display_name".to_string(),
            BoundedJson::Str(def.display_name.clone()),
        ),
        (
            "minimum_version".to_string(),
            BoundedJson::Str(def.minimum_version.clone()),
        ),
        (
            "executable_candidates".to_string(),
            candidates_to_json(&def.candidates),
        ),
        ("probe".to_string(), probe_to_json(&def.probe)),
        (
            "operations".to_string(),
            operations_to_json(&def.operations),
        ),
        ("targets".to_string(), targets_to_json(&def.targets)),
        (
            "repository_fields".to_string(),
            fields_to_json(&def.repository_fields),
        ),
        (
            "agent_fields".to_string(),
            fields_to_json(&def.agent_fields),
        ),
        ("emitters".to_string(), emitters_to_json(&def.emitters)),
    ];
    top.sort_by(|a, b| a.0.cmp(&b.0));
    BoundedJson::Object(top)
}

/// Write canonical bytes (sorted keys, no whitespace) for a bounded JSON tree.
#[must_use]
pub fn canonical_json_bytes(value: &BoundedJson) -> Vec<u8> {
    let mut out = String::new();
    write_canonical(value, &mut out);
    out.into_bytes()
}

fn write_canonical(value: &BoundedJson, out: &mut String) {
    match value {
        BoundedJson::Null => out.push_str("null"),
        BoundedJson::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        BoundedJson::Int(i) => {
            use std::fmt::Write as _;
            let _ = write!(out, "{i}");
        }
        BoundedJson::Str(s) => {
            out.push('"');
            for ch in s.chars() {
                match ch {
                    '"' => out.push_str("\\\""),
                    '\\' => out.push_str("\\\\"),
                    '\n' => out.push_str("\\n"),
                    '\r' => out.push_str("\\r"),
                    '\t' => out.push_str("\\t"),
                    c if (c as u32) < 0x20 => {
                        use std::fmt::Write as _;
                        let _ = write!(out, "\\u{:04x}", c as u32);
                    }
                    c => out.push(c),
                }
            }
            out.push('"');
        }
        BoundedJson::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_canonical(item, out);
            }
            out.push(']');
        }
        BoundedJson::Object(members) => {
            let mut sorted: Vec<&(String, BoundedJson)> = members.iter().collect();
            sorted.sort_by(|a, b| a.0.cmp(&b.0));
            out.push('{');
            for (i, (key, value)) in sorted.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_canonical(&BoundedJson::Str(key.clone()), out);
                out.push(':');
                write_canonical(value, out);
            }
            out.push('}');
        }
    }
}

fn candidates_to_json(candidates: &[ExecutableCandidate]) -> BoundedJson {
    BoundedJson::Array(candidates.iter().map(candidate_to_json).collect())
}

fn candidate_to_json(candidate: &ExecutableCandidate) -> BoundedJson {
    let mut members = match &candidate.kind {
        CandidateKind::PathName { name } => vec![
            (
                "kind".to_string(),
                BoundedJson::Str("path-name".to_string()),
            ),
            ("value".to_string(), BoundedJson::Str(name.clone())),
        ],
        CandidateKind::RepositoryLlxprt => vec![
            (
                "kind".to_string(),
                BoundedJson::Str("repository-llxprt".to_string()),
            ),
            (
                "value".to_string(),
                BoundedJson::Str(candidate.value.to_string_lossy().into_owned()),
            ),
        ],
        CandidateKind::NpmPackage { package, binary } => vec![
            (
                "kind".to_string(),
                BoundedJson::Str("npm-package".to_string()),
            ),
            ("package".to_string(), BoundedJson::Str(package.clone())),
            ("binary".to_string(), BoundedJson::Str(binary.clone())),
        ],
        CandidateKind::UvxPackage { package, binary } => vec![
            (
                "kind".to_string(),
                BoundedJson::Str("uvx-package".to_string()),
            ),
            ("package".to_string(), BoundedJson::Str(package.clone())),
            ("binary".to_string(), BoundedJson::Str(binary.clone())),
        ],
    };
    members.sort_by(|a, b| a.0.cmp(&b.0));
    BoundedJson::Object(members)
}

fn probe_to_json(probe: &ProbeSpec) -> BoundedJson {
    let mut members = vec![
        (
            "argv".to_string(),
            BoundedJson::Array(
                probe
                    .argv
                    .iter()
                    .map(|s| BoundedJson::Str(s.clone()))
                    .collect(),
            ),
        ),
        (
            "stream".to_string(),
            BoundedJson::Str(stream_str(probe.stream).to_string()),
        ),
        (
            "framing".to_string(),
            BoundedJson::Str(framing_str(probe.framing).to_string()),
        ),
        ("identity".to_string(), identity_to_json(&probe.identity)),
        (
            "normalize".to_string(),
            BoundedJson::Str(normalize_str(probe.normalize).to_string()),
        ),
        (
            "timeout_ms".to_string(),
            BoundedJson::Int(i64::try_from(probe.timeout_ms).unwrap_or(i64::MAX)),
        ),
        (
            "max_bytes".to_string(),
            BoundedJson::Int(i64::try_from(probe.max_bytes).unwrap_or(i64::MAX)),
        ),
    ];
    members.sort_by(|a, b| a.0.cmp(&b.0));
    BoundedJson::Object(members)
}

fn stream_str(stream: ProbeStream) -> &'static str {
    match stream {
        ProbeStream::Stdout => "stdout",
        ProbeStream::Stderr => "stderr",
        ProbeStream::Combined => "combined",
    }
}

fn framing_str(framing: ProbeFraming) -> &'static str {
    match framing {
        ProbeFraming::SingleJson => "single_json",
        ProbeFraming::JsonLines => "json_lines",
        ProbeFraming::Utf8Text => "utf8_text",
    }
}

fn identity_to_json(identity: &IdentityRecognizer) -> BoundedJson {
    let mut members = match identity {
        IdentityRecognizer::JsonPointer {
            pointer,
            anchored_pattern,
        } => vec![
            (
                "kind".to_string(),
                BoundedJson::Str("json_pointer".to_string()),
            ),
            ("pointer".to_string(), BoundedJson::Str(pointer.clone())),
            (
                "anchored_pattern".to_string(),
                anchored_to_json(anchored_pattern),
            ),
        ],
        IdentityRecognizer::Line {
            prefix,
            anchored_pattern,
        } => vec![
            ("kind".to_string(), BoundedJson::Str("line".to_string())),
            ("prefix".to_string(), BoundedJson::Str(prefix.clone())),
            (
                "anchored_pattern".to_string(),
                anchored_to_json(anchored_pattern),
            ),
        ],
    };
    members.sort_by(|a, b| a.0.cmp(&b.0));
    BoundedJson::Object(members)
}

fn anchored_to_json(pattern: &AnchoredPattern) -> BoundedJson {
    let mut members = match pattern {
        AnchoredPattern::Exact { value } => vec![
            ("kind".to_string(), BoundedJson::Str("exact".to_string())),
            ("value".to_string(), BoundedJson::Str(value.clone())),
        ],
        AnchoredPattern::Prefix { prefix } => vec![
            ("kind".to_string(), BoundedJson::Str("prefix".to_string())),
            ("prefix".to_string(), BoundedJson::Str(prefix.clone())),
        ],
        AnchoredPattern::Suffix { suffix } => vec![
            ("kind".to_string(), BoundedJson::Str("suffix".to_string())),
            ("suffix".to_string(), BoundedJson::Str(suffix.clone())),
        ],
        AnchoredPattern::PrefixSuffix { prefix, suffix } => vec![
            (
                "kind".to_string(),
                BoundedJson::Str("prefix_suffix".to_string()),
            ),
            ("prefix".to_string(), BoundedJson::Str(prefix.clone())),
            ("suffix".to_string(), BoundedJson::Str(suffix.clone())),
        ],
        AnchoredPattern::VersionToken => vec![(
            "kind".to_string(),
            BoundedJson::Str("version_token".to_string()),
        )],
    };
    members.sort_by(|a, b| a.0.cmp(&b.0));
    BoundedJson::Object(members)
}

fn normalize_str(normalize: super::normalize::Normalize) -> &'static str {
    match normalize {
        super::normalize::Normalize::None => "none",
        super::normalize::Normalize::StripAnsi => "strip_ansi",
    }
}

fn operations_to_json(ops: &OperationMatrix) -> BoundedJson {
    let mut members = vec![
        ("normal".to_string(), operation_support_to_json(&ops.normal)),
        ("resume".to_string(), operation_support_to_json(&ops.resume)),
        (
            "fresh_issue".to_string(),
            operation_support_to_json(&ops.fresh_issue),
        ),
        (
            "fresh_pull_request".to_string(),
            operation_support_to_json(&ops.fresh_pull_request),
        ),
    ];
    members.sort_by(|a, b| a.0.cmp(&b.0));
    BoundedJson::Object(members)
}

fn operation_support_to_json(support: &OperationSupport) -> BoundedJson {
    let mut members = Vec::new();
    let supported = match &support.supported {
        Support::Supported => BoundedJson::Bool(true),
        Support::Unsupported { .. } => BoundedJson::Bool(false),
    };
    members.push(("supported".to_string(), supported));
    if let Support::Unsupported { reason } = &support.supported {
        members.push(("reason".to_string(), BoundedJson::Str(reason.clone())));
    }
    let prompt = match support.prompt {
        PromptShape::None | PromptShape::NoneDefault => "none",
        PromptShape::InitialPositional => "initial_positional",
        PromptShape::InteractiveOption => "interactive_option",
    };
    members.push(("prompt".to_string(), BoundedJson::Str(prompt.to_string())));
    members.sort_by(|a, b| a.0.cmp(&b.0));
    BoundedJson::Object(members)
}

fn targets_to_json(targets: &TargetMatrix) -> BoundedJson {
    let mut members = vec![
        ("local".to_string(), target_support_to_json(&targets.local)),
        (
            "remote".to_string(),
            target_support_to_json(&targets.remote),
        ),
    ];
    members.sort_by(|a, b| a.0.cmp(&b.0));
    BoundedJson::Object(members)
}

fn target_support_to_json(support: &TargetSupport) -> BoundedJson {
    let mut members = Vec::new();
    let supported = match &support.supported {
        Support::Supported => BoundedJson::Bool(true),
        Support::Unsupported { .. } => BoundedJson::Bool(false),
    };
    members.push(("supported".to_string(), supported));
    if let Support::Unsupported { reason } = &support.supported {
        members.push(("reason".to_string(), BoundedJson::Str(reason.clone())));
    }
    members.sort_by(|a, b| a.0.cmp(&b.0));
    BoundedJson::Object(members)
}

fn fields_to_json(fields: &[Field]) -> BoundedJson {
    BoundedJson::Array(fields.iter().map(field_to_json).collect())
}

fn field_to_json(field: &Field) -> BoundedJson {
    let mut members = vec![
        ("id".to_string(), BoundedJson::Str(field.id.clone())),
        (
            "kind".to_string(),
            BoundedJson::Str(kind_str(field.kind).to_string()),
        ),
        ("required".to_string(), BoundedJson::Bool(field.required)),
        (
            "launch_signature".to_string(),
            BoundedJson::Bool(field.launch_signature),
        ),
    ];
    if let Some(default) = &field.default {
        members.push(("default".to_string(), field_value_to_json(default)));
    }
    if let Some(min) = field.minimum {
        members.push(("minimum".to_string(), BoundedJson::Int(min)));
    }
    if let Some(max) = field.maximum {
        members.push(("maximum".to_string(), BoundedJson::Int(max)));
    }
    if !field.choices.is_empty() {
        members.push((
            "choices".to_string(),
            BoundedJson::Array(
                field
                    .choices
                    .iter()
                    .map(|c| BoundedJson::Str(c.clone()))
                    .collect(),
            ),
        ));
    }
    if let Some(visible_when) = &field.visible_when {
        members.push((
            "visible_when".to_string(),
            BoundedJson::Str(visible_when.clone()),
        ));
    }
    members.sort_by(|a, b| a.0.cmp(&b.0));
    BoundedJson::Object(members)
}

fn kind_str(kind: FieldKind) -> &'static str {
    match kind {
        FieldKind::Boolean => "boolean",
        FieldKind::OptionalBoolean => "optional_boolean",
        FieldKind::String => "string",
        FieldKind::Integer => "integer",
        FieldKind::Enum => "enum",
        FieldKind::Path => "path",
        FieldKind::StringList => "string_list",
    }
}

fn field_value_to_json(value: &FieldValue) -> BoundedJson {
    match value {
        FieldValue::Boolean(b) => BoundedJson::Bool(*b),
        FieldValue::OptionalBoolean(opt) => match opt {
            Some(b) => BoundedJson::Bool(*b),
            None => BoundedJson::Null,
        },
        FieldValue::String(s) | FieldValue::Path(s) => BoundedJson::Str(s.clone()),
        FieldValue::Integer(i) => BoundedJson::Int(*i),
        FieldValue::StringList(items) => {
            BoundedJson::Array(items.iter().map(|s| BoundedJson::Str(s.clone())).collect())
        }
    }
}

fn emitters_to_json(emitters: &[Emitter]) -> BoundedJson {
    BoundedJson::Array(emitters.iter().map(emitter_to_json).collect())
}

fn emitter_to_json(emitter: &Emitter) -> BoundedJson {
    let kind = emitter_kind_str(emitter);
    let mut members = vec![("kind".to_string(), BoundedJson::Str(kind.to_string()))];
    match emitter {
        Emitter::Fixed { value } => {
            members.push(("value".to_string(), BoundedJson::Str(value.clone())));
        }
        Emitter::Positional { field } => {
            members.push(("field".to_string(), BoundedJson::Str(field.clone())));
        }
        Emitter::Flag { name, field }
        | Emitter::Option { name, field }
        | Emitter::RepeatedOption { name, field }
        | Emitter::Environment { name, field } => {
            members.push(("name".to_string(), BoundedJson::Str(name.clone())));
            members.push(("field".to_string(), BoundedJson::Str(field.clone())));
        }
        Emitter::BooleanOption {
            name,
            field,
            true_value,
            false_value,
        } => {
            members.push(("name".to_string(), BoundedJson::Str(name.clone())));
            members.push(("field".to_string(), BoundedJson::Str(field.clone())));
            members.push((
                "true_value".to_string(),
                BoundedJson::Str(true_value.clone()),
            ));
            if let Some(false_value) = false_value {
                members.push((
                    "false_value".to_string(),
                    BoundedJson::Str(false_value.clone()),
                ));
            }
        }
    }
    members.sort_by(|a, b| a.0.cmp(&b.0));
    BoundedJson::Object(members)
}

fn emitter_kind_str(emitter: &Emitter) -> &'static str {
    match emitter {
        Emitter::Fixed { .. } => "fixed",
        Emitter::Flag { .. } => "flag",
        Emitter::Option { .. } => "option",
        Emitter::BooleanOption { .. } => "boolean_option",
        Emitter::RepeatedOption { .. } => "repeated_option",
        Emitter::Positional { .. } => "positional",
        Emitter::Environment { .. } => "environment",
    }
}

const _: u16 = DEFINITION_SCHEMA;
