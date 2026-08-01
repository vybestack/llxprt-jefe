//! The bundled Issue and pull-request list-detail couplings, now declared
//! rather than hand-wired (issue #385, CW05-09).

use crate::workbench::{
    ActivationMode, EmptyPolicy, PortDirection, PortValue, RelationshipKind, ScreenId,
    master_detail_edge, screen_descriptor,
};

use super::{detail_follows_selection, subject};

/// The declared coupling of one workspace screen, as text.
fn declared_edge(screen: ScreenId) -> Option<(String, String)> {
    let descriptor = screen_descriptor(screen)
        .unwrap_or_else(|error| unreachable!("compiled descriptor must exist: {error}"));
    master_detail_edge(descriptor).map(|(source, target)| (source.to_string(), target.to_string()))
}

#[test]
fn the_issues_screen_declares_its_list_to_detail_coupling() {
    assert_eq!(
        declared_edge(ScreenId::Issues),
        Some((
            "issue-list.selection".to_owned(),
            "issue-detail.subject".to_owned()
        ))
    );
}

#[test]
fn the_pull_requests_screen_declares_its_list_to_detail_coupling() {
    assert_eq!(
        declared_edge(ScreenId::PullRequests),
        Some((
            "pr-list.selection".to_owned(),
            "pr-detail.subject".to_owned()
        ))
    );
}

#[test]
fn a_screen_whose_detail_does_not_follow_its_list_declares_no_coupling() {
    assert_eq!(declared_edge(ScreenId::Actions), None);
    assert_eq!(declared_edge(ScreenId::Errors), None);
    assert_eq!(declared_edge(ScreenId::Dashboard), None);
}

#[test]
fn the_bundled_couplings_follow_the_selection_at_once_and_clear_when_empty() {
    for screen in [ScreenId::Issues, ScreenId::PullRequests] {
        let descriptor = screen_descriptor(screen)
            .unwrap_or_else(|error| unreachable!("compiled descriptor must exist: {error}"));

        assert_eq!(
            descriptor.relationships[0].kind,
            RelationshipKind::MasterDetail {
                activation: ActivationMode::Immediate,
                empty: EmptyPolicy::ShowNone,
            },
            "{screen} must follow its selection in the same transition"
        );
    }
}

#[test]
fn the_bundled_ports_face_the_right_way_and_share_one_versioned_type() {
    for (screen, expected_type) in [
        (ScreenId::Issues, "github.issue@1"),
        (ScreenId::PullRequests, "github.pull-request@1"),
    ] {
        let descriptor = screen_descriptor(screen)
            .unwrap_or_else(|error| unreachable!("compiled descriptor must exist: {error}"));
        let (source, target) = master_detail_edge(descriptor)
            .unwrap_or_else(|| unreachable!("{screen} must declare a coupling"));
        let source_port = descriptor
            .port(&source)
            .unwrap_or_else(|| unreachable!("{screen} must declare its source port"));
        let target_port = descriptor
            .port(&target)
            .unwrap_or_else(|| unreachable!("{screen} must declare its target port"));

        assert_eq!(source_port.direction, PortDirection::Output);
        assert_eq!(target_port.direction, PortDirection::Input);
        assert_eq!(source_port.type_id.as_str(), expected_type);
        assert_eq!(target_port.type_id.as_str(), expected_type);
        assert!(
            !target_port.retained,
            "{screen} shows the current selection and nothing when there is none"
        );
    }
}

// ── Parity with the rule the reducer used to hold ──────────────────────────

#[test]
fn the_detail_moves_exactly_when_the_selected_subject_changes() {
    for screen in [ScreenId::Issues, ScreenId::PullRequests] {
        assert!(
            detail_follows_selection(screen, &subject(Some(41)), &subject(Some(42))),
            "{screen} must invalidate when the selection moves"
        );
        assert!(
            !detail_follows_selection(screen, &subject(Some(42)), &subject(Some(42))),
            "{screen} must not invalidate when the selection stays put"
        );
    }
}

#[test]
fn the_detail_clears_when_the_selection_empties_and_fills_when_it_appears() {
    for screen in [ScreenId::Issues, ScreenId::PullRequests] {
        assert!(
            detail_follows_selection(screen, &subject(Some(42)), &subject(None)),
            "{screen} must clear its detail when nothing is selected"
        );
        assert!(
            detail_follows_selection(screen, &subject(None), &subject(Some(42))),
            "{screen} must populate its detail when a selection appears"
        );
        assert!(
            !detail_follows_selection(screen, &subject(None), &subject(None)),
            "{screen} must not invalidate while nothing is selected"
        );
    }
}

#[test]
fn a_screen_with_no_declared_coupling_never_reports_a_detail_move() {
    assert!(!detail_follows_selection(
        ScreenId::Actions,
        &subject(Some(41)),
        &subject(Some(42))
    ));
}

#[test]
fn an_absent_selection_publishes_absence_rather_than_a_placeholder_subject() {
    assert_eq!(subject(None), PortValue::Absent);
    assert_eq!(subject(Some(42)), PortValue::Subject("42".to_owned()));
}
