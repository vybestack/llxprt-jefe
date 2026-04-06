use super::GhError;
use serde_json::Value;

/// Payload for creating a new issue.
///
/// @plan PLAN-20260329-ISSUES-MODE.P03
/// @requirement REQ-ISS-011
pub struct CreatedIssue {
    pub number: u64,
    pub title: String,
    pub body: String,
}

/// Parse JSON response from issue creation into a created-issue payload.
///
/// @plan PLAN-20260329-ISSUES-MODE.P08
/// @requirement REQ-ISS-011
pub fn parse_created_issue_json(json_str: &str) -> Result<CreatedIssue, GhError> {
    let value: Value = serde_json::from_str(json_str)
        .map_err(|e| GhError::ParseError(format!("Invalid JSON: {e}")))?;

    let number = value
        .get("number")
        .and_then(Value::as_u64)
        .ok_or_else(|| GhError::ParseError("Missing or invalid issue number".to_string()))?;

    let title = value
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let body = value
        .get("body")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    Ok(CreatedIssue {
        number,
        title,
        body,
    })
}
