//! Behavioral tests for CW-03 S1 registry composition and resolution.

use super::Id;
use super::action_registry::{
    Action, ActionAvailability, ActionId, ActionMetadata, ActionRegistrySnapshot, Availability,
    AvailabilityGeneration, Binding, BindingOverride, HandlerKey, Provenance, RegistryCandidate,
    RegistryDiagnosticKind, Resolution,
};
use super::effects::{Correlation, CorrelationId, EffectFamily, SemanticKey};
use super::input_context::{ContextId, ContextStack};
use super::keymap::{Chord, Key, MAX_EFFECTIVE_BINDINGS, Modifier, ModifierSet};

fn context(text: &str) -> ContextId {
    let Ok(value) = ContextId::parse(text) else {
        panic!("context {text:?} should parse");
    };
    value
}

fn action_id(text: &str) -> ActionId {
    let Ok(value) = ActionId::parse(text) else {
        panic!("action {text:?} should parse");
    };
    value
}

fn chord(text: &str) -> Chord {
    let Ok(value) = Chord::parse(text) else {
        panic!("chord {text:?} should parse");
    };
    value
}

fn action(id: &str, context_name: &str, handler: HandlerKey, protected: bool) -> Action {
    let result = Action::new(
        ActionMetadata {
            id: action_id(id),
            label: id.to_owned(),
            description: format!("{id} action."),
            category: "test".to_owned(),
            contexts: vec![context(context_name)],
        },
        handler,
        protected,
    );
    let Ok(value) = result else {
        panic!("test action should construct, got {result:?}");
    };
    value
}

fn binding(context_name: &str, action_name: &str, chords: &[&str]) -> Binding {
    let result = Binding::new(
        context(context_name),
        action_id(action_name),
        chords.iter().map(|text| chord(text)).collect(),
        Provenance::Compiled,
    );
    let Ok(value) = result else {
        panic!("test binding should construct, got {result:?}");
    };
    value
}

fn stack(names: &[&str]) -> ContextStack {
    let result = ContextStack::from_ordered(names.iter().copied(), false);
    let Ok(value) = result else {
        panic!("test context stack should construct, got {result:?}");
    };
    value
}

fn terminal_stack(terminal: &str, global: &str) -> ContextStack {
    let result = ContextStack::from_ordered([terminal, global], true);
    let Ok(value) = result else {
        panic!("test terminal stack should construct, got {result:?}");
    };
    value
}

fn correlation(id: u64) -> Correlation {
    let Ok(owner) = Id::parse("core.keymap") else {
        panic!("correlation owner should parse");
    };
    Correlation {
        correlation_id: CorrelationId::new(id),
        owner,
        screen_generation: 3,
        activation_generation: 7,
        semantic_key: SemanticKey::new(EffectFamily::Provider, "action-availability"),
    }
}

fn availability(actions: &[Action], unavailable: Option<(&str, &str)>) -> AvailabilityGeneration {
    let entries = actions
        .iter()
        .map(|action| {
            let value = match unavailable {
                Some((id, reason)) if action.id.as_str() == id => Availability::Unavailable {
                    reason: reason.to_owned(),
                },
                Some(_) | None => Availability::Available,
            };
            ActionAvailability::new(action.id.clone(), value)
        })
        .collect();
    AvailabilityGeneration::new(correlation(41), entries)
}

fn compose(
    actions: Vec<Action>,
    bindings: Vec<Binding>,
    overrides: Vec<BindingOverride>,
    stacks: Vec<ContextStack>,
    unavailable: Option<(&str, &str)>,
) -> Result<ActionRegistrySnapshot, super::action_registry::RegistryDiagnostic> {
    let generated = availability(&actions, unavailable);
    RegistryCandidate::new(actions, bindings, overrides, stacks, generated).compose()
}

