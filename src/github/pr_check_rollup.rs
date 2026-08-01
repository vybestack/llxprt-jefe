//! Check-rollup projection for pull requests.
//!
//! Extracted from `parse_pr.rs` to keep that file within the source-size
//! policy. Owns the three concerns that turn a raw `statusCheckRollup` into the
//! aggregate `PrCheckStatus`:
//!
//! 1. per-token status mapping (`parse_check_status`),
//! 2. supersession resolution (`effective_check_nodes`), and
//! 3. precedence aggregation (`parse_checks_rollup`).
//!
//! Boundary isolation: imports only `crate::domain`, `serde_json::Value`, and a
//! sibling timestamp comparator — mirroring `parse_pr.rs`.

use std::cmp::Ordering;
use std::collections::HashMap;

use crate::domain::PrCheckStatus;
use serde_json::Value;

use super::timestamp::cmp_rfc3339_newest_first;

/// Map a raw status/conclusion/state token to a [`PrCheckStatus`] (union of
/// CheckRun-conclusion, CheckRun-status, and StatusContext-state tokens).
///
/// A lone/latest `CANCELLED` stays `Failure` by design (issue #514 removes
/// *superseded* attempts before aggregation; it does not weaken this mapping).
#[must_use]
pub fn parse_check_status(raw_status: &str) -> PrCheckStatus {
    match raw_status {
        "SUCCESS" => PrCheckStatus::Success,
        "FAILURE" | "ERROR" | "TIMED_OUT" | "STARTUP_FAILURE" | "ACTION_REQUIRED" | "CANCELLED" => {
            PrCheckStatus::Failure
        }
        "PENDING" | "EXPECTED" | "QUEUED" | "IN_PROGRESS" | "WAITING" | "REQUESTED"
        | "COMPLETED" | "" => PrCheckStatus::Pending,
        _ => PrCheckStatus::Neutral,
    }
}

/// Resolve superseded attempts so only the latest effective run per check
/// identity remains.
///
/// GitHub's `statusCheckRollup` can return several `CheckRun` nodes for the
/// same logical check when a workflow was re-run: an older `CANCELLED`/failed
/// attempt alongside a newer successful one. GitHub's own UI and `gh pr checks`
/// project only the latest attempt; this function reproduces that projection.
///
/// Identity groups by `__typename` + `name`/`context` + a workflow disambiguator
/// (`workflowName` from `gh pr view --json`, or
/// `checkSuite.workflowRun.workflow.name` from raw GraphQL) + `checkSuite.app.slug`,
/// so unrelated apps or workflows that happen to share a job name are never
/// merged. Within an identity the attempt with the greatest `startedAt`
/// (fallback `completedAt`, then array position) wins. Output preserves
/// first-occurrence order of each identity.
///
/// `StatusContext` entries are treated as independent identities (one per
/// `context`), matching GitHub's status API semantics.
#[must_use]
pub fn effective_check_nodes(nodes: &[Value]) -> Vec<&Value> {
    // `winners` maps identity → index of the latest winning node; `order`
    // records identities in first-occurrence order so the output stays stable.
    // The identity is a borrowed tuple (Copy), so it keys the map without
    // allocation and cannot collide the way string concatenation can.
    let mut winners: HashMap<CheckIdentity<'_>, usize> = HashMap::new();
    let mut order: Vec<CheckIdentity<'_>> = Vec::new();
    for (idx, node) in nodes.iter().enumerate() {
        let identity = effective_check_identity(node);
        if let Some(&incumbent) = winners.get(&identity) {
            if is_later_attempt(
                order_key(node),
                idx,
                order_key(&nodes[incumbent]),
                incumbent,
            ) {
                winners.insert(identity, idx);
            }
        } else {
            winners.insert(identity, idx);
            order.push(identity);
        }
    }
    order
        .iter()
        .filter_map(|identity| winners.get(identity).copied())
        .map(|idx| &nodes[idx])
        .collect()
}

