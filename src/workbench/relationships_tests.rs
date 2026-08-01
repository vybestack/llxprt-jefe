//! Exhaustive relationship-graph invalid matrix (issue #385, CW05-08).

use super::descriptor::PortDirection;
use super::ids::MAX_RELATIONSHIPS_PER_SCREEN;
use super::relationship_fixtures::{
    SUBJECT_TYPE, list_detail, panel, panel_id, port, port_ref, screen, type_id,
};
use super::relationships::{
    ActivationMode, EmptyPolicy, Relationship, RelationshipError, RelationshipKind,
    SessionEmptyPolicy, validate_relationships,
};

const MASTER_DETAIL: RelationshipKind = RelationshipKind::MasterDetail {
    activation: ActivationMode::Immediate,
    empty: EmptyPolicy::Retain,
};

// ── Acceptance ─────────────────────────────────────────────────────────────

#[test]
fn a_well_formed_output_to_input_edge_is_accepted() {
    let descriptor = list_detail(MASTER_DETAIL, false);

    assert_eq!(validate_relationships(&descriptor), Ok(()));
}

#[test]
fn every_relationship_kind_is_accepted_on_a_well_formed_edge() {
    for kind in [
        RelationshipKind::Scope,
        MASTER_DETAIL,
        RelationshipKind::MasterDetail {
            activation: ActivationMode::Explicit,
            empty: EmptyPolicy::ShowNone,
        },
        RelationshipKind::SessionTarget {
            empty: SessionEmptyPolicy::Detach,
        },
    ] {
        assert_eq!(
            validate_relationships(&list_detail(kind, false)),
            Ok(()),
            "{} must be accepted",
            kind.as_str()
        );
    }
}

#[test]
fn a_screen_with_no_relationships_is_accepted() {
    let descriptor = screen(vec![panel("only", true, Vec::new())], Vec::new());

    assert_eq!(validate_relationships(&descriptor), Ok(()));
}

// ── Scope ──────────────────────────────────────────────────────────────────

#[test]
fn an_edge_naming_a_panel_this_screen_does_not_declare_is_rejected() {
    let mut descriptor = list_detail(MASTER_DETAIL, false);
    descriptor.relationships[0].source = port_ref("elsewhere", "selection");

    assert_eq!(
        validate_relationships(&descriptor),
        Err(RelationshipError::OutOfScope {
            reference: port_ref("elsewhere", "selection")
        })
    );
}

#[test]
fn an_edge_naming_a_port_the_panel_does_not_declare_is_rejected() {
    let mut descriptor = list_detail(MASTER_DETAIL, false);
    descriptor.relationships[0].target = port_ref("detail", "absent");

    assert_eq!(
        validate_relationships(&descriptor),
        Err(RelationshipError::OutOfScope {
            reference: port_ref("detail", "absent")
        })
    );
}

// ── Direction ──────────────────────────────────────────────────────────────

#[test]
fn an_input_may_not_be_a_source() {
    let descriptor = screen(
        vec![
            panel(
                "list",
                true,
                vec![port("in-a", PortDirection::Input, SUBJECT_TYPE, false)],
            ),
            panel(
                "detail",
                false,
                vec![port("in-b", PortDirection::Input, SUBJECT_TYPE, false)],
            ),
        ],
        vec![Relationship {
            kind: MASTER_DETAIL,
            source: port_ref("list", "in-a"),
            target: port_ref("detail", "in-b"),
        }],
    );

    assert_eq!(
        validate_relationships(&descriptor),
        Err(RelationshipError::WrongDirection {
            reference: port_ref("list", "in-a"),
            expected: PortDirection::Output
        })
    );
}

#[test]
fn an_output_may_not_be_a_target() {
    let descriptor = screen(
        vec![
            panel(
                "list",
                true,
                vec![port("out-a", PortDirection::Output, SUBJECT_TYPE, false)],
            ),
            panel(
                "detail",
                false,
                vec![port("out-b", PortDirection::Output, SUBJECT_TYPE, false)],
            ),
        ],
        vec![Relationship {
            kind: MASTER_DETAIL,
            source: port_ref("list", "out-a"),
            target: port_ref("detail", "out-b"),
        }],
    );

    assert_eq!(
        validate_relationships(&descriptor),
        Err(RelationshipError::WrongDirection {
            reference: port_ref("detail", "out-b"),
            expected: PortDirection::Input
        })
    );
}

