//! Behavioral tests for the registry-editor sparse settings leaves (issue #388).
//!
//! Each editor writes exactly one shape of syntax and nothing else. These are
//! the byte-level goldens that say what "sparse" means for each of them.
//!
//! @requirement CW08-01
//! @requirement CW08-02
//! @requirement CW08-03
//! @requirement CW08-07

use std::num::NonZeroU16;

use crate::domain::action_registry::ActionId;
use crate::domain::input_context::ContextId;
use crate::domain::keymap::Chord;
use crate::domain::sha256::Sha256;
use crate::domain::{Id, OwnerCatalog};
use crate::workbench::descriptor::{Axis, LayoutChild, LayoutNode, Size};
use crate::workbench::ids::PanelId;

use super::diagnostic::{CfgCode, Diagnostic};
use super::migration::migrate_settings;
use super::settings_edit::{SettingsCandidate, SettingsEdit, SyntaxPath};
use super::writer::ExpectedHash;

fn catalog() -> OwnerCatalog {
    crate::config_owners::builtin_owner_catalog()
        .unwrap_or_else(|error| panic!("owner catalog fixture: {error}"))
}

fn id(value: &str) -> Id {
    Id::parse(value).unwrap_or_else(|error| panic!("id fixture {value}: {error}"))
}

fn context(value: &str) -> ContextId {
    ContextId::parse(value).unwrap_or_else(|error| panic!("context fixture {value}: {error}"))
}

fn action(value: &str) -> ActionId {
    ActionId::parse(value).unwrap_or_else(|error| panic!("action fixture {value}: {error}"))
}

fn chord(value: &str) -> Chord {
    Chord::parse(value).unwrap_or_else(|error| panic!("chord fixture {value}: {error}"))
}

fn panel(value: &'static str) -> PanelId {
    PanelId::parse(value).unwrap_or_else(|error| panic!("panel fixture {value}: {error}"))
}

fn weight(value: u16) -> Size {
    Size::Weight(NonZeroU16::new(value).unwrap_or(NonZeroU16::MIN))
}

/// Build a candidate over `source`, applying `edits` in order.
fn candidate(source: &[u8], edits: &[SettingsEdit]) -> SettingsCandidate {
    let catalog = catalog();
    let migration = migrate_settings(source, &catalog)
        .unwrap_or_else(|diagnostics| panic!("settings fixture must load: {diagnostics:?}"));
    SettingsCandidate::from_edits(
        &migration,
        &catalog,
        edits,
        ExpectedHash::Present(Sha256::digest(source)),
    )
    .unwrap_or_else(|diagnostics| panic!("valid candidate must compose: {diagnostics:?}"))
}

/// The sorted diagnostics that block a candidate over `source`.
fn refused(source: &[u8], edits: &[SettingsEdit]) -> Vec<Diagnostic> {
    let catalog = catalog();
    let migration = migrate_settings(source, &catalog)
        .unwrap_or_else(|diagnostics| panic!("settings fixture must load: {diagnostics:?}"));
    match SettingsCandidate::from_edits(
        &migration,
        &catalog,
        edits,
        ExpectedHash::Present(Sha256::digest(source)),
    ) {
        Ok(_) => panic!("this candidate must be refused"),
        Err(diagnostics) => diagnostics,
    }
}

fn rendered(candidate: &SettingsCandidate) -> String {
    String::from_utf8_lossy(candidate.bytes()).into_owned()
}

// ── CW08-01: agent enablement writes only the sparse enabled path ──────────

#[test]
fn an_agent_toggle_replaces_only_its_own_enabled_value() {
    let source = br#"# keep me
settings_schema = 2

[appearance]
theme = 'green-screen'

[agents."core.llxprt"]
enabled = false
repository_defaults = { model = "kept" }

[extensions.future]
opaque = "retained"
"#;

    let candidate = candidate(
        source,
        &[SettingsEdit::AgentEnabled {
            agent: id("core.llxprt"),
            enabled: true,
        }],
    );

    assert_eq!(
        rendered(&candidate),
        r#"# keep me
settings_schema = 2

[appearance]
theme = 'green-screen'

[agents."core.llxprt"]
enabled = true
repository_defaults = { model = "kept" }

[extensions.future]
opaque = "retained"
"#
    );
}