/// Aggregate the effective check statuses into a single rollup status.
///
/// Supersession is resolved first via [`effective_check_nodes`], then
/// precedence is applied: empty→`None`; any Failure→Failure; any Pending→
/// Pending; all Success→Success; else Neutral.
#[must_use]
pub fn parse_checks_rollup(nodes: &[Value]) -> PrCheckStatus {
    let effective = effective_check_nodes(nodes);
    if effective.is_empty() {
        return PrCheckStatus::None;
    }
    let mut has_failure = false;
    let mut has_pending = false;
    let mut all_success = true;
    for node in effective {
        let status = node
            .get("conclusion")
            .or_else(|| node.get("state"))
            .or_else(|| node.get("status"))
            .and_then(Value::as_str)
            .map_or(PrCheckStatus::Pending, parse_check_status);
        match status {
            PrCheckStatus::Failure => has_failure = true,
            PrCheckStatus::Pending => has_pending = true,
            PrCheckStatus::Success => {}
            _ => all_success = false,
        }
    }
    if has_failure {
        PrCheckStatus::Failure
    } else if has_pending {
        PrCheckStatus::Pending
    } else if all_success {
        PrCheckStatus::Success
    } else {
        PrCheckStatus::Neutral
    }
}

/// Collision-free identity for one effective check: typename, name/context,
/// workflow name, app slug. A borrowed tuple is `Copy` (no allocation) and its
/// components are compared independently, so a field value can never forge or
/// collide with another identity the way string concatenation could.
type CheckIdentity<'a> = (&'a str, &'a str, &'a str, &'a str);

/// Stable identity key for one effective check. See [`effective_check_nodes`].
fn effective_check_identity(node: &Value) -> CheckIdentity<'_> {
    let typename = node.get("__typename").and_then(Value::as_str).unwrap_or("");
    let name = node
        .get("name")
        .or_else(|| node.get("context"))
        .and_then(Value::as_str)
        .unwrap_or("");
    // GitHub Actions exposes the same app slug ("github-actions") for every
    // workflow, so the app slug alone cannot tell two workflows apart. The
    // workflow name is what distinguishes same-named jobs across workflows.
    // `gh pr view --json` surfaces it as a top-level `workflowName`; raw
    // GraphQL surfaces it nested under `checkSuite.workflowRun.workflow.name`.
    let workflow = node
        .get("workflowName")
        .and_then(Value::as_str)
        .or_else(|| {
            node.get("checkSuite")
                .and_then(|suite| suite.get("workflowRun"))
                .and_then(|run| run.get("workflow"))
                .and_then(|wf| wf.get("name"))
                .and_then(Value::as_str)
        })
        .unwrap_or("");
    // The app slug is also included so a third-party app sharing a job name
    // with GitHub Actions (or another app) is never merged.
    let app_slug = node
        .get("checkSuite")
        .and_then(|suite| suite.get("app"))
        .and_then(|app| app.get("slug"))
        .and_then(Value::as_str)
        .unwrap_or("");
    (typename, name, workflow, app_slug)
}

/// Latest-start ordering key: `startedAt`, falling back to `completedAt`.
fn order_key(node: &Value) -> &str {
    node.get("startedAt")
        .and_then(Value::as_str)
        .or_else(|| node.get("completedAt").and_then(Value::as_str))
        .unwrap_or("")
}

/// True when the challenger attempt is newer than (or, on a timestamp tie,
/// later in the rollup than) the incumbent — i.e. it should win the identity.
fn is_later_attempt(
    challenger_key: &str,
    challenger_idx: usize,
    incumbent_key: &str,
    incumbent_idx: usize,
) -> bool {
    // `cmp_rfc3339_newest_first` orders newest-first, so the newer timestamp
    // compares as `Less`. Valid timestamps precede malformed/empty values, so a
    // node carrying a timestamp correctly beats one without.
    match cmp_rfc3339_newest_first(challenger_key, incumbent_key) {
        Ordering::Less => true,
        Ordering::Greater => false,
        Ordering::Equal => challenger_idx > incumbent_idx,
    }
}
