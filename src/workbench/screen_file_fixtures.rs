//! Text fixtures shared by the screen-syntax parser tests (issue #385).

use super::screen_file::{ScreenFile, parse_screen_file};
use super::screen_file_bounds::ScreenSyntaxReason;

/// Header shared by every fixture.
pub const HEADER: &str = r#"screen_schema = 1
id = "local.review"
title = "Review"
route = "review"
initial_focus = "pr-list"
focus_order = ["pr-list", "pr-detail"]
"#;

/// Two panels, one output port and one matching input port.
pub const PANELS: &str = r#"
[[panels]]
id = "pr-list"
type = "pull-request-list"
focusable = true
required = true

[[panels.ports]]
id = "selection"
direction = "output"
type_id = "github.pull-request@1"
required = false
retained = false

[[panels]]
id = "pr-detail"
type = "pull-request-detail"
focusable = true
required = false

[[panels.ports]]
id = "subject"
direction = "input"
type_id = "github.pull-request@1"
required = false
retained = true
"#;

/// A horizontal split of the two panels.
pub const LAYOUT: &str = r#"
[layout]
type = "split"
axis = "horizontal"

[[layout.children]]
min = 20
collapsible = false
size = { weight = 1 }
node = { type = "leaf", panel = "pr-list" }

[[layout.children]]
min = 20
collapsible = true
collapse_priority = 0
size = { weight = 1 }
node = { type = "leaf", panel = "pr-detail" }
"#;

pub const RELATIONSHIP: &str = r#"
[[relationships]]
kind = "master-detail"
source = "pr-list.selection"
target = "pr-detail.subject"
activation = "immediate"
empty = "retain"
"#;

pub const BINDING: &str = r#"
[[bindings]]
context = "pull-requests"
action = "activate-detail"
"#;

pub fn valid_text() -> String {
    format!("{HEADER}{PANELS}{LAYOUT}{RELATIONSHIP}{BINDING}")
}

pub fn parsed(text: &str) -> ScreenFile {
    parse_screen_file(text)
        .unwrap_or_else(|error| unreachable!("fixture must parse: {error} ({error:?})"))
}

pub fn rejected(text: &str) -> ScreenSyntaxReason {
    match parse_screen_file(text) {
        Ok(_) => unreachable!("fixture must be rejected"),
        Err(error) => error.reason,
    }
}

/// A minimal single-panel screen whose body can be extended.
pub fn single_panel_text(extra_panels: &str, layout: &str) -> String {
    format!(
        r#"screen_schema = 1
id = "local.review"
title = "Review"
route = "review"
initial_focus = "pr-list"
focus_order = ["pr-list"]

[[panels]]
id = "pr-list"
type = "pull-request-list"
focusable = true
required = true
{extra_panels}
{layout}
"#
    )
}

/// A leaf layout naming one panel.
pub fn leaf_layout(panel: &str) -> String {
    format!("[layout]\ntype = \"leaf\"\npanel = \"{panel}\"\n")
}
