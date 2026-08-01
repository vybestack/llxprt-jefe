//! Lowering and transactional composition (issue #385, CW05-02, CW05-03,
//! CW05-04, CW05-10).

use std::collections::BTreeSet;

use crate::persistence::diagnostic::{CfgCode, Severity};
use crate::persistence::screen_files::{ScreenFileCandidate, ScreenFileRejection};

use super::compose::{ScreenComposition, compose_screens};
use super::compose_fixtures::{candidate, enabled, review_definition, unreadable_candidate};
use super::descriptor::PortRef;
use super::diagnostics::ScrCode;
use super::geometry::{Extent, Rect};
use super::ids::{CustomScreenId, PanelId, PortId, ScreenId, ScreenIdentity};
use super::relationship_propagation::{PortValue, RelationshipState, SourceIntent, propagate};
use super::relationships::{ActivationMode, EmptyPolicy, RelationshipKind};
use super::resolve::{PanelState, resolve_layout};
use super::screens::{ScreenRegistry, builtin_screens};

fn compiled() -> ScreenRegistry {
    builtin_screens().unwrap_or_else(|error| unreachable!("compiled screens must build: {error}"))
}

fn composed(candidates: &[ScreenFileCandidate], active: &[&str]) -> ScreenComposition {
    compose_screens(&compiled(), candidates, &enabled(active))
        .unwrap_or_else(|error| unreachable!("composition must publish: {error}"))
}

fn review_identity() -> ScreenIdentity {
    ScreenIdentity::Custom(
        CustomScreenId::parse("local.review")
            .unwrap_or_else(|error| unreachable!("fixture identity must parse: {error}")),
    )
}

fn panel(value: &'static str) -> PanelId {
    PanelId::parse(value).unwrap_or_else(|error| unreachable!("fixture panel: {error}"))
}

fn port(panel_value: &'static str, port_value: &'static str) -> PortRef {
    PortRef {
        panel: panel(panel_value),
        port: PortId::parse(port_value)
            .unwrap_or_else(|error| unreachable!("fixture port: {error}")),
    }
}

// ── CW05-02: a valid active definition is lowered exactly once ─────────────

#[test]
fn an_enabled_definition_joins_the_registry_after_the_compiled_screens() {
    let composition = composed(&[candidate("review", &review_definition())], &["review"]);

    let identities: Vec<&str> = composition
        .registry
        .screens()
        .iter()
        .map(|screen| screen.id.as_str())
        .collect();
    assert_eq!(identities.last(), Some(&"local.review"));
    assert_eq!(identities.len(), ScreenId::ALL.len() + 1);
    assert!(composition.warnings.is_empty());
}

#[test]
fn the_lowered_descriptor_copies_the_definition_without_inventing_anything() {
    let composition = composed(&[candidate("review", &review_definition())], &["review"]);
    let screen = composition
        .registry
        .get_identity(review_identity())
        .unwrap_or_else(|| unreachable!("the lowered screen must be registered"));

    assert_eq!(screen.title, "Review");
    assert_eq!(screen.route.as_str(), "review");
    assert_eq!(screen.initial_focus.as_str(), "pr-list");
    assert_eq!(
        screen
            .focus_order
            .iter()
            .map(|id| id.as_str())
            .collect::<Vec<_>>(),
        vec!["pr-list", "pr-detail"]
    );
    assert_eq!(
        screen
            .panels
            .iter()
            .map(|declared| (
                declared.id.as_str(),
                declared.panel_type.as_str(),
                declared.focusable,
                declared.required
            ))
            .collect::<Vec<_>>(),
        vec![
            ("pr-list", "pr-list", true, true),
            ("pr-detail", "pr-detail", true, false)
        ]
    );
}

