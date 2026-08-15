//! Tests must not be able to reach the developer's real multiplexer server.
//!
//! A test that runs `psmux new-session` without `-L` puts its session on the
//! *default* server -- the same one the developer's live agents are on. If it
//! then tears that server down, every one of those agents dies. There is no
//! error, nothing in the logs, and nothing obviously to blame: it presents as
//! agents dying spontaneously.
//!
//! This is not hypothetical. It happened (see #617), it cost hours of live
//! agent work, and the mistake was a single missing flag in a fixture that
//! drove `psmux` directly instead of going through `MultiplexerPlan`.
//!
//! The safe path already exists and is used by every psmux test in the tree:
//! build commands from a `MultiplexerPlan` carrying
//! `MultiplexerIsolation::Namespace`, which puts `-L <namespace>` on every
//! invocation. This contract makes that the only path, so the unsafe one --
//! which is one obvious line away -- cannot be written by accident.

use std::path::{Path, PathBuf};

/// Verbs that either create state on a server or destroy it. `new-session` is
/// included deliberately: landing on the default server is the actual defect,
/// and a test that creates there will usually clean up there too.
const MULTIPLEXER_VERBS: [&str; 4] = [
    "new-session",
    "kill-server",
    "kill-session",
    "attach-session",
];

/// Evidence that the file does not merely mention a verb but actually runs it.
const SPAWN_MARKERS: [&str; 3] = [".status()", ".output()", ".spawn()"];

/// Evidence that commands are scoped to a private server. `MultiplexerPlan`
/// injects `-L`/`-S` itself, so naming the plan also counts.
const SCOPING_MARKERS: [&str; 5] = [
    "\"-L\"",
    "\"-S\"",
    "MultiplexerPlan",
    "plan.command",
    "base_args",
];

#[test]
fn tests_never_drive_the_multiplexer_without_scoping_it_to_a_private_server() {
    let mut offenders = Vec::new();
    let regions = test_regions();

    // A contract that scans nothing passes for the wrong reason. If the layout
    // moves under it, this fires instead of quietly blessing the whole tree.
    assert!(
        regions.len() > 50,
        "only {} test regions were scanned, which is too few to be real -- the \
         scanner has lost track of the source layout and would pass vacuously",
        regions.len()
    );
    assert!(
        regions
            .iter()
            .any(|region| region.origin.contains("psmux_attach.rs")),
        "the known psmux tests were not scanned, so this contract is not \
         looking where the hazard lives"
    );

    for region in regions {
        let TestRegion { origin, text } = region;
        if !text.lines().any(is_verb_line) {
            continue;
        }
        // A verb table or an assertion over argument strings is harmless; only
        // a file that actually launches something can reach a live server.
        if !SPAWN_MARKERS.iter().any(|marker| text.contains(marker)) {
            continue;
        }
        if SCOPING_MARKERS.iter().any(|marker| text.contains(marker)) {
            continue;
        }
        let line = text
            .lines()
            .position(is_verb_line)
            .map_or(0, |index| index + 1);
        offenders.push(format!("{origin} (line {line} of the scanned region)"));
    }

    assert!(
        offenders.is_empty(),
        "these tests run multiplexer commands without scoping them to a private \
         server, so they can reach the developer's live agent sessions and kill \
         them:\n  {}\n\nBuild commands from a MultiplexerPlan carrying \
         MultiplexerIsolation::Namespace(unique_namespace()); it puts -L on every \
         invocation. See tests/psmux_attach.rs for the pattern, and #617 for what \
         happens without it.",
        offenders.join("\n  ")
    );
}

fn is_verb_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//") || trimmed.starts_with("///") {
        return false;
    }
    MULTIPLEXER_VERBS
        .iter()
        .any(|verb| line.contains(&format!("\"{verb}\"")))
}