#[test]
fn context_resolution_searches_child_to_parent_and_explicit_child_shadow_wins() {
    let actions = vec![
        action(
            "test.modal",
            "modal.confirm",
            HandlerKey::ConfirmAccept,
            false,
        ),
        action(
            "test.editor",
            "issues.inline",
            HandlerKey::IssuesSubmitInline,
            false,
        ),
        action("test.panel", "issues.detail", HandlerKey::IssuesOpen, false),
        action("test.screen", "issues.list", HandlerKey::IssuesExit, false),
        action("test.global", "global", HandlerKey::OpenKeys, false),
    ];
    let bindings = vec![
        binding("modal.confirm", "test.modal", &["m"]),
        binding("issues.inline", "test.editor", &["e"]),
        binding("issues.detail", "test.panel", &["p"]),
        binding("issues.list", "test.screen", &["s"]),
        binding("global", "test.global", &["g"]),
    ];
    let order = stack(&[
        "modal.confirm",
        "issues.inline",
        "issues.detail",
        "issues.list",
        "global",
    ]);
    let overrides = vec![BindingOverride::new(
        context("modal.confirm"),
        action_id("test.modal"),
        vec![chord("g")],
        "settings.toml",
    )];
    let result = compose(actions, bindings, overrides, vec![order.clone()], None);
    let Ok(snapshot) = result else {
        panic!("explicit child shadow should compose, got {result:?}");
    };

    assert_eq!(
        snapshot.resolve(&chord("g"), &order),
        Resolution::Dispatch {
            action: action_id("test.modal"),
            handler: HandlerKey::ConfirmAccept,
        }
    );
    assert_eq!(
        snapshot.resolve(&chord("e"), &order),
        Resolution::Dispatch {
            action: action_id("test.editor"),
            handler: HandlerKey::IssuesSubmitInline,
        }
    );
    assert_eq!(snapshot.resolve(&chord("x"), &order), Resolution::Unbound);
}

#[test]
fn unavailable_resolution_owns_one_reason_and_preserves_exact_correlation() {
    let actions = vec![action(
        "test.merge",
        "prs.detail",
        HandlerKey::PullRequestsOpenMerge,
        false,
    )];
    let bindings = vec![binding("prs.detail", "test.merge", &["m"])];
    let order = stack(&["prs.detail"]);
    let result = compose(
        actions,
        bindings,
        Vec::new(),
        vec![order.clone()],
        Some(("test.merge", "no pull request selected")),
    );
    let Ok(snapshot) = result else {
        panic!("unavailable snapshot should compose, got {result:?}");
    };

    assert_eq!(snapshot.availability_correlation(), &correlation(41));
    assert_eq!(
        snapshot.resolve(&chord("m"), &order),
        Resolution::Unavailable {
            action: action_id("test.merge"),
            reason: "no pull request selected".to_owned(),
        }
    );
}

#[test]
fn terminal_capture_intercepts_only_recovery_and_scrollback_bindings() {
    let actions = vec![
        action(
            "core.emergency-exit",
            "global",
            HandlerKey::EmergencyExit,
            true,
        ),
        action(
            "core.leave-terminal",
            "terminal",
            HandlerKey::LeaveTerminal,
            true,
        ),
        action(
            "terminal.scroll-up",
            "terminal",
            HandlerKey::TerminalScrollUp,
            false,
        ),
        action("test.ordinary", "terminal", HandlerKey::OpenKeys, false),
    ];
    let bindings = vec![
        binding("global", "core.emergency-exit", &["Ctrl+Q"]),
        binding("terminal", "core.leave-terminal", &["F12"]),
        binding("terminal", "terminal.scroll-up", &["Up"]),
        binding("terminal", "test.ordinary", &["x"]),
    ];
    let order = terminal_stack("terminal", "global");
    let result = compose(actions, bindings, Vec::new(), vec![order.clone()], None);
    let Ok(snapshot) = result else {
        panic!("terminal snapshot should compose, got {result:?}");
    };

    for (text, expected_handler) in [
        ("Ctrl+Q", HandlerKey::EmergencyExit),
        ("F12", HandlerKey::LeaveTerminal),
        ("Up", HandlerKey::TerminalScrollUp),
    ] {
        assert!(matches!(
            snapshot.resolve(&chord(text), &order),
            Resolution::Dispatch { handler, .. } if handler == expected_handler
        ));
    }
    assert_eq!(
        snapshot.resolve(&chord("x"), &order),
        Resolution::ForwardToPty
    );
    assert_eq!(
        snapshot.resolve(&chord("Ctrl+C"), &order),
        Resolution::ForwardToPty
    );
}

