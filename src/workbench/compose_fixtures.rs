//! Definition-file fixtures shared by the lowering and composition tests
//! (issue #385).

use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::domain::Id;
use crate::persistence::screen_files::{ScreenFileCandidate, ScreenFileRejection};

/// A complete, valid `local.review` definition.
///
/// It lives beside this file as real TOML rather than as a string literal, so
/// the same bytes can be embedded here and written to disk by the startup
/// tests, and so an author can read it as the worked example it is.
pub const REVIEW_DEFINITION: &str = include_str!("testdata/local-review.screen.toml");

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
