//! Issue sort comparators and priority extraction (issue #473).
//!
//! Sort is a projection-time view transform: these functions are called after
//! fetch to re-order an already-loaded `PaginatedList`. The fetch-time
//! [`sort_issues`] remains as a defensive default ordering, but the active sort
//! re-projects after every load/append.
//!
//! Extracted from `parse.rs` to keep that file within the source-size policy.

use crate::domain::{Issue, IssueSortConfig, SortOrder};
use serde_json::Value;

use super::timestamp::cmp_rfc3339_newest_first;

/// Sort issues by updated_at desc, then number asc (defensive fetch-time order).
///
/// @plan PLAN-20260329-ISSUES-MODE.P08
/// @requirement REQ-ISS-006
/// @pseudocode component-002 lines 46-54
pub fn sort_issues(issues: &mut [Issue]) {
    issues.sort_by(|a, b| {
        cmp_rfc3339_newest_first(&a.updated_at, &b.updated_at).then(a.number.cmp(&b.number))
    });
}

/// Map a priority label to a numeric rank where a higher rank means a higher
/// priority (so a descending sort puts the most urgent issues first).
///
/// GitHub's built-in priorities (Critical/High/Medium/Low) get fixed ranks.
/// Any other label (custom org fields) is ranked case-insensitively below
/// "Low" so recognized priorities always surface above unknown ones. Missing
/// priority sorts last regardless of direction — `None` never beats `Some`.
#[must_use]
pub fn issue_priority_rank(priority: &str) -> i64 {
    match priority.trim().to_ascii_lowercase().as_str() {
        "critical" => 4,
        "high" => 3,
        "medium" | "med" => 2,
        "low" => 1,
        _ => 0,
    }
}

/// Compare two issues for the user-selected sort (issue #473).
///
/// Returns a `std::cmp::Ordering` driven by the active [`IssueSortConfig`].
/// Ties break to ascending `number` so the order is deterministic and
/// stable across re-projections (except when sorting by `Number`, which has
/// no further tie-break). Missing `created_at`/`updated_at` timestamps
/// sort as empty strings (RFC 3339 naturally sorts the empty string earliest).
/// Missing priority sorts last regardless of direction (descending still puts
/// `Some` above `None`), so un-prioritized issues never jump ahead.
#[must_use]
pub fn compare_issues(a: &Issue, b: &Issue, config: IssueSortConfig) -> std::cmp::Ordering {
    use crate::domain::IssueSortBy;
    use std::cmp::Ordering;
    let primary = match config.by {
        IssueSortBy::Number => match config.order {
            SortOrder::Asc => a.number.cmp(&b.number),
            SortOrder::Desc => b.number.cmp(&a.number),
        },
        IssueSortBy::Created => cmp_timestamp(&a.created_at, &b.created_at, config.order),
        IssueSortBy::Updated => cmp_timestamp(&a.updated_at, &b.updated_at, config.order),
        IssueSortBy::Priority => {
            cmp_priority(a.priority.as_ref(), b.priority.as_ref(), config.order)
        }
    };
    // Tie-break: ascending number for deterministic order regardless of sort key.
    primary.then_with(|| {
        if config.by == IssueSortBy::Number {
            Ordering::Equal
        } else {
            a.number.cmp(&b.number)
        }
    })
}

/// Compare two RFC 3339 timestamps honoring the sort direction.
fn cmp_timestamp(a: &str, b: &str, order: SortOrder) -> std::cmp::Ordering {
    let newest_first = matches!(order, SortOrder::Desc);
    // `cmp_rfc3339_newest_first` is newest-first; flip for ascending.
    let base = cmp_rfc3339_newest_first(a, b);
    if newest_first { base } else { base.reverse() }
}

/// Compare two optional priority labels honoring the sort direction.
///
/// `None` always sorts after `Some` regardless of direction, so un-prioritized
/// issues never jump ahead of prioritized ones.
fn cmp_priority(a: Option<&String>, b: Option<&String>, order: SortOrder) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (a, b) {
        (None, None) => Ordering::Equal,
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (Some(pa), Some(pb)) => {
            let rank_a = issue_priority_rank(pa);
            let rank_b = issue_priority_rank(pb);
            // Higher rank = higher priority. Descending puts highest first.
            match order {
                SortOrder::Desc => rank_b.cmp(&rank_a).then_with(|| {
                    // Stable secondary: alphabetical within equal rank.
                    pa.to_ascii_lowercase().cmp(&pb.to_ascii_lowercase())
                }),
                SortOrder::Asc => rank_a
                    .cmp(&rank_b)
                    .then_with(|| pa.to_ascii_lowercase().cmp(&pb.to_ascii_lowercase())),
            }
        }
    }
}

/// Extract the issue priority from the GraphQL `issueFieldValues` connection.
///
/// Requires the `GraphQL-Features: issue_fields` header at fetch time. It
/// arrives as an `IssueFieldSingleSelectValue` node whose parent `field.name`
/// is `"Priority"`; the selected option's display text is in the node's
/// `.name`. Returns `None` when the issue has no priority set or the field was
/// not fetched.
pub(super) fn issue_priority_from_item(item: &Value) -> Option<String> {
    let nodes = item
        .get("issueFieldValues")
        .and_then(|f| f.get("nodes"))
        .and_then(Value::as_array)?;
    nodes.iter().find_map(|node| {
        // Only single-select values carry a priority; verify the parent field name.
        let is_single_select = node
            .get("__typename")
            .and_then(Value::as_str)
            .is_some_and(|t| t == "IssueFieldSingleSelectValue");
        if !is_single_select {
            return None;
        }
        let field_name = node
            .get("field")
            .and_then(|f| f.get("name"))
            .and_then(Value::as_str)?;
        if field_name.eq_ignore_ascii_case("Priority") {
            node.get("name").and_then(Value::as_str).map(str::to_string)
        } else {
            None
        }
    })
}