#[test]
fn conflict_validator_rejects_same_context_alias_duplicate_and_implicit_shadow() {
    let same_context_actions = vec![
        action("test.first", "screen", HandlerKey::NavigateUp, false),
        action("test.second", "screen", HandlerKey::NavigateDown, false),
    ];
    let same_context = compose(
        same_context_actions,
        vec![
            binding("screen", "test.first", &["BackTab"]),
            binding("screen", "test.second", &["Shift+Tab"]),
        ],
        Vec::new(),
        vec![stack(&["screen"])],
        None,
    );
    assert!(matches!(
        same_context,
        Err(ref diagnostic)
            if diagnostic.code() == "KEY-E401"
                && matches!(diagnostic.kind(), RegistryDiagnosticKind::ContextConflict(..))
    ));

    let actions = vec![
        action("test.child", "child", HandlerKey::NavigateUp, false),
        action("test.parent", "parent", HandlerKey::NavigateDown, false),
    ];
    let implicit = compose(
        actions,
        vec![
            binding("child", "test.child", &["x"]),
            binding("parent", "test.parent", &["y"]),
        ],
        vec![BindingOverride::new(
            context("parent"),
            action_id("test.parent"),
            vec![chord("x")],
            "settings.toml",
        )],
        vec![stack(&["child", "parent"])],
        None,
    );
    assert!(matches!(
        implicit,
        Err(ref diagnostic)
            if diagnostic.code() == "KEY-E401"
                && matches!(diagnostic.kind(), RegistryDiagnosticKind::ImplicitShadow(..))
    ));
}

#[test]
fn candidate_rejects_unknown_targets_duplicate_override_chords_and_targets() {
    let actions = vec![action("test.action", "screen", HandlerKey::OpenKeys, false)];
    let bindings = vec![binding("screen", "test.action", &["x"])];
    let order = stack(&["screen"]);

    let unknown_action = compose(
        actions.clone(),
        bindings.clone(),
        vec![BindingOverride::new(
            context("screen"),
            action_id("test.unknown"),
            vec![chord("y")],
            "settings.toml",
        )],
        vec![order.clone()],
        None,
    );
    assert!(matches!(
        unknown_action,
        Err(ref diagnostic)
            if matches!(diagnostic.kind(), RegistryDiagnosticKind::UnknownAction(..))
    ));

    let duplicate_chord = compose(
        actions.clone(),
        bindings.clone(),
        vec![BindingOverride::new(
            context("screen"),
            action_id("test.action"),
            vec![chord("BackTab"), chord("Shift+Tab")],
            "settings.toml",
        )],
        vec![order.clone()],
        None,
    );
    assert!(matches!(
        duplicate_chord,
        Err(ref diagnostic)
            if matches!(diagnostic.kind(), RegistryDiagnosticKind::DuplicateChord(..))
    ));

    let replacement = BindingOverride::new(
        context("screen"),
        action_id("test.action"),
        vec![chord("y")],
        "settings.toml",
    );
    let duplicate_target = compose(
        actions,
        bindings,
        vec![replacement.clone(), replacement],
        vec![order],
        None,
    );
    assert!(matches!(
        duplicate_target,
        Err(ref diagnostic)
            if matches!(diagnostic.kind(), RegistryDiagnosticKind::DuplicateOverride(..))
    ));
}

#[test]
fn candidate_rejects_unknown_override_context() {
    let actions = vec![action("test.action", "screen", HandlerKey::OpenKeys, false)];
    let bindings = vec![binding("screen", "test.action", &["x"])];
    let result = compose(
        actions,
        bindings,
        vec![BindingOverride::new(
            context("unknown"),
            action_id("test.action"),
            vec![chord("y")],
            "settings.toml",
        )],
        vec![stack(&["screen"])],
        None,
    );
    assert!(matches!(
        result,
        Err(ref diagnostic)
            if matches!(diagnostic.kind(), RegistryDiagnosticKind::UnknownContext(..))
    ));
}