#[test]
fn the_lowered_descriptor_carries_its_declared_ports_and_relationship() {
    let composition = composed(&[candidate("review", &review_definition())], &["review"]);
    let screen = composition
        .registry
        .get_identity(review_identity())
        .unwrap_or_else(|| unreachable!("the lowered screen must be registered"));

    assert_eq!(screen.panels[0].ports[0].id.as_str(), "selection");
    assert_eq!(
        screen.panels[0].ports[0].type_id.as_str(),
        "github.pull-request@1"
    );
    assert!(screen.panels[1].ports[0].retained);
    assert_eq!(screen.relationships.len(), 1);
    assert_eq!(
        screen.relationships[0].kind,
        RelationshipKind::MasterDetail {
            activation: ActivationMode::Immediate,
            empty: EmptyPolicy::Retain
        }
    );
    assert_eq!(screen.relationships[0].source, port("pr-list", "selection"));
    assert_eq!(screen.relationships[0].target, port("pr-detail", "subject"));
}

#[test]
fn lowering_the_same_definition_twice_produces_the_same_descriptor() {
    let first = composed(&[candidate("review", &review_definition())], &["review"]);
    let second = composed(&[candidate("review", &review_definition())], &["review"]);

    assert_eq!(
        first.registry.get_identity(review_identity()),
        second.registry.get_identity(review_identity())
    );
}

#[test]
fn a_lowered_relationship_propagates_through_the_shared_engine() {
    let composition = composed(&[candidate("review", &review_definition())], &["review"]);
    let screen = composition
        .registry
        .get_identity(review_identity())
        .unwrap_or_else(|| unreachable!("the lowered screen must be registered"));

    let transition = propagate(
        screen,
        &RelationshipState::new(),
        &SourceIntent::Publish {
            port: port("pr-list", "selection"),
            value: PortValue::Subject("42".to_owned()),
        },
    )
    .unwrap_or_else(|error| unreachable!("transition must commit: {error}"));

    assert_eq!(
        transition.state.value(&port("pr-detail", "subject")),
        PortValue::Subject("42".to_owned())
    );
}

// ── CW05-10: a lowered screen uses the standard collapse/focus algorithm ───

#[test]
fn a_tiny_lowered_screen_falls_back_through_the_standard_resolver() {
    let composition = composed(&[candidate("review", &review_definition())], &["review"]);
    let screen = composition
        .registry
        .get_identity(review_identity())
        .unwrap_or_else(|| unreachable!("the lowered screen must be registered"));

    let roomy = resolve_layout(
        screen,
        super::ids::ScreenInstanceId::next(),
        Rect::new(0, 0, 80, 24),
        &PanelState::all_visible(),
    )
    .unwrap_or_else(|error| unreachable!("layout must resolve: {error}"));
    let tiny = resolve_layout(
        screen,
        super::ids::ScreenInstanceId::next(),
        Rect::new(0, 0, 10, 3),
        &PanelState::all_visible(),
    )
    .unwrap_or_else(|error| unreachable!("layout must resolve: {error}"));

    assert!(roomy.too_small.is_none());
    assert_eq!(roomy.visible_panels().count(), 2);
    assert_eq!(
        tiny.too_small.map(|small| small.available),
        Some(Extent::new(10, 3))
    );
    assert_eq!(
        tiny.visible_panels()
            .map(|resolved| resolved.id.as_str())
            .collect::<Vec<_>>(),
        vec!["pr-list"],
        "the too-small fallback must preserve the first required focusable panel"
    );
}

// ── CW05-03: an invalid dormant definition is preserved and omitted ────────

const BROKEN_DEFINITION: &str = "screen_schema = 1\nid = \"local.broken\"\n";

#[test]
fn an_invalid_dormant_definition_is_omitted_with_a_warning() {
    let composition = composed(&[candidate("broken", BROKEN_DEFINITION)], &[]);

    assert_eq!(composition.registry.screens().len(), ScreenId::ALL.len());
    assert_eq!(composition.warnings.len(), 1);
    assert_eq!(composition.warnings[0].code, CfgCode::W004);
    assert_eq!(composition.warnings[0].severity, Severity::Warning);
    assert_eq!(
        composition.warnings[0].path.as_str(),
        candidate("broken", "").path.to_string_lossy(),
        "a warning must name the file it is about"
    );
}

