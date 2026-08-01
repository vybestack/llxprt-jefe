//! Discovery type, name, symlink, order, and bound matrix (issue #385,
//! CW05-01).

use std::path::{Path, PathBuf};

use super::diagnostic::FILE_LIMIT;
use super::screen_files::{ScreenFileCandidate, ScreenFileRejection, discover};

/// A temporary definitions directory that removes itself.
struct Definitions {
    root: PathBuf,
}

impl Definitions {
    fn new(label: &str) -> Self {
        let unique = format!(
            "jefe-screen-files-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        );
        let root = std::env::temp_dir().join(unique);
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root)
            .unwrap_or_else(|error| unreachable!("fixture root must exist: {error}"));
        Self { root }
    }

    fn path(&self) -> &Path {
        &self.root
    }

    fn write(&self, name: &str, contents: &str) {
        std::fs::write(self.root.join(name), contents)
            .unwrap_or_else(|error| unreachable!("fixture file must be written: {error}"));
    }

    fn mkdir(&self, name: &str) {
        std::fs::create_dir_all(self.root.join(name))
            .unwrap_or_else(|error| unreachable!("fixture directory must exist: {error}"));
    }

    fn members(&self) -> Vec<String> {
        discovered(self.path())
            .into_iter()
            .map(|candidate| candidate.member)
            .collect()
    }
}

impl Drop for Definitions {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn discovered(root: &Path) -> Vec<ScreenFileCandidate> {
    discover(root).unwrap_or_else(|error| unreachable!("discovery must succeed: {error}"))
}

#[test]
fn a_missing_definitions_directory_yields_no_candidates() {
    let absent = std::env::temp_dir().join("jefe-screen-files-definitely-absent-directory");
    let _ = std::fs::remove_dir_all(&absent);

    assert_eq!(discovered(&absent), Vec::new());
}

#[test]
fn an_empty_definitions_directory_yields_no_candidates() {
    let definitions = Definitions::new("empty");

    assert_eq!(definitions.members(), Vec::<String>::new());
}

#[test]
fn an_exactly_named_direct_regular_file_is_a_candidate() {
    let definitions = Definitions::new("accepted");
    definitions.write("review.screen.toml", "screen_schema = 1\n");

    let candidates = discovered(definitions.path());

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].member, "review");
    assert_eq!(candidates[0].text, Ok("screen_schema = 1\n".to_owned()));
    assert_eq!(
        candidates[0].path,
        definitions.path().join("review.screen.toml")
    );
}

#[test]
fn an_extension_alias_is_not_a_candidate() {
    let definitions = Definitions::new("alias");
    for name in [
        "review.screen.tml",
        "review.screen.toml.bak",
        "review.toml",
        "review.screen",
        "review.screen.tomlx",
        "screen.toml",
        "review.SCREEN.TOML",
        "review.Screen.Toml",
    ] {
        definitions.write(name, "screen_schema = 1\n");
    }

    assert_eq!(definitions.members(), Vec::<String>::new());
}

#[test]
fn a_hidden_file_is_not_a_candidate() {
    let definitions = Definitions::new("hidden");
    definitions.write(".review.screen.toml", "screen_schema = 1\n");
    definitions.write(".screen.toml", "screen_schema = 1\n");

    assert_eq!(definitions.members(), Vec::<String>::new());
}

#[test]
fn a_name_outside_the_member_grammar_is_not_a_candidate() {
    let definitions = Definitions::new("grammar");
    for name in [
        "Review.screen.toml",
        "9review.screen.toml",
        "-review.screen.toml",
        "re_view.screen.toml",
        "re.view.screen.toml",
        "re view.screen.toml",
    ] {
        definitions.write(name, "screen_schema = 1\n");
    }

    assert_eq!(definitions.members(), Vec::<String>::new());
}