/// Non-ASCII source used to mask a cursor bug here once: the scan advanced by
/// the block's length instead of its end offset, so the next search started at
/// an arbitrary byte. CRLF checkouts happened to keep landing on boundaries, so
/// it only surfaced on Linux. This pins the arithmetic directly.
#[test]
fn scanning_survives_non_ascii_source_and_repeated_blocks() {
    let path = std::env::temp_dir().join(format!(
        "jefe-scan-probe-{}-{}.rs",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |value| value.as_nanos())
    ));
    // A single fixture is not enough: the panic needs the miscomputed index to
    // land *inside* a multi-byte character, which depends on exact byte
    // offsets. Sweeping the padding walks the cursor across the block until it
    // does, so the bug cannot hide behind a lucky alignment.
    for padding in 0..48_usize {
        let filler = "é".repeat(padding);
        // The multi-byte run has to sit *inside* the block: the miscomputed
        // cursor lands short of the block's true end, so that is the only place
        // it can fall mid-character. LF endings throughout, because CRLF shifts
        // every offset and is exactly what let this pass on Windows.
        let source = format!(
            "#[cfg(test)]\nmod first {{ fn a() {{ let _ = \"{filler}\"; }} }}\n\
             #[cfg(test)]\nmod second {{ fn b() {{}} }}\n"
        );
        if std::fs::write(&path, &source).is_err() {
            return;
        }

        let mut regions = Vec::new();
        push_cfg_test_blocks(&path, &mut regions);

        assert_eq!(
            regions.len(),
            2,
            "both #[cfg(test)] blocks should be found at padding {padding}; got {:?}",
            regions
                .iter()
                .map(|region| &region.text)
                .collect::<Vec<_>>()
        );
        assert!(
            regions[0].text.contains("fn a") && regions[1].text.contains("fn b"),
            "each region should hold its own block at padding {padding}, not a shifted slice"
        );
    }

    let _ = std::fs::remove_file(&path);
}

/// A stretch of source compiled only under `cfg(test)`, with a label good
/// enough to find it by hand.
struct TestRegion {
    origin: String,
    text: String,
}

/// Everything that only exists when testing: the whole `tests/` tree, in-crate
/// files named `*_tests.rs`, and -- because a test can just as easily be written
/// inline -- each `#[cfg(test)]` block inside an ordinary source file.
///
/// Ordinary source is deliberately scanned *only* inside those blocks.
/// Production code legitimately drives the multiplexer (`multiplexer.rs` builds
/// the very commands this contract asks for), so scanning it whole would flag
/// the implementation for implementing the thing.
fn test_regions() -> Vec<TestRegion> {
    let root = repo_root();
    let mut regions = Vec::new();

    for file in rust_files(&root.join("tests")) {
        push_whole_file(&file, &mut regions);
    }

    for file in rust_files(&root.join("src")) {
        let is_test_file = file
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with("_tests.rs"));
        if is_test_file {
            push_whole_file(&file, &mut regions);
        } else {
            push_cfg_test_blocks(&file, &mut regions);
        }
    }

    regions
}

fn push_whole_file(file: &Path, regions: &mut Vec<TestRegion>) {
    if let Ok(text) = std::fs::read_to_string(file) {
        regions.push(TestRegion {
            origin: display_path(file),
            text,
        });
    }
}

fn push_cfg_test_blocks(file: &Path, regions: &mut Vec<TestRegion>) {
    let Ok(text) = std::fs::read_to_string(file) else {
        return;
    };
    let mut search_from = 0;
    while let Some(found) = text[search_from..].find("#[cfg(test)]") {
        let marker = search_from + found;
        let Some((start, end)) = brace_block_span(&text[marker..]) else {
            break;
        };
        regions.push(TestRegion {
            origin: format!("{} (#[cfg(test)] block)", display_path(file)),
            text: text[marker + start..marker + end].to_owned(),
        });
        // Both ends are measured from `marker`, so the cursor has to move by
        // `end`, not by the block's length -- the gap between the attribute and
        // its opening brace is not part of the block. Getting that wrong lands
        // the next search mid-character on any source file containing non-ASCII.
        search_from = marker + end;
    }
}

/// Byte span of the brace-balanced body following an attribute, relative to the
/// start of `text`. `None` when no brace follows -- a `#[cfg(test)] mod name;`
/// declaration, for instance.
///
/// The returned end is always a char boundary because `}` is single-byte.
fn brace_block_span(text: &str) -> Option<(usize, usize)> {
    let open = text.find('{')?;
    let mut depth = 0_usize;
    for (offset, character) in text[open..].char_indices() {
        match character {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some((open, open + offset + 1));
                }
            }
            _ => {}
        }
    }
    None
}

fn rust_files(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    collect(dir, &mut found);
    found
}

fn collect(dir: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        // Unreadable directories are not reported here on purpose: a warning
        // printed by a passing test is not read by anyone. The non-vacuity
        // assertions in the test body are what actually catch a scan that has
        // stopped seeing the tree.
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
