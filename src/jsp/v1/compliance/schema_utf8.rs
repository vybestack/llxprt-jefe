//! Recursive UTF-8 byte-annotation validation for Draft 2020-12 schemas.

use serde_json::Value;

pub(super) fn custom_utf8_valid(root: &Value, schema: &Value, instance: &Value) -> bool {
    let reference_valid = schema
        .get("$ref")
        .and_then(Value::as_str)
        .map_or(true, |reference| {
            reference
                .strip_prefix('#')
                .and_then(|pointer| root.pointer(pointer))
                .is_some_and(|resolved| custom_utf8_valid(root, resolved, instance))
        });
    if !reference_valid {
        return false;
    }
    if let (Some(maximum), Some(value)) = (
        schema.get("x-jsp-maxUtf8Bytes").and_then(Value::as_u64),
        instance.as_str(),
    ) && u64::try_from(value.len()).map_or(true, |length| length > maximum)
    {
        return false;
    }
    let properties_valid = match (
        schema.get("properties").and_then(Value::as_object),
        instance.as_object(),
    ) {
        (Some(properties), Some(object)) => object.iter().all(|(key, value)| {
            properties
                .get(key)
                .map_or(true, |child| custom_utf8_valid(root, child, value))
        }),
        _ => true,
    };
    let items_valid = match (schema.get("items"), instance.as_array()) {
        (Some(items), Some(values)) => values
            .iter()
            .all(|value| custom_utf8_valid(root, items, value)),
        _ => true,
    };
    let branches_valid = schema
        .get("oneOf")
        .and_then(Value::as_array)
        .map_or(true, |branches| {
            branches
                .iter()
                .filter(|branch| branch_matches_discriminator(branch, instance))
                .all(|branch| custom_utf8_valid(root, branch, instance))
        });
    properties_valid && items_valid && branches_valid
}

fn branch_matches_discriminator(branch: &Value, instance: &Value) -> bool {
    let Some(properties) = branch.get("properties").and_then(Value::as_object) else {
        return true;
    };
    properties.iter().all(|(key, property)| {
        property.get("const").map_or(true, |expected| {
            instance.get(key).is_some_and(|actual| actual == expected)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ref_siblings_still_enforce_utf8_byte_limits() {
        let schema = serde_json::json!({
            "$defs": {"text": {"type": "string"}},
            "$ref": "#/$defs/text",
            "x-jsp-maxUtf8Bytes": 4
        });
        assert!(custom_utf8_valid(
            &schema,
            &schema,
            &serde_json::json!("éé")
        ));
        assert!(!custom_utf8_valid(
            &schema,
            &schema,
            &serde_json::json!("ééé")
        ));
    }
}
