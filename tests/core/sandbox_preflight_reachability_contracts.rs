//! The host sandbox preflight must stay reachable from the launch paths (issue #713).
//!
//! `runtime::sandbox_preflight` is what asks the host whether the SSH agent
//! holds a key before a sandboxed agent starts. Its only production caller was
//! one line inside `app_input::preflight`, and a refactor deleted that line
//! while leaving the check, the `SshAgentNoIdentities` issue, the prompt copy
//! and the `ssh-add` remediation all in place. Nothing failed to compile,
//! nothing failed a test, and the loss stayed invisible for two weeks until the
//! host rebooted and every sandboxed git-over-SSH operation started returning
//! `Permission denied (publickey)`.
//!
//! Unit tests can prove the gate decides correctly, but they cannot prove the
//! launch paths still ask it. That is asserted in source here, because the call
//! that disappears is a single line that reads like leftover scaffolding, which
//! is exactly the shape of thing a large refactor drops.

use std::path::{Path, PathBuf};

/// The launch-path gate every launch route crosses.
const LAUNCH_GATE: &str = "src/app_input/preflight.rs";

/// The module that owns the check itself; a mention here proves nothing.
const CHECK_OWNER: &str = "src/runtime/preflight.rs";

/// The check whose reachability this contract protects.
const CHECK: &str = "sandbox_preflight";

#[test]
fn the_launch_gate_hands_the_host_check_to_its_preflight_decision() {
    let text = read_repo_text(LAUNCH_GATE);
    assert!(
        text.contains("launch_preflight_issue(signature, sandbox_preflight)"),
        "{LAUNCH_GATE} no longer hands `{CHECK}` to its launch decision. Every \
         launch path crosses this module, so dropping that argument silently \
         starts sandboxed agents against an empty forwarded SSH agent, and the \
         failure surfaces as `Permission denied (publickey)` inside the \
         container rather than as a prompt (issue #713)."
    );
}

#[test]
fn the_launch_gate_rechecks_the_host_after_running_a_remediation() {
    let text = read_repo_text(LAUNCH_GATE);
    assert!(
        text.contains("launch_preflight_issue(&signature, sandbox_preflight)"),
        "{LAUNCH_GATE} no longer re-runs its decision against `{CHECK}` after \
         the user confirms a remediation. Without the re-check a failed \
         `ssh-add` still resumes the launch, which is the state this prompt \
         exists to prevent (issue #713)."
    );
}

#[test]
fn the_host_sandbox_preflight_has_a_production_caller_outside_its_own_module() {
    let callers: Vec<String> = production_sources()
        .into_iter()
        .filter(|relative| relative != CHECK_OWNER)
        .filter(|relative| uses_check_symbol(&read_repo_text(relative)))
        .collect();

    assert!(
        !callers.is_empty(),
        "`{CHECK}` has no production use outside {CHECK_OWNER}. An unreachable \
         preflight check protects nothing: the prompt, the issue variant and \
         the ssh-add remediation all survive review while no launch ever runs \
         them (issue #713)."
    );
}

/// Whether this source uses the check as a value or a call, ignoring imports,
/// re-exports, comments, the definition itself, and longer identifiers that
/// merely start with the same text.
///
/// A re-export is not a use: `runtime::mod` naming the symbol in a `pub use`
/// block is how it stayed public while nothing called it.
fn uses_check_symbol(text: &str) -> bool {
    let mut inside_use_item = false;
    for line in text.lines() {
        let trimmed = line.trim_start();
        let opens_use_item = trimmed.starts_with("use ") || trimmed.starts_with("pub use ");
        if opens_use_item {
            inside_use_item = !trimmed.trim_end().ends_with(';');
            continue;
        }
        if inside_use_item {
            inside_use_item = !trimmed.trim_end().ends_with(';');
            continue;
        }
        if trimmed.starts_with("//") || trimmed.contains("pub fn sandbox_preflight") {
            continue;
        }
        if mentions_symbol(line, CHECK) {
            return true;
        }
    }
    false
}

/// Whether `line` contains `symbol` as a whole Rust identifier.
fn mentions_symbol(line: &str, symbol: &str) -> bool {
    let bytes = line.as_bytes();
    let mut offset = 0;
    while let Some(found) = line[offset..].find(symbol) {
        let start = offset + found;
        let end = start + symbol.len();
        let before_is_ident = start
            .checked_sub(1)
            .is_some_and(|index| is_ident_byte(bytes[index]));
        let after_is_ident = bytes.get(end).copied().is_some_and(is_ident_byte);
        if !before_is_ident && !after_is_ident {
            return true;
        }
        offset = end;
    }
    false
}

const fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Every non-test production source file under `src/`, repo-relative.
fn production_sources() -> Vec<String> {
    let mut found = Vec::new();
    collect_sources(&repo_path("src"), &mut found);
    found
}

fn collect_sources(directory: &Path, found: &mut Vec<String>) {
    let entries = std::fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("failed to list {}: {error}", directory.display()));
    for entry in entries {
        let path = entry
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", directory.display()))
            .path();
        if path.is_dir() {
            collect_sources(&path, found);
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !name.ends_with(".rs") || name.contains("_test") {
            continue;
        }
        let Ok(relative) = path.strip_prefix(repo_path("")) else {
            continue;
        };
        let Some(relative) = relative.to_str() else {
            continue;
        };
        found.push(relative.replace('\\', "/"));
    }
}

fn read_repo_text(relative: &str) -> String {
    let path = repo_path(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

fn repo_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}