#[test]
fn a_member_at_the_length_limit_is_a_candidate_and_one_over_is_not() {
    let definitions = Definitions::new("member-length");
    let at_limit = format!("a{}", "b".repeat(62));
    let over_limit = format!("a{}", "b".repeat(63));
    definitions.write(&format!("{at_limit}.screen.toml"), "screen_schema = 1\n");
    definitions.write(&format!("{over_limit}.screen.toml"), "screen_schema = 1\n");

    assert_eq!(definitions.members(), vec![at_limit]);
}

#[test]
fn a_directory_named_like_a_definition_is_not_a_candidate() {
    let definitions = Definitions::new("directory");
    definitions.mkdir("review.screen.toml");

    assert_eq!(definitions.members(), Vec::<String>::new());
}

#[test]
fn a_nested_definition_is_not_a_candidate() {
    let definitions = Definitions::new("nested");
    definitions.mkdir("nested");
    definitions.write("nested/review.screen.toml", "screen_schema = 1\n");

    assert_eq!(definitions.members(), Vec::<String>::new());
}

#[cfg(unix)]
#[test]
fn a_symbolic_link_is_not_a_candidate_even_when_it_points_at_a_regular_file() {
    let definitions = Definitions::new("symlink");
    definitions.write("source.txt", "screen_schema = 1\n");
    definitions.mkdir("elsewhere");
    std::os::unix::fs::symlink(
        definitions.path().join("source.txt"),
        definitions.path().join("linked.screen.toml"),
    )
    .unwrap_or_else(|error| unreachable!("fixture symlink must be created: {error}"));
    std::os::unix::fs::symlink(
        definitions.path().join("elsewhere"),
        definitions.path().join("dir-link.screen.toml"),
    )
    .unwrap_or_else(|error| unreachable!("fixture symlink must be created: {error}"));

    assert_eq!(definitions.members(), Vec::<String>::new());
}

#[test]
fn candidates_are_ordered_by_canonical_path_bytes() {
    let definitions = Definitions::new("order");
    for member in ["zulu", "alpha", "mike", "a0", "a-0"] {
        definitions.write(&format!("{member}.screen.toml"), "screen_schema = 1\n");
    }

    // '-' (0x2d) sorts before '0' (0x30), which sorts before any letter.
    assert_eq!(
        definitions.members(),
        vec!["a-0", "a0", "alpha", "mike", "zulu"]
    );
}

#[test]
fn a_file_at_the_size_limit_is_read_and_one_byte_over_is_rejected() {
    let definitions = Definitions::new("size");
    definitions.write("small.screen.toml", &"a".repeat(FILE_LIMIT));
    definitions.write("large.screen.toml", &"a".repeat(FILE_LIMIT + 1));

    let candidates = discovered(definitions.path());

    assert_eq!(candidates.len(), 2);
    assert_eq!(candidates[0].member, "large");
    assert_eq!(
        candidates[0].text,
        Err(ScreenFileRejection::TooLarge {
            bytes: FILE_LIMIT as u64 + 1
        })
    );
    assert_eq!(candidates[1].member, "small");
    assert_eq!(candidates[1].text.as_ref().map(String::len), Ok(FILE_LIMIT));
}

#[test]
fn a_file_that_is_not_utf8_is_reported_rather_than_silently_dropped() {
    let definitions = Definitions::new("utf8");
    std::fs::write(
        definitions.path().join("binary.screen.toml"),
        [0x73_u8, 0x3d, 0xff, 0xfe],
    )
    .unwrap_or_else(|error| unreachable!("fixture file must be written: {error}"));

    let candidates = discovered(definitions.path());

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].member, "binary");
    assert_eq!(candidates[0].text, Err(ScreenFileRejection::NotUtf8));
}

#[test]
fn discovery_of_the_same_directory_is_repeatable() {
    let definitions = Definitions::new("repeatable");
    for member in ["one", "two", "three"] {
        definitions.write(&format!("{member}.screen.toml"), "screen_schema = 1\n");
    }

    assert_eq!(
        discovered(definitions.path()),
        discovered(definitions.path())
    );
}
