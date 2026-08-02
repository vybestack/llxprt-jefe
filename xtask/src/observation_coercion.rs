//! Uncertain-observation coercion policy (jefe issue #541, V13).
//!
//! `Observed<T>` carries "I could not determine this" as a value so it cannot
//! be mistaken for an answer. The type deliberately has no `Default`, no
//! `unwrap_or`, and no `From<Option<T>>`, and `resolve` never shows the
//! uncertain case to the decision closure.
//!
//! But `known()` returns `Option<&T>`, and `Option` brings its own collapsing
//! combinators back with it. `observed.known().unwrap_or(&dead)` type-checks
//! and reads innocently, and it is exactly the bug this issue exists to remove:
//! a failure to determine, silently spent as a determination.
//!
//! Nine such collapses were found by hand. This policy fails the build on the
//! tenth rather than waiting for it to be noticed in production.

use std::path::{Path, PathBuf};

use crate::process::CommandFailed;

/// Accessors that hand back an `Option` wrapping an uncertain observation.
const UNCERTAIN_ACCESSORS: &[&str] = &["known", "transition", "held", "uncertainty"];

/// `Option` methods that turn "absent" into a usable value without the caller
/// ever naming the uncertainty.
const COLLAPSING: &[&str] = &[
    "unwrap_or",
    "unwrap_or_default",
    "unwrap_or_else",
    "map_or",
    "map_or_else",
    "is_some_and",
    "unwrap",
    "expect",
];

const SCAN_ROOTS: &[&str] = &["src"];

/// # Errors
///
/// Fails when a source line spends an uncertain observation as an answer,
/// listing each site so the offending line is named rather than merely
/// counted.
pub fn run_repo_check(root: &Path) -> Result<(), CommandFailed> {
    let mut offences: Vec<String> = Vec::new();

    for scan_root in SCAN_ROOTS {
        for file in rust_sources(&root.join(scan_root)) {
            let Ok(text) = std::fs::read_to_string(&file) else {
                continue;
            };
            let relative = file
                .strip_prefix(root)
                .unwrap_or(&file)
                .display()
                .to_string();
            // The policy's own documentation spells out the pattern it bans.
            if relative
                .replace('\\', "/")
                .ends_with("xtask/src/observation_coercion.rs")
            {
                continue;
            }
            for (number, line) in text.lines().enumerate() {
                if let Some(found) = coercion_on_line(line) {
                    offences.push(format!("{relative}:{}: {found}", number + 1));
                }
            }
        }
    }

    if offences.is_empty() {
        return Ok(());
    }

    let mut message = String::from("an uncertain observation was spent as if it were an answer:\n");
    for offence in &offences {
        message.push_str("  ");
        message.push_str(offence);
        message.push('\n');
    }
    message.push_str(
        "\nMatch the uncertain case explicitly and hold, or use `resolve`, which\n\
         cannot show the undetermined case to a decision.\n",
    );
    Err(failure(message))
}

/// Detect an uncertain accessor whose `Option` is immediately collapsed.
///
/// Deliberately literal: it looks for the two spellings adjacent on one line,
/// which is how every instance in this issue was actually written. A caller
/// determined to launder the value through a binding will not be caught, and
/// that is the accepted limit -- the policy exists to stop the easy mistake,
/// not to prove a negative.
fn coercion_on_line(line: &str) -> Option<String> {
    let code = line.split("//").next().unwrap_or(line);
    for accessor in UNCERTAIN_ACCESSORS {
        let needle = format!(".{accessor}()");
        let mut search_from = 0;
        while let Some(offset) = code[search_from..].find(&needle) {
            let after = &code[search_from + offset + needle.len()..];
            let trimmed = after.trim_start();
            for collapsing in COLLAPSING {
                let call = format!(".{collapsing}(");
                if trimmed.starts_with(&call) {
                    return Some(format!(".{accessor}(){call}...)"));
                }
            }
            search_from += offset + needle.len();
        }
    }
    None
}

fn rust_sources(directory: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(directory) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(rust_sources(&path));
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            found.push(path);
        }
    }
    found
}

fn failure(message: String) -> CommandFailed {
    CommandFailed {
        program: "xtask".into(),
        args: vec!["check".into(), "observation-coercion".into()],
        status: None,
        stdout: Vec::new(),
        stderr: message.into_bytes(),
    }
}
