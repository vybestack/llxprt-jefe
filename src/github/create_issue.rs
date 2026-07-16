use super::GhError;
use crate::domain::{Issue, IssueState};
use serde_json::Value;

/// Payload for creating a new issue.
///
/// @plan PLAN-20260329-ISSUES-MODE.P03
/// @requirement REQ-ISS-011
#[derive(Debug)]
pub struct CreatedIssue {
    pub number: u64,
    pub title: String,
    pub body: String,
    /// Required GraphQL/REST node id from the create response.
    pub node_id: String,
    pub author_login: String,
    pub updated_at: String,
}

impl CreatedIssue {
    /// Build a list-row [`Issue`] from the create response (issue #215).
    #[must_use]
    pub fn into_list_issue(self) -> Issue {
        Issue {
            number: self.number,
            node_id: self.node_id,
            title: self.title,
            state: IssueState::Open,
            author_login: self.author_login,
            updated_at: self.updated_at,
            assignee_summary: String::new(),
            labels_summary: String::new(),
            assignees: Vec::new(),
            labels: Vec::new(),
            issue_type: String::new(),
            milestone: String::new(),
            module: String::new(),
            comment_count: 0,
            body: self.body,
        }
    }
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

    let node_id = value
        .get("node_id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| GhError::ParseError("Missing or empty node_id".to_string()))?
        .to_string();

    let author_login = value
        .get("user")
        .and_then(|user| user.get("login"))
        .and_then(Value::as_str)
        .or_else(|| {
            value
                .get("author")
                .and_then(|author| author.get("login"))
                .and_then(Value::as_str)
        })
        .unwrap_or("")
        .to_string();

    let updated_at = value
        .get("updated_at")
        .and_then(Value::as_str)
        .or_else(|| value.get("created_at").and_then(Value::as_str))
        .unwrap_or("")
        .to_string();

    Ok(CreatedIssue {
        number,
        title,
        body,
        node_id,
        author_login,
        updated_at,
    })
}
