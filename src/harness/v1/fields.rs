//! Closed-object field access over parsed JSON (issue #380).
//!
//! [`ObjectReader`] gives the typed parser exact-once field access with
//! unknown-field rejection, so every schema object is closed by construction.

use super::error::HarnessError;
use super::json::JsonValue;

/// Reads fields from one JSON object, tracking which were consumed. Any
/// unconsumed field at [`ObjectReader::finish`] is an unknown-field error.
pub struct ObjectReader<'a> {
    context: String,
    members: &'a [(String, JsonValue)],
    taken: Vec<bool>,
}

impl<'a> ObjectReader<'a> {
    /// Wrap `value`, which must be a JSON object.
    ///
    /// # Errors
    ///
    /// `HAR-E001` when `value` is not an object.
    pub fn new(context: impl Into<String>, value: &'a JsonValue) -> Result<Self, HarnessError> {
        let context = context.into();
        let members = value
            .as_object()
            .ok_or_else(|| HarnessError::syntax(format!("{context}: expected an object")))?;
        Ok(Self {
            taken: vec![false; members.len()],
            context,
            members,
        })
    }

    /// Take an optional field.
    ///
    /// # Errors
    ///
    /// `HAR-E001` when the caller attempts to consume the field twice.
    pub fn opt(&mut self, name: &str) -> Result<Option<&'a JsonValue>, HarnessError> {
        let Some(index) = self.members.iter().position(|(key, _)| key == name) else {
            return Ok(None);
        };
        if self.taken[index] {
            return Err(HarnessError::syntax(format!(
                "{}: field '{name}' was consumed more than once",
                self.context
            )));
        }
        self.taken[index] = true;
        Ok(Some(&self.members[index].1))
    }

    /// Take a required field.
    ///
    /// # Errors
    ///
    /// `HAR-E001` when the field is absent or has already been consumed.
    pub fn require(&mut self, name: &str) -> Result<&'a JsonValue, HarnessError> {
        self.opt(name)?.ok_or_else(|| {
            HarnessError::syntax(format!("{}: missing required field '{name}'", self.context))
        })
    }

    /// Fail if any field was not consumed.
    ///
    /// # Errors
    ///
    /// `HAR-E001` naming the first unknown field.
    pub fn finish(self) -> Result<(), HarnessError> {
        for (index, (key, _)) in self.members.iter().enumerate() {
            if !self.taken[index] {
                return Err(HarnessError::syntax(format!(
                    "{}: unknown field '{key}'",
                    self.context
                )));
            }
        }
        Ok(())
    }

    /// The reader's context label for child errors.
    #[must_use]
    pub fn context(&self) -> &str {
        &self.context
    }
}

/// Extract a string value.
///
/// # Errors
///
/// `HAR-E001` when `value` is not a string.
pub fn as_str<'a>(context: &str, value: &'a JsonValue) -> Result<&'a str, HarnessError> {
    match value {
        JsonValue::Str(text) => Ok(text),
        _ => Err(HarnessError::syntax(format!(
            "{context}: expected a string"
        ))),
    }
}

/// Extract a boolean value.
///
/// # Errors
///
/// `HAR-E001` when `value` is not a boolean.
pub fn as_bool(context: &str, value: &JsonValue) -> Result<bool, HarnessError> {
    match value {
        JsonValue::Bool(flag) => Ok(*flag),
        _ => Err(HarnessError::syntax(format!(
            "{context}: expected a boolean"
        ))),
    }
}

/// Extract an array value.
///
/// # Errors
///
/// `HAR-E001` when `value` is not an array.
pub fn as_array<'a>(context: &str, value: &'a JsonValue) -> Result<&'a [JsonValue], HarnessError> {
    match value {
        JsonValue::Array(items) => Ok(items),
        _ => Err(HarnessError::syntax(format!(
            "{context}: expected an array"
        ))),
    }
}

/// Extract an integer restricted to an inclusive range. Range violations are
/// limit errors (`HAR-E002`) per the "limit plus one" contract.
///
/// # Errors
///
/// `HAR-E001` for non-integers, `HAR-E002` for out-of-range values.
pub fn as_int_in(
    context: &str,
    value: &JsonValue,
    (min, max): (u64, u64),
) -> Result<u64, HarnessError> {
    let JsonValue::Int(raw) = value else {
        return Err(HarnessError::syntax(format!(
            "{context}: expected an integer"
        )));
    };
    let unsigned = u64::try_from(*raw)
        .map_err(|_| HarnessError::limit(format!("{context}: {raw} is below {min}")))?;
    if unsigned < min || unsigned > max {
        return Err(HarnessError::limit(format!(
            "{context}: {unsigned} is outside {min}..={max}"
        )));
    }
    Ok(unsigned)
}

/// Bound an array's length inclusively (`HAR-E002` beyond the bound).
///
/// # Errors
///
/// `HAR-E002` when over the maximum.
pub fn bounded_len(context: &str, len: usize, max: usize) -> Result<(), HarnessError> {
    if len > max {
        return Err(HarnessError::limit(format!(
            "{context}: {len} entries exceed the maximum of {max}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::error::HarCode;
    use super::{JsonValue, ObjectReader};

    #[test]
    fn optional_field_cannot_be_consumed_twice() {
        let value = JsonValue::Object(vec![("field".to_string(), JsonValue::Null)]);
        let mut reader = ObjectReader::new("object", &value)
            .unwrap_or_else(|err| panic!("reader should construct: {err}"));
        assert!(
            reader
                .opt("field")
                .unwrap_or_else(|err| panic!("first read should pass: {err}"))
                .is_some()
        );
        let err = reader
            .opt("field")
            .err()
            .unwrap_or_else(|| panic!("second read must fail"));
        assert_eq!(err.code(), HarCode::E001);
    }
}