#[test]
fn protected_bindings_cannot_be_unbound_shadowed_or_unavailable() {
    let actions = vec![
        action(
            "core.emergency-exit",
            "global",
            HandlerKey::EmergencyExit,
            true,
        ),
        action("test.child", "screen", HandlerKey::OpenKeys, false),
    ];
    let bindings = vec![
        binding("global", "core.emergency-exit", &["Ctrl+Q"]),
        binding("screen", "test.child", &["x"]),
    ];
    let order = stack(&["screen", "global"]);

    let unbound = compose(
        actions.clone(),
        bindings.clone(),
        vec![BindingOverride::new(
            context("global"),
            action_id("core.emergency-exit"),
            Vec::new(),
            "settings.toml",
        )],
        vec![order.clone()],
        None,
    );
    assert!(matches!(
        unbound,
        Err(ref diagnostic)
            if matches!(diagnostic.kind(), RegistryDiagnosticKind::ProtectedUnbound(..))
    ));

    let shadowed = compose(
        actions.clone(),
        bindings.clone(),
        vec![BindingOverride::new(
            context("screen"),
            action_id("test.child"),
            vec![chord("Ctrl+Q")],
            "settings.toml",
        )],
        vec![order.clone()],
        None,
    );
    assert!(matches!(
        shadowed,
        Err(ref diagnostic)
            if matches!(diagnostic.kind(), RegistryDiagnosticKind::ProtectedShadowed(..))
    ));

    let unavailable = compose(
        actions,
        bindings,
        Vec::new(),
        vec![order],
        Some(("core.emergency-exit", "disabled")),
    );
    assert!(matches!(
        unavailable,
        Err(ref diagnostic)
            if matches!(diagnostic.kind(), RegistryDiagnosticKind::ProtectedUnavailable(..))
    ));
}

#[test]
fn nested_protected_local_unwinds_remain_reachable_in_child_order() {
    let actions = vec![
        action("test.modal-back", "modal", HandlerKey::ConfirmCancel, true),
        action("test.screen-back", "screen", HandlerKey::IssuesBack, true),
    ];
    let bindings = vec![
        binding("modal", "test.modal-back", &["Esc"]),
        binding("screen", "test.screen-back", &["Esc"]),
    ];
    let order = stack(&["modal", "screen"]);
    let result = compose(actions, bindings, Vec::new(), vec![order.clone()], None);
    let Ok(snapshot) = result else {
        panic!("nested local unwind bindings should remain reachable, got {result:?}");
    };
    assert_eq!(
        snapshot.resolve(&chord("Esc"), &order),
        Resolution::Dispatch {
            action: action_id("test.modal-back"),
            handler: HandlerKey::ConfirmCancel,
        }
    );
}

fn bounded_candidate(chord_count: usize) -> RegistryCandidate {
    let context_id = context("bounds");
    let mut actions = Vec::new();
    let mut bindings = Vec::new();
    for (action_index, first) in (0..chord_count).step_by(8).enumerate() {
        let id_text = format!("test.bound-{action_index}");
        let action = action(&id_text, "bounds", HandlerKey::OpenKeys, false);
        let chords = (first..chord_count.min(first + 8))
            .map(|index| {
                let Ok(offset) = u32::try_from(index) else {
                    panic!("bounded test index should fit u32");
                };
                let Some(character) = char::from_u32(0x1000 + offset) else {
                    panic!("test scalar should be valid");
                };
                Chord::new(ModifierSet::empty(), Key::Char(character))
            })
            .collect();
        let result = Binding::new(
            context_id.clone(),
            action.id.clone(),
            chords,
            Provenance::Compiled,
        );
        let Ok(binding) = result else {
            panic!("bounded binding should construct, got {result:?}");
        };
        actions.push(action);
        bindings.push(binding);
    }
    let generated = availability(&actions, None);
    RegistryCandidate::new(
        actions,
        bindings,
        Vec::new(),
        vec![stack(&["bounds"])],
        generated,
    )
}