#[test]
fn a_valid_dormant_definition_is_omitted_without_a_warning() {
    let composition = composed(&[candidate("review", &review_definition())], &[]);

    assert_eq!(composition.registry.screens().len(), ScreenId::ALL.len());
    assert!(composition.warnings.is_empty());
}

#[test]
fn one_broken_dormant_definition_does_not_stop_an_enabled_one() {
    let composition = composed(
        &[
            candidate("broken", BROKEN_DEFINITION),
            candidate("review", &review_definition()),
        ],
        &["review"],
    );

    assert!(
        composition
            .registry
            .get_identity(review_identity())
            .is_some()
    );
    assert_eq!(composition.warnings.len(), 1);
}

// ── CW05-04: an invalid enabled definition refuses the whole registry ──────

/// Compose with `review` enabled and return the refusal it produced.
fn refused(text: &str) -> super::compose::CompositionRefused {
    let Err(refusal) = compose_screens(
        &compiled(),
        &[candidate("review", text)],
        &enabled(&["review"]),
    ) else {
        unreachable!("composition must be refused")
    };
    refusal
}

#[test]
fn an_unparseable_enabled_definition_refuses_publication() {
    let refusal = refused("this is not toml {{{");

    assert_eq!(refusal.screen.code, ScrCode::E301);
    assert_eq!(refusal.configuration.code, CfgCode::E006);
    assert_eq!(
        refusal.screen.path.as_str(),
        candidate("review", "").path.to_string_lossy(),
        "a refusal must name the file it is about"
    );
}

#[test]
fn an_unreadable_enabled_definition_refuses_publication() {
    let Err(refusal) = compose_screens(
        &compiled(),
        &[unreadable_candidate(
            "review",
            ScreenFileRejection::TooLarge { bytes: 2_000_000 },
        )],
        &enabled(&["review"]),
    ) else {
        unreachable!("composition must be refused")
    };

    assert_eq!(refusal.screen.code, ScrCode::E301);
    assert_eq!(refusal.configuration.code, CfgCode::E006);
}

#[test]
fn a_definition_claiming_another_files_identity_refuses_publication_as_ownership() {
    let refusal = refused(&review_definition().replace("local.review", "local.elsewhere"));

    assert_eq!(refusal.screen.code, ScrCode::E301);
    assert_eq!(
        refusal.configuration.code,
        CfgCode::E005,
        "claiming an identity that is not the file's is an ownership failure"
    );
}

#[test]
fn a_definition_naming_an_unknown_panel_type_refuses_publication_as_ownership() {
    let refusal =
        refused(&review_definition().replace("type = \"pr-list\"", "type = \"invented\""));

    assert_eq!(refusal.configuration.code, CfgCode::E005);
}

#[test]
fn a_definition_may_not_request_a_pty_panel() {
    let refusal =
        refused(&review_definition().replace("type = \"pr-list\"", "type = \"pty-terminal\""));

    assert_eq!(refusal.configuration.code, CfgCode::E005);
    assert!(
        refusal.screen.redacted_detail.contains("may not be"),
        "the refusal must say a definition is not allowed to request it, got {:?}",
        refusal.screen.redacted_detail
    );
}

#[test]
fn a_definition_naming_an_unknown_action_refuses_publication_as_a_reference() {
    let text = format!(
        "{}\n[[bindings]]\ncontext = \"global\"\naction = \"no-such-action\"\n",
        review_definition()
    );

    let refusal = refused(&text);

    assert_eq!(refusal.configuration.code, CfgCode::E006);
}

#[test]
fn a_definition_whose_layout_omits_a_panel_refuses_publication() {
    let text = review_definition().replace(
        "[[layout.children]]\nmin = 20\ncollapsible = true\ncollapse_priority = 0\nsize = { weight = 1 }\nnode = { type = \"leaf\", panel = \"pr-detail\" }\n",
        "[[layout.children]]\nmin = 20\ncollapsible = true\ncollapse_priority = 0\nsize = { weight = 1 }\nnode = { type = \"leaf\", panel = \"pr-list\" }\n",
    );

    let refusal = refused(&text);

    assert_eq!(refusal.screen.code, ScrCode::E301);
}

