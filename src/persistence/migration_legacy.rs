//! Lossless retention of schema-1 launch values without a schema-2 typed representation.

use serde_json::{Map, Value};

use super::schema1::Schema1Agent;
use crate::domain::{DormantRecord, Id};

pub(super) fn record_legacy_launch_values(
    source: &Schema1Agent,
    raw_source: &Value,
    id: &Id,
    reason: &str,
    dormant: &mut Vec<DormantRecord>,
) {
    let raw_object = raw_source.as_object();
    let mut values = Map::new();
    for field in [
        "code_puppy_quick_resume",
        "mode_flags",
        "llxprt_debug",
        "pass_continue",
        "sandbox_enabled",
        "sandbox_engine",
        "sandbox_flags",
    ] {
        if let Some(value) = raw_object.and_then(|object| object.get(field)) {
            values.insert(field.to_owned(), value.clone());
        }
    }
    let field_absent = |field: &str| raw_object.and_then(|object| object.get(field)).is_none();
    if field_absent("code_puppy_quick_resume") && source.code_puppy_quick_resume {
        values.insert("code_puppy_quick_resume".to_owned(), Value::Bool(true));
    }
    if field_absent("llxprt_debug") && !source.llxprt_debug.is_empty() {
        values.insert(
            "llxprt_debug".to_owned(),
            Value::String(source.llxprt_debug.clone()),
        );
    }
    if field_absent("pass_continue") {
        values.insert(
            "pass_continue".to_owned(),
            Value::Bool(source.pass_continue),
        );
    }
    if field_absent("sandbox_enabled") && source.sandbox_enabled {
        values.insert("sandbox_enabled".to_owned(), Value::Bool(true));
    }
    if field_absent("sandbox_engine") && source.sandbox_enabled {
        values.insert(
            "sandbox_engine".to_owned(),
            Value::String(source.sandbox_engine.clone()),
        );
    }
    if field_absent("sandbox_flags") && !source.sandbox_flags.is_empty() {
        values.insert(
            "sandbox_flags".to_owned(),
            Value::String(source.sandbox_flags.clone()),
        );
    }
    if values.is_empty() {
        return;
    }
    dormant.push(DormantRecord {
        kind: "schema1.agent.legacy-launch-values".to_owned(),
        stable_id: Some(id.clone()),
        raw_schema: 1,
        reason: reason.to_owned(),
        raw_value: Value::Object(values),
    });
}
