//! Definition-file fixtures shared by the lowering and composition tests
//! (issue #385).

use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::domain::Id;
use crate::persistence::screen_files::{ScreenFileCandidate, ScreenFileRejection};

/// A complete, valid `local.review` definition.
pub const REVIEW_DEFINITION: &str = r#"screen_schema = 1
id = "local.review"
title = "Review"
route = "review"
initial_focus = "pr-list"
focus_order = ["pr-list", "pr-detail"]

[[panels]]
id = "pr-list"
type = "pr-list"
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
type = "pr-detail"
focusable = true
required = false

[[panels.ports]]
id = "subject"
direction = "input"
type_id = "github.pull-request@1"
required = false
retained = true

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

[[relationships]]
kind = "master-detail"
source = "pr-list.selection"
target = "pr-detail.subject"
activation = "immediate"
empty = "retain"
"#;

/// A candidate holding the given text under `<root>/<member>.screen.toml`.
pub fn candidate(member: &str, text: &str) -> ScreenFileCandidate {
    ScreenFileCandidate {
        path: PathBuf::from("/definitions").join(format!("{member}.screen.toml")),
        member: member.to_owned(),
        text: Ok(text.to_owned()),
    }
}

/// A candidate whose bytes discovery refused.
pub fn unreadable_candidate(member: &str, rejection: ScreenFileRejection) -> ScreenFileCandidate {
    ScreenFileCandidate {
        path: PathBuf::from("/definitions").join(format!("{member}.screen.toml")),
        member: member.to_owned(),
        text: Err(rejection),
    }
}

/// The enabled-screens set naming the given members.
pub fn enabled(members: &[&str]) -> BTreeSet<Id> {
    members
        .iter()
        .map(|member| {
            Id::parse(&format!("local.{member}"))
                .unwrap_or_else(|error| unreachable!("fixture owner id must parse: {error}"))
        })
        .collect()
}
