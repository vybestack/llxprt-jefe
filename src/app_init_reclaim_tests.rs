//! Behavioural tests for startup reclaim classification.

use super::*;

fn agent(id: &str) -> AgentId {
    AgentId(id.to_owned())
}

fn candidate(id: &str) -> (AgentId, String) {
    let agent_id = agent(id);
    let session = expected_session(&agent_id);
    (agent_id, session)
}

/// A live session whose record startup did not bind is adopted.
///
/// This is the whole point of the pass: the agent is alive, the record exists,
/// and before this nothing would ever reconnect them (issue #585).
#[test]
fn a_live_session_whose_record_lost_its_binding_is_adopted() {
    let candidates = vec![candidate("agent-1"), candidate("agent-2")];
    let observed = vec![expected_session(&agent("agent-2"))];

    assert_eq!(
        classify_reclaimable(&observed, &candidates),
        vec![ReclaimDecision::Adopt(agent("agent-2"))]
    );
}

/// A live session no record claims is reported, never adopted and never killed.
#[test]
fn a_live_session_matching_no_record_is_reported_not_adopted() {
    let candidates = vec![candidate("agent-1")];
    let observed = vec!["jefe-agent-unknown".to_owned()];

    assert_eq!(
        classify_reclaimable(&observed, &candidates),
        vec![ReclaimDecision::Unowned("jefe-agent-unknown".to_owned())]
    );
}

/// `session_name_for` is lossy, so two ids can collide onto one session name.
///
/// Ownership cannot be established from a collided name, so nothing is adopted.
/// Matching forward is what makes this detectable at all: parsing an id back
/// out of the name would silently pick one of the two.
#[test]
fn an_ambiguous_session_name_adopts_nothing() {
    // Both ids sanitize to `jefe-agent_1`.
    let dotted = candidate("agent.1");
    let scored = candidate("agent_1");
    assert_eq!(
        dotted.1, scored.1,
        "fixture requires a genuine sanitizer collision"
    );
    let session = dotted.1.clone();
    let candidates = vec![dotted, scored];

    assert_eq!(
        classify_reclaimable(std::slice::from_ref(&session), &candidates),
        vec![ReclaimDecision::Ambiguous(session)]
    );
}

/// An agent whose session is genuinely gone is never resurrected: reclaim only
/// ever considers sessions that were actually observed alive.
#[test]
fn a_record_without_a_live_session_is_not_reclaimed() {
    let candidates = vec![candidate("agent-1")];

    assert!(classify_reclaimable(&[], &candidates).is_empty());
}

/// Records that startup already bound are excluded by the caller, so a healthy
/// agent's session is not reconsidered and cannot be double-bound.
#[test]
fn a_session_with_no_eligible_candidate_is_unowned_rather_than_adopted() {
    let observed = vec![expected_session(&agent("agent-live"))];

    assert_eq!(
        classify_reclaimable(&observed, &[]),
        vec![ReclaimDecision::Unowned(expected_session(&agent(
            "agent-live"
        )))]
    );
}

/// A quiet startup says nothing.
#[test]
fn an_uneventful_reclaim_reports_nothing() {
    assert_eq!(reclaim_report(&[], &[], &[]), None);
}

/// Adoption is announced, never silent, and each class is distinguishable.
#[test]
fn the_report_names_what_was_adopted_and_what_was_left_alone() {
    let report = reclaim_report(
        &[agent("agent-1")],
        &["jefe-agent_1".to_owned()],
        &["jefe-agent-stray".to_owned()],
    )
    .unwrap_or_else(|| panic!("a report is expected when something happened"));

    assert!(report.contains("reattached 1"), "{report}");
    assert!(report.contains("agent-1"), "{report}");
    assert!(report.contains("more than one agent"), "{report}");
    assert!(report.contains("jefe-agent-stray"), "{report}");
    assert!(report.contains("left running"), "{report}");
}