// ── Type ───────────────────────────────────────────────────────────────────

#[test]
fn endpoints_carrying_different_type_names_are_rejected() {
    let mut descriptor = list_detail(MASTER_DETAIL, false);
    descriptor.panels[1].ports[0].type_id = type_id("github.issue@1");

    assert_eq!(
        validate_relationships(&descriptor),
        Err(RelationshipError::TypeMismatch {
            source: type_id(SUBJECT_TYPE),
            target: type_id("github.issue@1")
        })
    );
}

#[test]
fn endpoints_carrying_different_versions_of_one_type_are_rejected() {
    let mut descriptor = list_detail(MASTER_DETAIL, false);
    descriptor.panels[1].ports[0].type_id = type_id("github.pull-request@2");

    assert_eq!(
        validate_relationships(&descriptor),
        Err(RelationshipError::TypeMismatch {
            source: type_id(SUBJECT_TYPE),
            target: type_id("github.pull-request@2")
        })
    );
}

// ── Cycles ─────────────────────────────────────────────────────────────────

/// A panel with one output and one input, so it can sit on a cycle.
fn relay(id: &str, required: bool) -> super::descriptor::PanelDescriptor {
    panel(
        id,
        required,
        vec![
            port("out", PortDirection::Output, SUBJECT_TYPE, false),
            port("in", PortDirection::Input, SUBJECT_TYPE, false),
        ],
    )
}

fn edge(from: &str, to: &str) -> Relationship {
    Relationship {
        kind: MASTER_DETAIL,
        source: port_ref(from, "out"),
        target: port_ref(to, "in"),
    }
}

#[test]
fn a_self_edge_is_rejected_as_a_cycle() {
    let descriptor = screen(vec![relay("a", true)], vec![edge("a", "a")]);

    assert_eq!(
        validate_relationships(&descriptor),
        Err(RelationshipError::Cycle {
            panel: panel_id("a")
        })
    );
}

#[test]
fn a_two_panel_cycle_is_rejected() {
    let descriptor = screen(
        vec![relay("a", true), relay("b", false)],
        vec![edge("a", "b"), edge("b", "a")],
    );

    assert!(matches!(
        validate_relationships(&descriptor),
        Err(RelationshipError::Cycle { .. })
    ));
}

#[test]
fn a_three_panel_cycle_is_rejected() {
    let descriptor = screen(
        vec![relay("a", true), relay("b", false), relay("c", false)],
        vec![edge("a", "b"), edge("b", "c"), edge("c", "a")],
    );

    assert!(matches!(
        validate_relationships(&descriptor),
        Err(RelationshipError::Cycle { .. })
    ));
}

#[test]
fn an_acyclic_chain_is_accepted() {
    let descriptor = screen(
        vec![relay("a", true), relay("b", false), relay("c", false)],
        vec![edge("a", "b"), edge("b", "c")],
    );

    assert_eq!(validate_relationships(&descriptor), Ok(()));
}

// ── Uniqueness ─────────────────────────────────────────────────────────────

#[test]
fn one_target_may_not_be_driven_by_two_edges() {
    let descriptor = screen(
        vec![relay("a", true), relay("b", false), relay("c", false)],
        vec![edge("a", "c"), edge("b", "c")],
    );

    assert_eq!(
        validate_relationships(&descriptor),
        Err(RelationshipError::DuplicateIncoming {
            target: port_ref("c", "in")
        })
    );
}

