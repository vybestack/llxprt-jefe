//! The bundled Issue and pull-request list-detail couplings, now declared
//! rather than hand-wired (issue #385, CW05-09).

use crate::workbench::{
    ActivationMode, EmptyPolicy, PortDirection, PortValue, RelationshipKind, ScreenId,
    master_detail_edge, screen_descriptor,
};

use super::{couples_list_to_detail, detail_target_for, subject};

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

// ── What the declaration tells the reducer ─────────────────────────────────

#[test]
fn a_coupled_screen_hands_its_detail_the_selected_subject() {
    for screen in [ScreenId::Issues, ScreenId::PullRequests] {
        assert_eq!(
            detail_target_for(screen, &subject(Some(42))),
            Some(PortValue::Subject("42".to_owned())),
            "{screen} must hand its detail whatever the list selected"
        );
    }
}

#[test]
fn a_coupled_screen_clears_its_detail_when_nothing_is_selected() {
    for screen in [ScreenId::Issues, ScreenId::PullRequests] {
        assert_eq!(
            detail_target_for(screen, &subject(None)),
            Some(PortValue::Absent),
            "{screen} shows nothing when its list selects nothing"
        );
    }
}

#[test]
fn only_the_screens_that_declare_a_coupling_report_one() {
    assert!(couples_list_to_detail(ScreenId::Issues));
    assert!(couples_list_to_detail(ScreenId::PullRequests));
    assert!(!couples_list_to_detail(ScreenId::Actions));
    assert!(!couples_list_to_detail(ScreenId::Errors));
    assert!(!couples_list_to_detail(ScreenId::Dashboard));
    assert_eq!(
        detail_target_for(ScreenId::Actions, &subject(Some(42))),
        None
    );
}

#[test]
fn an_absent_selection_publishes_absence_rather_than_a_placeholder_subject() {
    assert_eq!(subject(None), PortValue::Absent);
    assert_eq!(subject(Some(42)), PortValue::Subject("42".to_owned()));
}