#[test]
fn an_agent_toggle_creates_only_its_own_table_when_the_agent_is_unmentioned() {
    let source = b"settings_schema = 2\n";

    let candidate = candidate(
        source,
        &[SettingsEdit::AgentEnabled {
            agent: id("core.llxprt"),
            enabled: false,
        }],
    );

    assert_eq!(
        rendered(&candidate),
        "settings_schema = 2\n[agents.\"core.llxprt\"]\nenabled = false\n"
    );
    assert_eq!(
        candidate
            .published()
            .agents
            .get(&id("core.llxprt"))
            .and_then(|owner| owner.enabled),
        Some(false)
    );
}

#[test]
fn resetting_an_agent_removes_only_its_enabled_assignment() {
    let source = br#"settings_schema = 2
[agents."core.llxprt"]
enabled = false
repository_defaults = { model = "kept" }
"#;

    let candidate = candidate(
        source,
        &[SettingsEdit::Reset(SyntaxPath::AgentEnabled(id(
            "core.llxprt",
        )))],
    );

    assert_eq!(
        rendered(&candidate),
        "settings_schema = 2\n[agents.\"core.llxprt\"]\nrepository_defaults = { model = \"kept\" }\n"
    );
    assert_eq!(
        candidate
            .published()
            .agents
            .get(&id("core.llxprt"))
            .and_then(|owner| owner.enabled),
        None,
        "a reset agent inherits its compiled default"
    );
}

// ── CW08-02: screen membership and order write replacement arrays ──────────

#[test]
fn enabling_screens_writes_one_replacement_array_and_leaves_order_alone() {
    let source = br#"settings_schema = 2
[workbench]
enabled_screens = ["local.alpha"]
screen_order = ["core.dashboard", "local.alpha"]
"#;

    let candidate = candidate(
        source,
        &[SettingsEdit::EnabledScreens(vec![
            id("local.alpha"),
            id("local.beta"),
        ])],
    );

    assert_eq!(
        rendered(&candidate),
        "settings_schema = 2\n[workbench]\nenabled_screens = [\"local.alpha\", \"local.beta\"]\nscreen_order = [\"core.dashboard\", \"local.alpha\"]\n"
    );
}

#[test]
fn disabling_every_screen_writes_an_empty_array_rather_than_removing_the_key() {
    let source = br#"settings_schema = 2
[workbench]
enabled_screens = ["local.alpha"]
"#;

    let candidate = candidate(source, &[SettingsEdit::EnabledScreens(Vec::new())]);

    assert_eq!(
        rendered(&candidate),
        "settings_schema = 2\n[workbench]\nenabled_screens = []\n"
    );
    assert!(candidate.published().workbench.enabled_screens.is_empty());
}

#[test]
fn reordering_screens_writes_one_replacement_order_array() {
    let source = br#"settings_schema = 2
[workbench]
screen_order = ["core.dashboard", "core.issues", "core.errors"]
"#;

    let candidate = candidate(
        source,
        &[SettingsEdit::ScreenOrder(vec![
            id("core.issues"),
            id("core.dashboard"),
            id("core.errors"),
        ])],
    );

    assert_eq!(
        rendered(&candidate),
        "settings_schema = 2\n[workbench]\nscreen_order = [\"core.issues\", \"core.dashboard\", \"core.errors\"]\n"
    );
    assert_eq!(
        candidate.published().workbench.screen_order,
        vec![id("core.issues"), id("core.dashboard"), id("core.errors")]
    );
}

// ── CW08-03: a layout override writes, replaces, and removes one whole tree ─

#[test]
fn a_layout_override_writes_one_whole_tree_under_its_own_key() {
    let source = b"settings_schema = 2\n";

    let candidate = candidate(
        source,
        &[SettingsEdit::ReplaceLayout {
            screen: id("core.dashboard"),
            layout: Box::new(LayoutNode::Split {
                axis: Axis::Horizontal,
                gap: 0,
                children: vec![
                    LayoutChild {
                        node: LayoutNode::Leaf {
                            panel: panel("list"),
                        },
                        size: weight(1),
                        min: 10,
                        max: None,
                        collapsible: false,
                        collapse_priority: None,
                    },
                    LayoutChild {
                        node: LayoutNode::Leaf {
                            panel: panel("detail"),
                        },
                        size: weight(2),
                        min: 20,
                        max: Some(80),
                        collapsible: true,
                        collapse_priority: Some(-1),
                    },
                ],
            }),
        }],
    );

    assert_eq!(
        rendered(&candidate),
        concat!(
            "settings_schema = 2\n",
            "[workbench.layout_overrides]\n",
            "\"core.dashboard\" = { type = \"split\", axis = \"horizontal\", children = [",
            "{ node = { type = \"leaf\", panel = \"list\" }, size = { weight = 1 }, min = 10, collapsible = false }, ",
            "{ node = { type = \"leaf\", panel = \"detail\" }, size = { weight = 2 }, min = 20, max = 80, collapsible = true, collapse-priority = -1 }",
            "] }\n",
        )
    );
    assert!(
        candidate
            .published()
            .workbench
            .layout_overrides
            .contains_key(&id("core.dashboard")),
        "the override publishes under its own screen id"
    );
}

