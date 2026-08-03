//! Shared GraphQL error-envelope handling for the `gh` boundary.
//!
//! GitHub's GraphQL API answers a failed query or mutation with HTTP 200 and a
//! top-level `errors` array, so a subprocess exit code of zero does not mean
//! the operation succeeded. Every GraphQL caller in this module tree checks the
//! envelope through these helpers rather than reimplementing the check.

use serde_json::Value;

use super::GhError;

/// Extract non-empty GraphQL error messages from a parsed response, if any.
pub(super) fn error_messages(value: &Value) -> Option<Vec<String>> {
    let errors = value.get("errors")?.as_array()?;
    let messages: Vec<String> = errors
        .iter()
        .filter_map(|e| e.get("message").and_then(Value::as_str).map(String::from))
        .collect();
    if messages.is_empty() {
        None
    } else {
        Some(messages)
    }
}

/// Reject a GraphQL mutation response that carries an `errors` array.
///
/// An empty response body is accepted: some mutations answer with no output on
/// success. `mutation` names the operation so the surfaced error says which
/// call failed.
///
/// # Errors
/// [`GhError::ParseError`] when the body is not JSON, and [`GhError::ApiError`]
/// when GitHub reported one or more errors.
pub(super) fn reject_mutation_errors(stdout: &str, mutation: &str) -> Result<(), GhError> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    let value: Value = serde_json::from_str(trimmed)
        .map_err(|e| GhError::ParseError(format!("invalid JSON in {mutation} response: {e}")))?;
    if let Some(messages) = error_messages(&value) {
        return Err(GhError::ApiError(format!(
            "GraphQL {mutation} mutation failed: {}",
            messages.join("; ")
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{error_messages, reject_mutation_errors};
    use crate::github::GhError;

    #[test]
    fn a_successful_response_is_accepted() {
        let json = r#"{"data":{"deleteRef":{"clientMutationId":null}}}"#;
        assert!(reject_mutation_errors(json, "deleteRef").is_ok());
    }

    #[test]
    fn an_empty_body_is_accepted() {
        assert!(reject_mutation_errors("", "deleteRef").is_ok());
        assert!(reject_mutation_errors("   ", "deleteRef").is_ok());
    }

    #[test]
    fn a_reported_error_names_the_mutation_and_the_message() {
        let json = r#"{"data":null,"errors":[{"message":"Ref not found"}]}"#;
        match reject_mutation_errors(json, "deleteRef") {
            Err(GhError::ApiError(message)) => {
                assert!(message.contains("deleteRef"), "got: {message}");
                assert!(message.contains("Ref not found"), "got: {message}");
            }
            other => panic!("expected ApiError, got {other:?}"),
        }
    }

    #[test]
    fn a_body_that_is_not_json_is_a_parse_error_that_names_the_mutation() {
        match reject_mutation_errors("{ not valid", "deleteRef") {
            Err(GhError::ParseError(message)) => {
                assert!(message.contains("deleteRef"), "got: {message}");
            }
            other => panic!("expected ParseError, got {other:?}"),
        }
    }

    #[test]
    fn an_empty_errors_array_is_not_an_error() {
        let value = serde_json::json!({ "errors": [] });
        assert!(error_messages(&value).is_none());
    }

    #[test]
    fn errors_without_messages_are_not_reported() {
        let value = serde_json::json!({ "errors": [{ "path": ["repository"] }] });
        assert!(error_messages(&value).is_none());
    }
}
