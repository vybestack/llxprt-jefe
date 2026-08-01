//! Multiplexer contract surface policy (jefe issue #540, V5).
//!
//! jefe declares every psmux verb and format string it depends on in
//! `src/runtime/multiplexer_contract.rs`. A declaration nobody enforces drifts,
//! so this policy fails the build in both directions:
//!
//! * a format used in the runtime but never declared is a dependency the
//!   conformance suite will never assert, which is how divergence from tmux
//!   went unnoticed until it caused an incident;
//! * a format declared but never used makes the conformance suite demand a
//!   capability jefe does not need, which can reject a serviceable binary.
//!
//! Rust shares the `#{...}` spelling whenever a literal `#` precedes a
//! placeholder â€” `format!("... #{ordinal}")` names a variable in scope, and
//! `format!("#{next_display_index}")` builds an agent's display identifier.
//! Neither is a multiplexer format, so occurrences inside a formatting macro
//! are not counted.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::process::CommandFailed;

/// Directories that build multiplexer commands. Formats elsewhere in the tree
/// belong to jefe's own templating and are outside this policy.
const SCAN_ROOTS: &[&str] = &["src/runtime", "src/harness"];

/// The contract module the declarations are read from.
const CONTRACT_PATH: &str = "src/runtime/multiplexer_contract.rs";

/// Rust formatting macros whose arguments use `{...}` placeholders.
const FORMATTING_MACROS: &[&str] = &[
    "format!",
    "write!",
    "writeln!",
    "print!",
    "println!",
    "eprint!",
    "eprintln!",
    "panic!",
    "assert!",
    "assert_eq!",
    "assert_ne!",
    "unreachable!",
    "todo!",
];

/// Names declared in the contract.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Surface {
    pub formats: BTreeSet<String>,
}

/// A breach of the declared surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Violation {
    /// Used by the runtime but absent from the contract.
    UsedButNotDeclared { name: String, files: Vec<String> },
    /// Declared in the contract but used nowhere.
    DeclaredButNotUsed { name: String },
}

impl Violation {
    /// Operator-facing description.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::UsedButNotDeclared { name, files } => format!(
                "format `#{{{name}}}` is used but not declared in {CONTRACT_PATH} \
                 (seen in: {}). Declare it, or stop depending on it.",
                files.join(", "),
            ),
            Self::DeclaredButNotUsed { name } => format!(
                "format `#{{{name}}}` is declared in {CONTRACT_PATH} but used nowhere. \
                 Remove it, or the conformance suite will demand a capability jefe \
                 does not need.",
            ),
        }
    }
}

/// Read the declared format names out of the contract source.
///
/// Matches the `format("name", â€¦)` declarations rather than every string in the
/// file, so rationale prose cannot be mistaken for a declaration.
#[must_use]
pub fn declared_surface(contract_source: &str) -> Surface {
    let mut formats = BTreeSet::new();
    let mut rest = contract_source;

    while let Some(start) = rest.find("format(") {
        rest = &rest[start + "format(".len()..];
        let Some(open) = rest.find('"') else { break };
        let after_open = &rest[open + 1..];
        let Some(close) = after_open.find('"') else {
            break;
        };
        let name = &after_open[..close];
        // A declaration's first argument is the bare variable name; anything
        // containing punctuation is prose that happened to follow the keyword.
        if !name.is_empty()
            && name
                .chars()
                .all(|character| character.is_ascii_lowercase() || character == '_')
        {
            formats.insert(name.to_owned());
        }
        rest = after_open;
    }

    Surface { formats }
}

/// Collect the multiplexer format names a source file uses.
#[must_use]
pub fn format_usages(source: &str) -> BTreeSet<String> {
    let mut used = BTreeSet::new();

    for line in source.lines() {
        if FORMATTING_MACROS
            .iter()
            .any(|formatting_macro| line.contains(formatting_macro))
            || holds_rust_placeholder(line)
        {
            continue;
        }
        collect_line_usages(line, &mut used);
    }

    used
}