#[test]
fn a_layout_override_replaces_the_whole_previous_tree_and_no_neighbour() {
    let source = br#"settings_schema = 2
[workbench.layout_overrides]
"core.dashboard" = { type = "leaf", panel = "list" }
"core.issues" = { type = "leaf", panel = "list" }
"#;

    let candidate = candidate(
        source,
        &[SettingsEdit::ReplaceLayout {
            screen: id("core.dashboard"),
            layout: Box::new(LayoutNode::Leaf {
                panel: panel("detail"),
            }),
        }],
    );

    assert_eq!(
        rendered(&candidate),
        concat!(
            "settings_schema = 2\n",
            "[workbench.layout_overrides]\n",
            "\"core.dashboard\" = { type = \"leaf\", panel = \"detail\" }\n",
            "\"core.issues\" = { type = \"leaf\", panel = \"list\" }\n",
        )
    );
}

#[test]
fn a_layout_override_written_as_its_own_table_is_replaced_whole() {
    // The header form is the same tree written differently. Replacing it means
    // replacing the whole block, not appending a second definition of the same
    // key beside it.
    let source = br#"settings_schema = 2
[workbench]
screen_order = ["core.dashboard"]
[workbench.layout_overrides."core.dashboard"]
type = "leaf"
panel = "list"
"#;

    let candidate = candidate(
        source,
        &[SettingsEdit::ReplaceLayout {
            screen: id("core.dashboard"),
            layout: Box::new(LayoutNode::Leaf {
                panel: panel("detail"),
            }),
        }],
    );

    assert_eq!(
        rendered(&candidate),
        concat!(
            "settings_schema = 2\n",
            "[workbench]\n",
            "screen_order = [\"core.dashboard\"]\n",
            "[workbench.layout_overrides]\n",
            "\"core.dashboard\" = { type = \"leaf\", panel = \"detail\" }\n",
        )
    );
}

#[test]
fn resetting_a_layout_removes_the_whole_override_and_nothing_else() {
    let source = br#"settings_schema = 2
[workbench.layout_overrides]
"core.dashboard" = { type = "leaf", panel = "list" }
"core.issues" = { type = "leaf", panel = "list" }
"#;

    let candidate = candidate(
        source,
        &[SettingsEdit::Reset(SyntaxPath::LayoutOverride(id(
            "core.dashboard",
        )))],
    );

    assert_eq!(
        rendered(&candidate),
        concat!(
            "settings_schema = 2\n",
            "[workbench.layout_overrides]\n",
            "\"core.issues\" = { type = \"leaf\", panel = \"list\" }\n",
        )
    );
    assert!(
        !candidate
            .published()
            .workbench
            .layout_overrides
            .contains_key(&id("core.dashboard"))
    );
}

#[test]
fn resetting_a_layout_written_as_its_own_table_removes_the_whole_block() {
    let source = br#"settings_schema = 2
[workbench.layout_overrides."core.dashboard"]
type = "leaf"
panel = "list"
[appearance]
theme = "dracula"
"#;

    let candidate = candidate(
        source,
        &[SettingsEdit::Reset(SyntaxPath::LayoutOverride(id(
            "core.dashboard",
        )))],
    );

    assert_eq!(
        rendered(&candidate),
        "settings_schema = 2\n[appearance]\ntheme = \"dracula\"\n"
    );
}

// ── CW08-07: Unbind writes an empty array, Reset removes the syntax ────────

#[test]
fn setting_chords_writes_the_whole_binding_array() {
    let source = br#"settings_schema = 2
[keymap.global]
"core.open-settings" = [","]
"#;

    let candidate = candidate(
        source,
        &[SettingsEdit::Keymap {
            context: context("global"),
            action: action("core.open-settings"),
            chords: vec![chord("Ctrl+,"), chord("F2")],
        }],
    );

    assert_eq!(
        rendered(&candidate),
        "settings_schema = 2\n[keymap.global]\n\"core.open-settings\" = [\"Ctrl+,\", \"F2\"]\n"
    );
}

