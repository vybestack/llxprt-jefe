//! Relationship propagation: immediate and explicit activation, the closed
//! empty/retained policy table, and the transition bound (issue #385,
//! CW05-05..CW05-08).

use crate::persistence::diagnostic::FOLLOW_UP_LIMIT;

use super::descriptor::{PortDirection, ScreenDescriptor};
use super::relationship_fixtures::{SUBJECT_TYPE, list_detail, panel, port, port_ref, screen};
use super::relationship_propagation::{
    PortUpdate, PortValue, PropagationAbort, RelationshipState, SourceIntent, propagate,
};
use super::relationships::{
    ActivationMode, EmptyPolicy, Relationship, RelationshipKind, SessionEmptyPolicy,
};

const IMMEDIATE: RelationshipKind = RelationshipKind::MasterDetail {
    activation: ActivationMode::Immediate,
    empty: EmptyPolicy::Retain,
};

fn subject(text: &str) -> PortValue {
    PortValue::Subject(text.to_owned())
}

/// Publish `value` from `list.selection` and return the committed state and
/// the ordered updates.
fn published(
    descriptor: &ScreenDescriptor,
    state: &RelationshipState,
    value: PortValue,
) -> (RelationshipState, Vec<PortUpdate>) {
    let transition = propagate(
        descriptor,
        state,
        &SourceIntent::Publish {
            port: port_ref("list", "selection"),
            value,
        },
    )
    .unwrap_or_else(|error| unreachable!("fixture transition must commit: {error}"));
    (transition.state, transition.updates)
}

// ── CW05-05: immediate relationships ───────────────────────────────────────

#[test]
fn an_immediate_relationship_updates_source_and_target_in_one_transition() {
    let descriptor = list_detail(IMMEDIATE, false);

    let (state, updates) = published(&descriptor, &RelationshipState::new(), subject("42"));

    assert_eq!(
        updates,
        vec![
            PortUpdate {
                port: port_ref("list", "selection"),
                value: subject("42"),
            },
            PortUpdate {
                port: port_ref("detail", "subject"),
                value: subject("42"),
            },
        ]
    );
    assert_eq!(state.value(&port_ref("list", "selection")), subject("42"));
    assert_eq!(state.value(&port_ref("detail", "subject")), subject("42"));
}

#[test]
fn a_transition_that_changes_nothing_reports_no_updates() {
    let descriptor = list_detail(IMMEDIATE, false);
    let (state, _) = published(&descriptor, &RelationshipState::new(), subject("42"));

    let (_, updates) = published(&descriptor, &state, subject("42"));

    assert_eq!(updates, Vec::new());
}

#[test]
fn relationships_apply_in_declaration_order() {
    let descriptor = screen(
        vec![
            panel(
                "list",
                true,
                vec![
                    port("selection", PortDirection::Output, SUBJECT_TYPE, false),
                    port("scope", PortDirection::Output, SUBJECT_TYPE, false),
                ],
            ),
            panel(
                "first",
                false,
                vec![port("subject", PortDirection::Input, SUBJECT_TYPE, false)],
            ),
            panel(
                "second",
                false,
                vec![port("subject", PortDirection::Input, SUBJECT_TYPE, false)],
            ),
        ],
        vec![
            Relationship {
                kind: IMMEDIATE,
                source: port_ref("list", "selection"),
                target: port_ref("first", "subject"),
            },
            Relationship {
                kind: RelationshipKind::Scope,
                source: port_ref("list", "selection"),
                target: port_ref("second", "subject"),
            },
        ],
    );

    let (_, updates) = published(&descriptor, &RelationshipState::new(), subject("42"));

    let ports: Vec<String> = updates
        .iter()
        .map(|update| update.port.to_string())
        .collect();
    assert_eq!(
        ports,
        vec![
            "list.selection".to_owned(),
            "first.subject".to_owned(),
            "second.subject".to_owned()
        ]
    );
}

#[test]
fn propagation_never_moves_focus_because_it_only_reports_port_changes() {
    let descriptor = list_detail(IMMEDIATE, false);

    let (_, updates) = published(&descriptor, &RelationshipState::new(), subject("42"));

    assert!(
        updates
            .iter()
            .all(|update| descriptor.port(&update.port).is_some()),
        "every change a transition reports must name a declared port"
    );
}

// ── CW05-06: explicit relationships ────────────────────────────────────────