#[test]
fn one_source_port_may_not_declare_two_edges_of_one_kind() {
    let descriptor = screen(
        vec![
            relay("a", true),
            relay("b", false),
            panel(
                "c",
                false,
                vec![port("in", PortDirection::Input, SUBJECT_TYPE, false)],
            ),
        ],
        vec![edge("a", "b"), edge("a", "c")],
    );

    assert_eq!(
        validate_relationships(&descriptor),
        Err(RelationshipError::DuplicateOutgoing {
            source: port_ref("a", "out"),
            kind: "master-detail"
        })
    );
}

#[test]
fn one_panel_may_not_fan_out_two_targets_of_one_kind_from_different_ports() {
    let descriptor = screen(
        vec![
            panel(
                "a",
                true,
                vec![
                    port("out-1", PortDirection::Output, SUBJECT_TYPE, false),
                    port("out-2", PortDirection::Output, SUBJECT_TYPE, false),
                ],
            ),
            panel(
                "b",
                false,
                vec![port("in", PortDirection::Input, SUBJECT_TYPE, false)],
            ),
            panel(
                "c",
                false,
                vec![port("in", PortDirection::Input, SUBJECT_TYPE, false)],
            ),
        ],
        vec![
            Relationship {
                kind: MASTER_DETAIL,
                source: port_ref("a", "out-1"),
                target: port_ref("b", "in"),
            },
            Relationship {
                kind: MASTER_DETAIL,
                source: port_ref("a", "out-2"),
                target: port_ref("c", "in"),
            },
        ],
    );

    assert_eq!(
        validate_relationships(&descriptor),
        Err(RelationshipError::SameKindFanOut {
            panel: panel_id("a"),
            kind: "master-detail"
        })
    );
}

#[test]
fn one_panel_may_drive_two_targets_with_different_kinds() {
    let descriptor = screen(
        vec![
            panel(
                "a",
                true,
                vec![
                    port("out-1", PortDirection::Output, SUBJECT_TYPE, false),
                    port("out-2", PortDirection::Output, SUBJECT_TYPE, false),
                ],
            ),
            panel(
                "b",
                false,
                vec![port("in", PortDirection::Input, SUBJECT_TYPE, false)],
            ),
            panel(
                "c",
                false,
                vec![port("in", PortDirection::Input, SUBJECT_TYPE, false)],
            ),
        ],
        vec![
            Relationship {
                kind: MASTER_DETAIL,
                source: port_ref("a", "out-1"),
                target: port_ref("b", "in"),
            },
            Relationship {
                kind: RelationshipKind::Scope,
                source: port_ref("a", "out-2"),
                target: port_ref("c", "in"),
            },
        ],
    );

    assert_eq!(validate_relationships(&descriptor), Ok(()));
}

// ── Count bound ────────────────────────────────────────────────────────────

/// A star of `count` edges from distinct sources to distinct targets.
fn star(count: usize) -> super::descriptor::ScreenDescriptor {
    let mut panels = vec![panel(
        "hub",
        true,
        (0..count)
            .map(|index| {
                port(
                    &format!("out{index}"),
                    PortDirection::Output,
                    SUBJECT_TYPE,
                    false,
                )
            })
            .collect(),
    )];
    panels.push(panel(
        "sink",
        false,
        (0..count)
            .map(|index| {
                port(
                    &format!("in{index}"),
                    PortDirection::Input,
                    SUBJECT_TYPE,
                    false,
                )
            })
            .collect(),
    ));
    let relationships = (0..count)
        .map(|index| Relationship {
            kind: MASTER_DETAIL,
            source: port_ref("hub", &format!("out{index}")),
            target: port_ref("sink", &format!("in{index}")),
        })
        .collect();
    screen(panels, relationships)
}

#[test]
fn the_relationship_count_bound_is_reported_before_any_other_rule() {
    // The star violates the fan-out rule too, so this also proves the count
    // bound is checked first: a screen that is far over the limit should not
    // have to be walked edge by edge to be rejected.
    let over_limit = star(MAX_RELATIONSHIPS_PER_SCREEN + 1);

    assert_eq!(
        validate_relationships(&over_limit),
        Err(RelationshipError::TooMany {
            count: MAX_RELATIONSHIPS_PER_SCREEN + 1
        })
    );
}
