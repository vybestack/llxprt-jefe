//! The Unix tmux socket path must be unreachable on Windows (#547, V7).
//!
//! `src/runtime/socket.rs` resolves a Unix-domain socket for tmux's `-S` flag.
//! Every part of it is Unix-shaped: the `sun_path` length guard and the
//! `XDG_RUNTIME_DIR` precedence. It also used to shell out to `id -u` to name
//! the socket after the user; that is gone, because #547 re-keyed the socket on
//! the installation identity, but the ban below keeps it from coming back.
//!
//! None of that means anything on Windows, where isolation is a `-L <namespace>`
//! server name and `id` is not a program that exists. The module nevertheless
//! compiled into the Windows binary, and `MultiplexerPlan::resolved()` carried a
//! live `LocalPlatform::Unix` arm calling into it. That arm is unreachable today
//! only because `LocalPlatform::current()` never returns `Unix` on Windows --
//! a runtime accident, not a guarantee. Anyone adding a platform override, a
//! test seam, or a WSL path would silently reach a Unix-only code path on a
//! machine that cannot honour it.
//!
//! The fix is to let the compiler enforce it: gate the module on `cfg(unix)` so
//! the Windows build cannot name it at all. This contract keeps the gate from
//! being quietly removed, and keeps `id -u` from reappearing somewhere that is
//! not Unix-only.

use std::path::{Path, PathBuf};

#[test]
fn the_unix_socket_module_is_gated_out_of_the_windows_build() {
    let source = read_source("src/runtime/mod.rs");

    let declaration = locate(&source, "mod socket;")
        .unwrap_or_else(|| panic!("src/runtime/mod.rs no longer declares `mod socket;`"));
    assert!(
        is_unix_gated(&source, declaration),
        "src/runtime/mod.rs declares `mod socket;` without a `#[cfg(unix)]` gate, so the \
         Unix-domain socket resolver -- including its `id -u` subprocess -- compiles into \
         the Windows binary where none of it is meaningful. Gate the declaration."
    );

    let re_export =
        locate(&source, "pub use socket::jefe_tmux_socket_path;").unwrap_or_else(|| {
            panic!("src/runtime/mod.rs no longer re-exports `jefe_tmux_socket_path`")
        });
    assert!(
        is_unix_gated(&source, re_export),
        "src/runtime/mod.rs re-exports `jefe_tmux_socket_path` without a `#[cfg(unix)]` gate. \
         An ungated re-export of a gated module does not compile on Windows; gate both together."
    );
}

#[test]
fn nothing_outside_a_unix_gate_names_the_socket_path() {
    let mut offenders = Vec::new();
    let mut sightings = 0_usize;

    for file in rust_files(&repo_root().join("src")) {
        // The module is allowed to define and document its own function.
        if display_path(&file).ends_with("src/runtime/socket.rs") {
            continue;
        }
        let source = read_source_at(&file);
        for (index, line) in source.iter().enumerate() {
            if !mentions_in_code(line, "jefe_tmux_socket_path") {
                continue;
            }
            sightings += 1;
            if !is_unix_gated(&source, index) {
                offenders.push(format!("{} line {}", display_path(&file), index + 1));
            }
        }
    }

    assert!(
        sightings > 0,
        "no reference to `jefe_tmux_socket_path` was found anywhere, so this contract is \
         scanning the wrong tree and would pass vacuously"
    );
    assert!(
        offenders.is_empty(),
        "these call sites reach the Unix socket resolver from code that also compiles on \
         Windows:\n  {}\n\nMove each behind a `#[cfg(unix)]` item. For the multiplexer this \
         means splitting `current_isolation()` into `#[cfg(unix)]` and `#[cfg(windows)]` \
         versions rather than matching on `LocalPlatform` at runtime, so the Windows build \
         never names the Unix path.",
        offenders.join("\n  ")
    );
}