#[test]
fn a_definition_with_a_cyclic_relationship_refuses_publication() {
    let text = review_definition().replace(
        "source = \"pr-list.selection\"\ntarget = \"pr-detail.subject\"",
        "source = \"pr-list.selection\"\ntarget = \"pr-list.selection\"",
    );

    let refusal = refused(&text);

    assert_eq!(refusal.screen.code, ScrCode::E301);
}

#[test]
fn refusing_one_enabled_definition_publishes_nothing_at_all() {
    let outcome = compose_screens(
        &compiled(),
        &[
            candidate("review", &review_definition()),
            candidate("broken", BROKEN_DEFINITION),
        ],
        &enabled(&["review", "broken"]),
    );

    assert!(
        outcome.is_err(),
        "one broken enabled definition must refuse the whole candidate registry"
    );
}

#[test]
fn composing_no_candidates_publishes_exactly_the_compiled_screens() {
    let composition = composed(&[], &[]);

    assert_eq!(composition.registry, compiled());
    assert!(composition.warnings.is_empty());
}

#[test]
fn an_enabled_member_with_no_file_is_simply_absent() {
    let composition = compose_screens(&compiled(), &[], &enabled(&["review"]))
        .unwrap_or_else(|error| unreachable!("composition must publish: {error}"));

    assert_eq!(composition.registry.screens().len(), ScreenId::ALL.len());
}

#[test]
fn a_lowered_screen_is_not_resolvable_from_persisted_text() {
    let composition = composed(&[candidate("review", &review_definition())], &["review"]);

    assert_eq!(
        composition.registry.resolve("local.review"),
        None,
        "a screen with no renderer must not be restorable as the active screen"
    );
    assert_eq!(
        composition
            .registry
            .initial_screen()
            .map(|screen| screen.id.as_str()),
        compiled().initial_screen().map(|screen| screen.id.as_str()),
        "adding a lowered screen must not move the fallback"
    );
}

#[test]
fn a_definition_is_left_out_when_the_enabled_set_is_empty() {
    let composition = compose_screens(
        &compiled(),
        &[candidate("review", &review_definition())],
        &BTreeSet::new(),
    )
    .unwrap_or_else(|error| unreachable!("composition must publish: {error}"));

    assert_eq!(composition.registry.screens().len(), ScreenId::ALL.len());
}

// ── Dormant candidates are inspected, not lowered (review remediation) ─────

#[test]
fn a_dormant_definition_is_parsed_but_never_lowered() {
    // The panel type does not exist, which only lowering would notice. A
    // dormant file must not be lowered, so this parses and produces no
    // warning even though it could never be enabled as written.
    let unlowerable = review_definition().replace("type = \"pr-list\"", "type = \"invented\"");
    assert_ne!(
        unlowerable,
        review_definition(),
        "the example must still declare the panel type this test replaces"
    );

    let composition = composed(&[candidate("review", &unlowerable)], &[]);

    assert!(
        composition.warnings.is_empty(),
        "a dormant definition is inspected for well-formedness and no further"
    );
    assert_eq!(composition.registry.screens().len(), ScreenId::ALL.len());
}

#[test]
fn the_same_definition_enabled_is_lowered_and_refused() {
    let unlowerable = review_definition().replace("type = \"pr-list\"", "type = \"invented\"");

    let refusal = refused(&unlowerable);

    assert_eq!(refusal.configuration.code, CfgCode::E005);
}

#[test]
fn a_refusal_names_the_rule_without_repeating_the_files_prose() {
    let text = review_definition().replace("title = \"Review\"", "title = 7");

    let refusal = refused(&text);

    assert!(
        !refusal.screen.redacted_detail.contains("Review"),
        "a refusal must not repeat the file's prose, got {:?}",
        refusal.screen.redacted_detail
    );
}