const EXPLICIT: RelationshipKind = RelationshipKind::MasterDetail {
    activation: ActivationMode::Explicit,
    empty: EmptyPolicy::Retain,
};

#[test]
fn an_explicit_relationship_stages_the_selection_and_leaves_the_target_alone() {
    let descriptor = list_detail(EXPLICIT, false);

    let (state, updates) = published(&descriptor, &RelationshipState::new(), subject("42"));

    assert_eq!(
        updates,
        vec![PortUpdate {
            port: port_ref("list", "selection"),
            value: subject("42"),
        }]
    );
    assert_eq!(
        state.value(&port_ref("detail", "subject")),
        PortValue::Absent
    );
    assert_eq!(
        state.staged(&port_ref("detail", "subject")),
        Some(&subject("42"))
    );
}

#[test]
fn an_explicit_relationship_applies_its_staged_selection_on_activation() {
    let descriptor = list_detail(EXPLICIT, false);
    let (staged, _) = published(&descriptor, &RelationshipState::new(), subject("42"));

    let transition = propagate(
        &descriptor,
        &staged,
        &SourceIntent::Activate {
            target: port_ref("detail", "subject"),
        },
    )
    .unwrap_or_else(|error| unreachable!("activation must commit: {error}"));

    assert_eq!(
        transition.updates,
        vec![PortUpdate {
            port: port_ref("detail", "subject"),
            value: subject("42"),
        }]
    );
    assert_eq!(
        transition.state.staged(&port_ref("detail", "subject")),
        None
    );
}

#[test]
fn activating_a_target_with_nothing_staged_changes_nothing() {
    let descriptor = list_detail(EXPLICIT, false);

    let transition = propagate(
        &descriptor,
        &RelationshipState::new(),
        &SourceIntent::Activate {
            target: port_ref("detail", "subject"),
        },
    )
    .unwrap_or_else(|error| unreachable!("activation must commit: {error}"));

    assert_eq!(transition.updates, Vec::new());
    assert_eq!(transition.state, RelationshipState::new());
}

#[test]
fn an_explicit_relationship_stages_only_the_latest_selection() {
    let descriptor = list_detail(EXPLICIT, false);
    let (first, _) = published(&descriptor, &RelationshipState::new(), subject("41"));

    let (second, _) = published(&descriptor, &first, subject("42"));

    assert_eq!(
        second.staged(&port_ref("detail", "subject")),
        Some(&subject("42"))
    );
}

// ── CW05-07: the closed empty and retention policy table ───────────────────

/// Publish a subject and then absence, returning what the target ends up with.
fn target_after_absence(descriptor: &ScreenDescriptor) -> PortValue {
    let (populated, _) = published(descriptor, &RelationshipState::new(), subject("42"));
    let (emptied, _) = published(descriptor, &populated, PortValue::Absent);
    emptied.value(&port_ref("detail", "subject"))
}

#[test]
fn a_nonretained_input_clears_whatever_its_relationship_declares() {
    for empty in [
        EmptyPolicy::ShowNone,
        EmptyPolicy::ShowAll,
        EmptyPolicy::Retain,
    ] {
        let descriptor = list_detail(
            RelationshipKind::MasterDetail {
                activation: ActivationMode::Immediate,
                empty,
            },
            false,
        );

        assert_eq!(
            target_after_absence(&descriptor),
            PortValue::Absent,
            "a panel that does not retain must not be handed a value it cannot hold ({empty:?})"
        );
    }
}

#[test]
fn a_retained_input_follows_its_declared_empty_policy() {
    for (empty, expected) in [
        (EmptyPolicy::ShowNone, PortValue::Absent),
        (EmptyPolicy::ShowAll, PortValue::All),
        (EmptyPolicy::Retain, subject("42")),
    ] {
        let descriptor = list_detail(
            RelationshipKind::MasterDetail {
                activation: ActivationMode::Immediate,
                empty,
            },
            true,
        );

        assert_eq!(
            target_after_absence(&descriptor),
            expected,
            "{empty:?} must be applied exactly"
        );
    }
}

#[test]
fn a_retained_session_target_follows_its_declared_empty_policy() {
    for (empty, expected) in [
        (SessionEmptyPolicy::Detach, PortValue::Absent),
        (SessionEmptyPolicy::Retain, subject("42")),
    ] {
        let descriptor = list_detail(RelationshipKind::SessionTarget { empty }, true);

        assert_eq!(
            target_after_absence(&descriptor),
            expected,
            "{empty:?} must be applied exactly"
        );
    }
}