/// Whether a line carries a bare `{â€¦}` placeholder.
///
/// A multiplexer format spells every variable `#{â€¦}`, so an unprefixed brace
/// means the line is a Rust format string. This catches the case the macro name
/// alone cannot: a `format!` whose literal sits on a later line, as in
/// `"read capture record '{name}' #{ordinal}: {err}"`.
fn holds_rust_placeholder(line: &str) -> bool {
    let bytes = line.as_bytes();
    line.match_indices('{').any(|(index, _)| {
        let preceded_by_hash = index > 0 && bytes[index - 1] == b'#';
        // `{{` is an escaped brace, not a placeholder.
        let escaped = bytes.get(index + 1) == Some(&b'{');
        !preceded_by_hash && !escaped
    })
}

fn collect_line_usages(line: &str, used: &mut BTreeSet<String>) {
    let bytes = line.as_bytes();
    let mut index = 0;
    while let Some(offset) = line[index..].find("#{") {
        let start = index + offset + 2;
        let Some(end_offset) = line[start..].find('}') else {
            break;
        };
        let end = start + end_offset;
        let name = &line[start..end];
        if !name.is_empty()
            && name
                .chars()
                .all(|character| character.is_ascii_lowercase() || character == '_')
        {
            used.insert(name.to_owned());
        }
        index = end.min(bytes.len());
        if index >= line.len() {
            break;
        }
    }
}

/// Compare declared against used.
#[must_use]
pub fn surface_violations(declared: &BTreeSet<String>, used: &BTreeSet<String>) -> Vec<Violation> {
    let mut violations = Vec::new();

    for name in used.difference(declared) {
        violations.push(Violation::UsedButNotDeclared {
            name: name.clone(),
            files: Vec::new(),
        });
    }
    for name in declared.difference(used) {
        violations.push(Violation::DeclaredButNotUsed { name: name.clone() });
    }

    violations
}

/// Run the policy across the repository.
///
/// # Errors
/// Returns `CommandFailed` when the declared surface and its use diverge.
pub fn run_repo_check(root: &Path) -> Result<(), CommandFailed> {
    let contract_source = std::fs::read_to_string(root.join(CONTRACT_PATH)).map_err(|error| {
        failure(format!(
            "could not read the contract at {CONTRACT_PATH}: {error}"
        ))
    })?;
    let declared = declared_surface(&contract_source).formats;

    let mut used = BTreeSet::new();
    let mut sources_by_name: Vec<(String, String)> = Vec::new();
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
            // The contract itself names every format it declares; counting
            // those would make every declaration self-justifying.
            if relative.replace('\\', "/").ends_with(CONTRACT_PATH) {
                continue;
            }
            let file_usages = format_usages(&text);
            for name in &file_usages {
                sources_by_name.push((name.clone(), relative.clone()));
            }
            used.extend(file_usages);
        }
    }

    let violations: Vec<Violation> = surface_violations(&declared, &used)
        .into_iter()
        .map(|violation| match violation {
            Violation::UsedButNotDeclared { name, .. } => {
                let files = sources_by_name
                    .iter()
                    .filter(|(usage, _)| usage == &name)
                    .map(|(_, file)| file.clone())
                    .collect();
                Violation::UsedButNotDeclared { name, files }
            }
            declared @ Violation::DeclaredButNotUsed { .. } => declared,
        })
        .collect();

    if violations.is_empty() {
        return Ok(());
    }

    for violation in &violations {
        eprintln!("ERROR: {}", violation.describe());
    }
    Err(failure(format!(
        "Found {} multiplexer surface violation(s).",
        violations.len()
    )))
}

fn rust_sources(directory: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let Ok(entries) = std::fs::read_dir(directory) else {
        return files;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files.extend(rust_sources(&path));
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
    files
}

fn failure(message: String) -> CommandFailed {
    CommandFailed {
        program: "xtask".into(),
        args: vec!["check".into(), "multiplexer-surface".into()],
        status: Some(1),
        stdout: Vec::new(),
        stderr: message.into_bytes(),
    }
}
