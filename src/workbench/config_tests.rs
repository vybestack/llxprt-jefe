//! Panel configuration read/write tests (issue #384).

use crate::domain::{Id, TypedMap, TypedValue};

use super::config::{
    CHROME_BOTTOM, CHROME_LEFT, CHROME_RIGHT, CHROME_TOP, insets_config, panel_insets,
};
use super::geometry::Insets;

fn key(value: &str) -> Id {
    Id::parse(value).unwrap_or_else(|_| unreachable!("{value} is a valid configuration key"))
}

#[test]
fn insets_round_trip_through_the_configuration_bag() {
    let insets = Insets::new(3, 1, 2, 4);
    let Some(config) = insets_config(insets) else {
        unreachable!("the chrome keys are valid identifiers");
    };
    assert_eq!(panel_insets(&config), insets);
}

#[test]
fn a_zero_edge_is_omitted_rather_than_stored() {
    // Writing zeros would bloat every panel's bag with the default.
    let Some(config) = insets_config(Insets::new(0, 0, 1, 0)) else {
        unreachable!("the chrome keys are valid identifiers");
    };
    assert_eq!(config.len(), 1);
    assert_eq!(panel_insets(&config), Insets::new(0, 0, 1, 0));
}

#[test]
fn an_empty_bag_reads_as_no_chrome() {
    assert_eq!(panel_insets(&TypedMap::new()), Insets::default());
}

#[test]
fn a_missing_edge_reads_as_zero() {
    let mut config = TypedMap::new();
    config.insert(key(CHROME_TOP), TypedValue::Integer(2));
    assert_eq!(panel_insets(&config), Insets::new(2, 0, 0, 0));
}

#[test]
fn every_edge_is_read_from_its_own_key() {
    let mut config = TypedMap::new();
    config.insert(key(CHROME_TOP), TypedValue::Integer(1));
    config.insert(key(CHROME_BOTTOM), TypedValue::Integer(2));
    config.insert(key(CHROME_LEFT), TypedValue::Integer(3));
    config.insert(key(CHROME_RIGHT), TypedValue::Integer(4));
    assert_eq!(panel_insets(&config), Insets::new(1, 2, 3, 4));
}

#[test]
fn a_wrongly_typed_value_reads_as_zero() {
    // The bag is shared with panel-specific keys this module does not know, so
    // a wrong type is a default rather than a failure.
    let mut config = TypedMap::new();
    config.insert(key(CHROME_TOP), TypedValue::Bool(true));
    config.insert(key(CHROME_LEFT), TypedValue::String("2".to_owned()));
    assert_eq!(panel_insets(&config), Insets::default());
}

#[test]
fn a_negative_value_reads_as_zero() {
    let mut config = TypedMap::new();
    config.insert(key(CHROME_TOP), TypedValue::Integer(-4));
    assert_eq!(panel_insets(&config), Insets::default());
}

#[test]
fn a_value_beyond_a_cell_count_reads_as_zero() {
    let mut config = TypedMap::new();
    config.insert(key(CHROME_BOTTOM), TypedValue::Integer(100_000));
    assert_eq!(panel_insets(&config), Insets::default());
}

#[test]
fn unrelated_keys_do_not_disturb_the_chrome_read() {
    let mut config = TypedMap::new();
    config.insert(key(CHROME_TOP), TypedValue::Integer(2));
    config.insert(key("panel.title"), TypedValue::String("Issues".to_owned()));
    assert_eq!(panel_insets(&config), Insets::new(2, 0, 0, 0));
}

#[test]
fn the_widest_representable_chrome_round_trips() {
    let insets = Insets::new(u16::MAX, u16::MAX, u16::MAX, u16::MAX);
    let Some(config) = insets_config(insets) else {
        unreachable!("the chrome keys are valid identifiers");
    };
    assert_eq!(panel_insets(&config), insets);
}
