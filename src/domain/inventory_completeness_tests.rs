//! RED-first bidirectional completeness tests for the generated inventory
//! golden (issue #383 S8, CW03-01).
//!
//! Direction 1: every generated inventory row must name a handler that the
//! production dispatch route can execute — no orphan row.
//! Direction 2: every `HandlerKey` reachable from the production dispatch
//! sources must appear in the generated inventory — no orphan handler.

use std::collections::BTreeSet;

use super::inventory_completeness::{
    CompletenessReport, InventoryGoldenRow, dispatchable_handlers, generated_golden,
    inventory_completeness,
};

fn report() -> CompletenessReport {
    let result = inventory_completeness();
    let Ok(report) = result else {
        panic!("completeness projection must compile, got {result:?}");
    };
    report
}

fn golden() -> Vec<InventoryGoldenRow> {
    let result = generated_golden();
    let Ok(rows) = result else {
        panic!("generated golden must compile, got {result:?}");
    };
    rows
}

#[test]
fn generated_golden_is_complete_and_deterministic() {
    let rows = golden();
    assert!(!rows.is_empty(), "the golden must not be empty");

    let mut sorted = rows.clone();
    sorted.sort_by(|left, right| {
        (
            left.context.as_str(),
            left.chord_text.as_str(),
            left.action.as_str(),
        )
            .cmp(&(
                right.context.as_str(),
                right.chord_text.as_str(),
                right.action.as_str(),
            ))
    });
    assert_eq!(rows, sorted, "golden rows must be emitted in stable order");

    let second = golden();
    assert_eq!(rows, second, "the projection must be deterministic");
}

#[test]
fn every_generated_row_names_a_dispatchable_handler() {
    let report = report();
    assert!(
        report.rows_without_dispatch.is_empty(),
        "generated rows have no production dispatch path: {:?}",
        report.rows_without_dispatch
    );
}

#[test]
fn every_dispatchable_handler_appears_in_the_generated_inventory() {
    let report = report();
    assert!(
        report.handlers_without_row.is_empty(),
        "production handlers are absent from the generated inventory: {:?}",
        report.handlers_without_row
    );
}

#[test]
fn completeness_is_bidirectional_over_the_same_handler_set() {
    let report = report();
    let golden_handlers: BTreeSet<String> = golden()
        .iter()
        .map(|row| row.handler_name.clone())
        .collect();
    let dispatchable: BTreeSet<String> = dispatchable_handlers()
        .iter()
        .map(ToString::to_string)
        .collect();

    assert_eq!(
        golden_handlers, dispatchable,
        "the generated golden and production dispatch must describe one handler set"
    );
    assert_eq!(report.handler_count, dispatchable.len());
}

#[test]
fn every_golden_row_carries_its_context_chord_action_and_handler() {
    for row in golden() {
        assert!(!row.context.as_str().is_empty());
        assert!(!row.chord_text.is_empty());
        assert!(!row.action.as_str().is_empty());
        assert!(!row.handler_name.is_empty());
        assert_eq!(
            row.chord_text,
            row.chord.to_string(),
            "chord text must be the canonical formatting of the chord"
        );
    }
}
