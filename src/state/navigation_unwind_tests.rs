//! One Back key unwinds exactly one layer (issue #386, CW06-04).

use super::navigation_dirty::DirtyChoice;
use super::navigation_unwind::{BackLayer, BackResolution, LocalIntent, resolve_back};

/// The exact order the contract states, written out independently of the
/// implementation so a reordering of `PRECEDENCE` fails here rather than
/// silently redefining what Back means.
const CONTRACT_ORDER: [BackLayer; 8] = [
    BackLayer::HostConfirmation,
    BackLayer::DirtyGuard,
    BackLayer::Chooser,
    BackLayer::Editor,
    BackLayer::Search,
    BackLayer::Filter,
    BackLayer::Overlay,
    BackLayer::PanelTransient,
];

#[test]
fn the_precedence_order_is_the_one_the_contract_states() {
    assert_eq!(BackLayer::PRECEDENCE, CONTRACT_ORDER);
}

#[test]
fn with_every_layer_open_back_unwinds_only_the_innermost() {
    let all: Vec<BackLayer> = CONTRACT_ORDER.to_vec();
    assert_eq!(
        resolve_back(&all, true),
        BackResolution::Local(LocalIntent::CloseHostConfirmation)
    );
}

#[test]
fn back_unwinds_the_stack_of_layers_one_press_at_a_time() {
    // Start with everything open and peel one layer per press, asserting that
    // each press closes exactly the next layer in the contract order.
    let mut open: Vec<BackLayer> = CONTRACT_ORDER.to_vec();
    for layer in CONTRACT_ORDER {
        assert_eq!(
            resolve_back(&open, true),
            BackResolution::Local(layer.intent()),
            "with {open:?} open, Back must unwind {layer:?}"
        );
        open.retain(|candidate| *candidate != layer);
    }
    assert!(open.is_empty());
    assert_eq!(resolve_back(&open, true), BackResolution::Leave);
}

#[test]
fn each_layer_alone_resolves_to_its_own_intent() {
    for layer in CONTRACT_ORDER {
        assert_eq!(
            resolve_back(&[layer], true),
            BackResolution::Local(layer.intent())
        );
    }
}

#[test]
fn every_layer_maps_to_a_distinct_intent() {
    let intents: Vec<LocalIntent> = CONTRACT_ORDER.into_iter().map(BackLayer::intent).collect();
    for (index, intent) in intents.iter().enumerate() {
        assert!(
            !intents[..index].contains(intent),
            "{intent:?} is produced by two layers"
        );
    }
}

#[test]
fn an_outer_layer_never_pre_empts_an_inner_one() {
    // Every pair, in both listed orders, must resolve to the earlier layer.
    for (index, inner) in CONTRACT_ORDER.into_iter().enumerate() {
        for outer in CONTRACT_ORDER.into_iter().skip(index + 1) {
            for open in [vec![inner, outer], vec![outer, inner]] {
                assert_eq!(
                    resolve_back(&open, true),
                    BackResolution::Local(inner.intent()),
                    "{inner:?} must win over {outer:?} regardless of listing order"
                );
            }
        }
    }
}

#[test]
fn the_dirty_guard_is_answered_the_way_the_cancel_control_answers_it() {
    assert_eq!(
        BackLayer::DirtyGuard.intent(),
        LocalIntent::ResolveDirty(DirtyChoice::Cancel)
    );
}

#[test]
fn with_nothing_open_back_leaves_the_screen() {
    assert_eq!(resolve_back(&[], true), BackResolution::Leave);
}

#[test]
fn with_nothing_open_and_nowhere_to_go_back_does_nothing() {
    assert_eq!(resolve_back(&[], false), BackResolution::Nothing);
}

#[test]
fn a_local_layer_is_unwound_even_when_there_is_nowhere_to_go_back_to() {
    // The root screen still has to be able to close its own overlays.
    for layer in CONTRACT_ORDER {
        assert_eq!(
            resolve_back(&[layer], false),
            BackResolution::Local(layer.intent())
        );
    }
}

#[test]
fn resolution_does_not_depend_on_how_often_a_layer_is_listed() {
    let open = [
        BackLayer::Filter,
        BackLayer::Filter,
        BackLayer::Editor,
        BackLayer::Editor,
    ];
    assert_eq!(
        resolve_back(&open, true),
        BackResolution::Local(LocalIntent::CloseEditor)
    );
}