#[test]
fn availability_republication_is_atomic_and_preserves_exact_correlation() {
    let actions = vec![action("test.action", "screen", HandlerKey::OpenKeys, false)];
    let snapshot = compose(
        actions.clone(),
        vec![binding("screen", "test.action", &["x"])],
        Vec::new(),
        vec![stack(&["screen"])],
        None,
    );
    let Ok(snapshot) = snapshot else {
        panic!("baseline snapshot must compose: {snapshot:?}");
    };
    let generation = AvailabilityGeneration::new(
        correlation(99),
        vec![ActionAvailability::new(
            action_id("test.action"),
            Availability::Unavailable {
                reason: "exact reason".to_owned(),
            },
        )],
    );
    let published = snapshot.publish_availability(generation);
    let Ok(published) = published else {
        panic!("complete generation must publish: {published:?}");
    };
    assert_eq!(published.availability_correlation(), &correlation(99));
    assert_eq!(
        published.resolve(&chord("x"), &stack(&["screen"])),
        Resolution::Unavailable {
            action: action_id("test.action"),
            reason: "exact reason".to_owned(),
        }
    );

    let incomplete = AvailabilityGeneration::new(correlation(100), Vec::new());
    assert!(matches!(
        published.publish_availability(incomplete),
        Err(ref diagnostic)
            if matches!(diagnostic.kind(), RegistryDiagnosticKind::MissingAvailability(_))
    ));
}

#[test]
fn complete_candidate_owns_exact_eight_nine_and_2048_2049_limits() {
    let actions = vec![action("test.action", "screen", HandlerKey::OpenKeys, false)];
    let bindings = vec![binding("screen", "test.action", &["x"])];
    let order = stack(&["screen"]);
    let eight = (1..=8).map(|number| chord(&format!("F{number}"))).collect();
    let accepted = compose(
        actions.clone(),
        bindings.clone(),
        vec![BindingOverride::new(
            context("screen"),
            action_id("test.action"),
            eight,
            "settings.toml",
        )],
        vec![order.clone()],
        None,
    );
    assert!(
        accepted.is_ok(),
        "eight chords must be accepted: {accepted:?}"
    );

    let nine = (1..=9).map(|number| chord(&format!("F{number}"))).collect();
    let rejected = compose(
        actions,
        bindings,
        vec![BindingOverride::new(
            context("screen"),
            action_id("test.action"),
            nine,
            "settings.toml",
        )],
        vec![order],
        None,
    );
    assert!(matches!(
        rejected,
        Err(ref diagnostic)
            if matches!(diagnostic.kind(), RegistryDiagnosticKind::TooManyChords(_, _, 9, _))
    ));

    assert!(bounded_candidate(MAX_EFFECTIVE_BINDINGS).compose().is_ok());
    let too_many = bounded_candidate(MAX_EFFECTIVE_BINDINGS + 1).compose();
    assert!(matches!(
        too_many,
        Err(ref diagnostic)
            if matches!(
                diagnostic.kind(),
                RegistryDiagnosticKind::TooManyEffectiveBindings(count, _)
                    if *count == MAX_EFFECTIVE_BINDINGS + 1
            )
    ));
}

#[test]
fn normalized_shift_backtab_protects_recovery_without_wildcard_matching() {
    let actions = vec![
        action("core.back", "screen", HandlerKey::ErrorsBack, true),
        action(
            "test.child",
            "modal.confirm",
            HandlerKey::ConfirmAccept,
            false,
        ),
    ];
    let bindings = vec![
        binding("screen", "core.back", &["BackTab"]),
        binding("modal.confirm", "test.child", &["x"]),
    ];
    let mut shift = ModifierSet::empty();
    assert!(shift.insert(Modifier::Shift).is_ok());
    let alias = Chord::new(shift, Key::Tab);
    let result = compose(
        actions,
        bindings,
        vec![BindingOverride::new(
            context("modal.confirm"),
            action_id("test.child"),
            vec![alias],
            "settings.toml",
        )],
        vec![stack(&["modal.confirm", "screen"])],
        None,
    );
    assert!(matches!(
        result,
        Err(ref diagnostic)
            if matches!(diagnostic.kind(), RegistryDiagnosticKind::ProtectedShadowed(..))
    ));
}