#[test]
fn a_retained_scope_target_keeps_its_value_because_scope_declares_no_policy() {
    let descriptor = list_detail(RelationshipKind::Scope, true);

    assert_eq!(target_after_absence(&descriptor), subject("42"));
}

#[test]
fn an_absent_source_applies_the_empty_policy_at_once_even_on_an_explicit_edge() {
    let descriptor = list_detail(
        RelationshipKind::MasterDetail {
            activation: ActivationMode::Explicit,
            empty: EmptyPolicy::ShowAll,
        },
        true,
    );
    let (staged, _) = published(&descriptor, &RelationshipState::new(), subject("42"));

    let (emptied, _) = published(&descriptor, &staged, PortValue::Absent);

    assert_eq!(
        emptied.value(&port_ref("detail", "subject")),
        PortValue::All,
        "a vanished source is not a selection the user might still confirm"
    );
    assert_eq!(emptied.staged(&port_ref("detail", "subject")), None);
}

// ── CW05-08: the transition bound ──────────────────────────────────────────

/// A screen whose single source drives `count` targets, which the graph rules
/// forbid but the propagation bound must still defend against.
fn over_wide_screen(count: usize) -> ScreenDescriptor {
    let source = panel(
        "list",
        true,
        vec![port(
            "selection",
            PortDirection::Output,
            SUBJECT_TYPE,
            false,
        )],
    );
    let sink = panel(
        "detail",
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
    );
    let relationships = (0..count)
        .map(|index| Relationship {
            kind: IMMEDIATE,
            source: port_ref("list", "selection"),
            target: port_ref("detail", &format!("in{index}")),
        })
        .collect();
    screen(vec![source, sink], relationships)
}

#[test]
fn a_transition_at_the_follow_up_bound_commits() {
    let descriptor = over_wide_screen(FOLLOW_UP_LIMIT);

    let transition = propagate(
        &descriptor,
        &RelationshipState::new(),
        &SourceIntent::Publish {
            port: port_ref("list", "selection"),
            value: subject("42"),
        },
    );

    assert_eq!(
        transition.map(|committed| committed.follow_ups),
        Ok(FOLLOW_UP_LIMIT),
        "the source's own publication is what caused the transition, not a follow-up"
    );
}

#[test]
fn staging_an_explicit_selection_counts_against_the_follow_up_bound() {
    // Staging changes no port, so counting reported updates would let an
    // explicit graph mutate more than the bound allows without noticing.
    let mut descriptor = over_wide_screen(FOLLOW_UP_LIMIT + 1);
    for relationship in &mut descriptor.relationships {
        relationship.kind = RelationshipKind::MasterDetail {
            activation: ActivationMode::Explicit,
            empty: EmptyPolicy::Retain,
        };
    }

    let transition = propagate(
        &descriptor,
        &RelationshipState::new(),
        &SourceIntent::Publish {
            port: port_ref("list", "selection"),
            value: subject("42"),
        },
    );

    assert_eq!(
        transition.err(),
        Some(PropagationAbort::FollowUpLimit {
            attempted: FOLLOW_UP_LIMIT + 1
        })
    );
}

#[test]
fn a_transition_that_moves_nothing_performs_no_follow_ups() {
    let descriptor = list_detail(IMMEDIATE, false);

    let transition = propagate(
        &descriptor,
        &RelationshipState::new(),
        &SourceIntent::Activate {
            target: port_ref("detail", "subject"),
        },
    )
    .unwrap_or_else(|error| unreachable!("activation must commit: {error}"));

    assert_eq!(transition.follow_ups, 0);
}

#[test]
fn a_transition_one_past_the_follow_up_bound_aborts_without_partial_state() {
    let descriptor = over_wide_screen(FOLLOW_UP_LIMIT + 1);
    let before = RelationshipState::new();

    let transition = propagate(
        &descriptor,
        &before,
        &SourceIntent::Publish {
            port: port_ref("list", "selection"),
            value: subject("42"),
        },
    );

    assert_eq!(
        transition.err(),
        Some(PropagationAbort::FollowUpLimit {
            attempted: FOLLOW_UP_LIMIT + 1
        })
    );
    assert_eq!(
        before,
        RelationshipState::new(),
        "an aborted transition must leave the prior state exactly as it was"
    );
}
