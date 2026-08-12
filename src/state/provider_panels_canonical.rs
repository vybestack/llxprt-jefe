//! Canonical host-local byte-size writer (issue #391).
//!
//! Private child of [`super::provider_panels`]: mirrors the closed canonical
//! typed-value writer in `runtime::provider::encode` so the host-local
//! presentation-state byte size is deterministic, allocation-light, and
//! independent of any `serde_json` value. Keys are sorted (BTreeMap) and typed
//! values use the closed `{"type","value"}` shape so the size is byte-identical
//! to the wire writer.

use std::fmt::Write as _;

use crate::domain::{TypedMap, TypedValue};
use crate::runtime::provider::protocol::HostLocal;

/// The exact canonical-JSON byte length of host-local state.
///
/// Deterministic, allocation-light, and independent of any `serde_json` value:
/// keys are sorted (BTreeMap) and typed values use the closed
/// `{"type","value"}` shape so the size is byte-identical to the wire writer.
#[must_use]
pub(super) fn host_local_canonical_bytes(host: &HostLocal) -> usize {
    let mut out = String::new();
    out.push('{');
    if let Some(focus) = host.focus_target.as_ref() {
        out.push_str("\"focus_target\":");
        append_json_string(&mut out, focus.as_str());
        out.push(',');
    }
    if let Some(draft) = host.form_draft.as_ref() {
        out.push_str("\"form_draft\":");
        append_typed_map(&mut out, draft);
        out.push(',');
    }
    out.push_str("\"scroll_offset\":");
    let _ = write!(out, "{}", host.scroll_offset);
    if let Some(selected) = host.selected_id.as_ref() {
        out.push_str(",\"selected_id\":");
        append_json_string(&mut out, selected.as_str());
    }
    out.push('}');
    out.len()
}

fn append_typed_map(out: &mut String, map: &TypedMap) {
    out.push('{');
    for (index, (key, value)) in map.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        append_json_string(out, key.as_str());
        out.push(':');
        append_typed_value(out, value);
    }
    out.push('}');
}

fn append_typed_value(out: &mut String, value: &TypedValue) {
    out.push_str("{\"type\":");
    match value {
        TypedValue::String(text) => {
            out.push_str("\"string\",\"value\":");
            append_json_string(out, text);
        }
        TypedValue::Bool(flag) => {
            let _ = write!(out, "\"bool\",\"value\":{flag}");
        }
        TypedValue::Integer(number) => {
            let _ = write!(out, "\"integer\",\"value\":{number}");
        }
        TypedValue::Decimal(number) => {
            out.push_str("\"decimal\",\"value\":");
            append_json_string(out, number.as_str());
        }
        TypedValue::Datetime(moment) => {
            out.push_str("\"datetime\",\"value\":");
            append_json_string(out, moment.as_str());
        }
        TypedValue::List(items) => {
            out.push_str("\"list\",\"value\":[");
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                append_typed_value(out, item);
            }
            out.push(']');
        }
        TypedValue::Map(inner) => {
            out.push_str("\"map\",\"value\":");
            append_typed_map(out, inner);
        }
        TypedValue::SecretRef(reference) => {
            out.push_str("\"secret_ref\",\"value\":{\"env\":");
            append_json_string(out, reference.env.env());
            out.push('}');
        }
    }
    out.push('}');
}

fn append_json_string(out: &mut String, value: &str) {
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}