/// Nothing may identify this instance by the operating-system user.
///
/// The Unix socket used to be named `jefe-<uid>.sock`, which meant one server
/// per account and therefore one shared server for every worktree an operator
/// had open -- the same collision issue #547 reports on Windows. Issue #547
/// re-keyed it on the installation identity, which deletes the need for the uid
/// entirely: different accounts already have different home directories, so
/// they already derive different identities.
///
/// This ban is repo-wide with no exemption. The `id -u` shell-out it names is
/// gone; the test exists to keep it gone.
#[test]
fn nothing_identifies_the_installation_by_operating_system_user() {
    let mut offenders = Vec::new();

    for file in rust_files(&repo_root().join("src")) {
        let shown = display_path(&file);
        let source = read_source_at(&file);
        for (index, line) in source.iter().enumerate() {
            if mentions_in_code(line, "Command::new(\"id\")") {
                offenders.push(format!("{shown} line {}", index + 1));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "the operating-system user id is being read to identify this instance:\n  {}\n\nA jefe \
         instance is identified by the config/state location it was launched from, not by who \
         is running it (issue #547). Two worktrees under one account are two installations and \
         must not share a server; one installation reached after a rename is still the same \
         installation and must keep its sessions. Keying on the user gets both backwards, and \
         `id` does not exist on Windows anyway.",
        offenders.join("\n  ")
    );
}

/// Index of the first line whose code (not comment) contains `needle`.
fn locate(source: &[String], needle: &str) -> Option<usize> {
    source
        .iter()
        .position(|line| mentions_in_code(line, needle))
}

/// Whether `line` contains `needle` as code rather than prose. Doc comments in
/// this tree routinely name the very items being gated, and flagging those would
/// make the contract unmaintainable.
fn mentions_in_code(line: &str, needle: &str) -> bool {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//") {
        return false;
    }
    line.contains(needle)
}

/// Whether the item at `index` sits under a `cfg(unix)` attribute.
///
/// Walks upward past blank lines, comments and other attributes, because an item
/// is commonly written as a doc comment, then `#[cfg(unix)]`, then the code. Any
/// `cfg(unix)` found in that attribute run counts, which tolerates
/// `#[cfg(all(unix, ...))]` without needing to parse it.
fn is_unix_gated(source: &[String], index: usize) -> bool {
    for line in source[..index].iter().rev() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }
        if trimmed.starts_with("#[") || trimmed.starts_with("#![") {
            if trimmed.contains("cfg(unix)") || trimmed.contains("cfg(all(unix") {
                return true;
            }
            continue;
        }
        // Reached ordinary code: the item carries no gate of its own. It may
        // still live inside a gated block, so accept an enclosing gate found on
        // a less-indented line above.
        break;
    }
    enclosing_gate(source, index)
}

/// Whether the item at `index` is nested inside a `#[cfg(unix)] mod`/`fn` block.
///
/// Indentation is a good enough proxy for nesting here and avoids pulling a
/// parser into a source-scan contract: find the nearest strictly-less-indented
/// line above and ask whether *it* is gated.
fn enclosing_gate(source: &[String], index: usize) -> bool {
    let own_indent = indent_of(&source[index]);
    if own_indent == 0 {
        return false;
    }
    for (offset, line) in source[..index].iter().enumerate().rev() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with('#') {
            continue;
        }
        if indent_of(line) < own_indent {
            return is_unix_gated(source, offset);
        }
    }
    false
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

fn read_source(relative: &str) -> Vec<String> {
    read_source_at(&repo_root().join(relative))
}

fn read_source_at(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("{} should be readable: {error}", path.display()))
        .lines()
        .map(str::to_owned)
        .collect()
}

fn rust_files(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    collect(dir, &mut found);
    found
}

fn collect(dir: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, found);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            found.push(path);
        }
    }
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn display_path(path: &Path) -> String {
    path.strip_prefix(repo_root())
        .unwrap_or(path)
        .display()
        .to_string()
        .replace('\\', "/")
}