#[test]
fn unbinding_writes_an_empty_array_rather_than_removing_the_assignment() {
    let source = br#"settings_schema = 2
[keymap.global]
"core.open-settings" = [","]
"#;

    let candidate = candidate(
        source,
        &[SettingsEdit::Keymap {
            context: context("global"),
            action: action("core.open-settings"),
            chords: Vec::new(),
        }],
    );

    assert_eq!(
        rendered(&candidate),
        "settings_schema = 2\n[keymap.global]\n\"core.open-settings\" = []\n"
    );
    assert_eq!(
        candidate
            .published()
            .keymap
            .get("global")
            .and_then(|actions| actions.get("core.open-settings"))
            .map(Vec::len),
        Some(0),
        "an unbound action is present and empty, not absent"
    );
}

#[test]
fn resetting_a_binding_removes_the_assignment_so_the_compiled_chord_is_inherited() {
    let source = br#"settings_schema = 2
[keymap.global]
"core.open-settings" = [","]
"core.emergency-exit" = ["Ctrl+Q"]
"#;

    let candidate = candidate(
        source,
        &[SettingsEdit::Reset(SyntaxPath::Keymap {
            context: context("global"),
            action: action("core.open-settings"),
        })],
    );

    assert_eq!(
        rendered(&candidate),
        "settings_schema = 2\n[keymap.global]\n\"core.emergency-exit\" = [\"Ctrl+Q\"]\n"
    );
    assert!(
        candidate
            .published()
            .keymap
            .get("global")
            .is_none_or(|actions| !actions.contains_key("core.open-settings"))
    );
}

// ── shared: every registry leaf applies at the next start ─────────────────

#[test]
fn every_registry_leaf_is_structural() {
    for path in [
        SyntaxPath::EnabledScreens,
        SyntaxPath::ScreenOrder,
        SyntaxPath::AgentEnabled(id("core.llxprt")),
        SyntaxPath::LayoutOverride(id("core.dashboard")),
        SyntaxPath::Keymap {
            context: context("global"),
            action: action("core.open-settings"),
        },
    ] {
        assert!(
            path.structural(),
            "{path:?} composes a registry, so it applies at the next start"
        );
    }
}

#[test]
fn a_registry_leaf_inside_an_inline_table_is_refused_rather_than_silently_dropped() {
    let source = b"settings_schema = 2\nworkbench = { screen_order = [] }\n";

    let diagnostics = refused(
        source,
        &[SettingsEdit::ScreenOrder(vec![id("core.errors")])],
    );

    let Some(first) = diagnostics.first() else {
        panic!("a refusal must carry a diagnostic");
    };
    assert_eq!(first.code, CfgCode::E006);
}

#[test]
fn an_unedited_document_with_registry_syntax_round_trips_byte_unchanged() {
    let source = br#"settings_schema = 2

# every one of these is syntax this editor can write
[workbench]
enabled_screens = ["local.alpha"]
screen_order = ['core.dashboard', "local.alpha"]

[workbench.layout_overrides."core.dashboard"]
type = "leaf"
panel = "list"

[agents."core.llxprt"]
enabled = false

[keymap.global]
"core.emergency-exit" = ["Ctrl+Q"]

[extensions.future]
opaque = { bytes = "retained" }
"#;

    let candidate = candidate(source, &[]);

    assert_eq!(candidate.bytes(), source);
}

#[test]
fn replacing_a_header_form_layout_keeps_the_comments_around_it() {
    let source = br#"settings_schema = 2
[workbench.layout_overrides."core.dashboard"]
type = "leaf"
panel = "list"

# this comment introduces appearance, not the override above it
[appearance]
theme = "dracula"
"#;

    let candidate = candidate(
        source,
        &[SettingsEdit::ReplaceLayout {
            screen: id("core.dashboard"),
            layout: Box::new(LayoutNode::Leaf {
                panel: panel("detail"),
            }),
        }],
    );

    let rendered = rendered(&candidate);
    assert!(
        rendered.contains("# this comment introduces appearance, not the override above it"),
        "a comment outside the edited override survives it: {rendered}"
    );
    assert!(rendered.contains("theme = \"dracula\""), "{rendered}");
}

#[test]
fn resetting_a_header_form_layout_keeps_a_trailing_comment() {
    let source = br#"settings_schema = 2
[appearance]
theme = "dracula"
[workbench.layout_overrides."core.dashboard"]
type = "leaf"
panel = "list"

# a note somebody left at the end of the file
"#;

    let candidate = candidate(
        source,
        &[SettingsEdit::Reset(SyntaxPath::LayoutOverride(id(
            "core.dashboard",
        )))],
    );

    let rendered = rendered(&candidate);
    assert!(
        rendered.contains("# a note somebody left at the end of the file"),
        "a comment after the removed override survives it: {rendered}"
    );
}
