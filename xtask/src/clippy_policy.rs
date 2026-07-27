//! Clippy allow/expect suppression policy (issue #459, A4 + A5).
//!
//! Zero-tolerance gate for first-party clippy `allow`/`expect` suppressions,
//! ported from the Python/Bash `scripts/check-clippy-allows.sh`. Also verifies
//! that the root `clippy.toml` and `.github/clippy/clippy.toml` keep the same
//! five complexity thresholds so `CLIPPY_CONF_DIR` cannot silently fall back
//! to defaults.
//!
//! The scanner is a faithful Rust port of the existing Python lexer: a
//! Rust-aware tokenizer that skips comments, string/char/raw-string literals,
//! and char literals (but not lifetimes), tracks nested brackets inside
//! attributes, and matches `(allow|expect)\s*\([^)]*clippy::` on the sanitized
//! attribute text. Scanner errors fail closed.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::process::{CommandFailed, CommandPlan};

/// One clippy `allow`/`expect` suppression found in a source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suppression {
    pub file: PathBuf,
    pub attribute: String,
}

/// A scanner-level failure (distinct from a policy failure, which is a found
/// suppression). Scanner errors fail closed: the policy treats them as a
/// failure rather than a clean result.
#[derive(Debug)]
pub enum ScanError {
    Io(PathBuf, String),
    Git(String),
}

impl std::fmt::Display for ScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(path, err) => write!(f, "io error reading {}: {err}", path.display()),
            Self::Git(err) => write!(f, "git file enumeration failed: {err}"),
        }
    }
}

/// The five complexity thresholds that must match between root and CI
/// `clippy.toml`. Kept as a single source of truth so the sync check and its
/// tests cannot drift.
const COMPLEXITY_THRESHOLDS: &[&str] = &[
    "cognitive-complexity-threshold",
    "too-many-lines-threshold",
    "too-many-arguments-threshold",
    "max-struct-bools",
    "type-complexity-threshold",
];

/// Run the clippy-allow policy against the repository root: scan git-tracked
/// first-party Rust files (excluding vendor/) and verify the clippy.toml
/// threshold sync.
///
/// # Errors
/// Returns `CommandFailed` if the scanner errors (fail closed), if any
/// suppression is found, or if the clippy.toml thresholds are missing or
/// mismatched.
pub fn run_repo_check(root: &Path) -> Result<(), CommandFailed> {
    let files = git_tracked_rust_files(root)
        .map_err(|err| to_command_failed("check", "clippy-allows", &err))?;
    let suppressions = scan_files(&files);
    if !suppressions.is_empty() {
        let mut stderr =
            String::from("first-party clippy allow attributes are forbidden; remove them:\n");
        for s in &suppressions {
            writeln!(stderr, "  {}\t{}", s.file.display(), s.attribute).ok();
        }
        return Err(CommandFailed {
            program: "xtask".into(),
            args: vec!["check".into(), "clippy-allows".into()],
            status: Some(1),
            stdout: Vec::new(),
            stderr: stderr.into_bytes(),
        });
    }
    match check_config_sync(root) {
        Ok(()) => Ok(()),
        Err(messages) => {
            let stderr = messages.join("\n");
            Err(CommandFailed {
                program: "xtask".into(),
                args: vec!["check".into(), "clippy-allows".into()],
                status: Some(1),
                stdout: Vec::new(),
                stderr: stderr.into_bytes(),
            })
        }
    }
}

/// Scan an explicit list of files.
///
/// Files that cannot be read are skipped; the policy's fail-closed behavior
/// is enforced at the enumeration layer in `run_repo_check`. This helper is
/// also used by tests with known-good fixtures).
#[must_use]
pub fn scan_files(files: &[PathBuf]) -> Vec<Suppression> {
    let mut all = Vec::new();
    for file in files {
        if let Ok(suppressions) = scan_file(file) {
            all.extend(suppressions);
        }
    }
    all
}

/// Scan a directory tree for `*.rs` files (the fixture/test entry point,
/// equivalent to the old `CLIPPY_ALLOW_SCAN_ROOT` environment variable).
///
/// # Errors
/// Returns `ScanError` if the directory walk or any file read fails.
pub fn scan_directory(root: &Path) -> Result<Vec<Suppression>, ScanError> {
    let mut files = Vec::new();
    collect_rust_files(root, &mut files)?;
    files.sort();
    let mut all = Vec::new();
    for file in &files {
        let suppressions = scan_file(file)?;
        all.extend(suppressions);
    }
    Ok(all)
}

/// Scan a single file for clippy allow/expect suppressions.
///
/// # Errors
/// Returns `ScanError` if the file cannot be read.
pub fn scan_file(path: &Path) -> Result<Vec<Suppression>, ScanError> {
    let source = std::fs::read_to_string(path)
        .map_err(|err| ScanError::Io(path.to_path_buf(), err.to_string()))?;
    Ok(scan_source(path, &source))
}

/// Verify the five complexity thresholds are present and equal in both
/// `clippy.toml` and `.github/clippy/clippy.toml`.
///
/// # Errors
/// Returns a `Vec<String>` of failure messages if any threshold is missing or
/// mismatched. Empty vec / Ok means the configs are in sync.
pub fn check_config_sync(root: &Path) -> Result<(), Vec<String>> {
    let root_config = root.join("clippy.toml");
    let ci_config = root.join(".github").join("clippy").join("clippy.toml");
    let mut errors = Vec::new();
    let Ok(root_text) = std::fs::read_to_string(&root_config) else {
        errors.push(format!(
            "required file is missing: {}",
            root_config.display()
        ));
        return Err(errors);
    };
    let Ok(ci_text) = std::fs::read_to_string(&ci_config) else {
        errors.push(format!("required file is missing: {}", ci_config.display()));
        return Err(errors);
    };
    for key in COMPLEXITY_THRESHOLDS {
        let root_value = config_value(&root_text, key);
        let ci_value = config_value(&ci_text, key);
        match (root_value, ci_value) {
            (Some(rv), Some(cv)) if rv != cv => {
                errors.push(format!(
                    "clippy threshold mismatch for {key}: clippy.toml={rv}, .github/clippy/clippy.toml={cv}"
                ));
            }
            (None, _) => errors.push(format!("clippy.toml is missing clippy threshold: {key}")),
            (_, None) => errors.push(format!(
                ".github/clippy/clippy.toml is missing clippy threshold: {key}"
            )),
            _ => {}
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}
// --- file enumeration -------------------------------------------------------

/// Enumerate git-tracked first-party Rust files via `git ls-files`, excluding
/// vendor/. Entries that do not exist on disk (deleted in the working tree)
/// are skipped, matching the original shell behavior.
///
/// # Errors
/// Returns `ScanError::Git` if `git ls-files` fails or cannot be spawned.
fn git_tracked_rust_files(root: &Path) -> Result<Vec<PathBuf>, ScanError> {
    let output = CommandPlan::new("git")
        .args(["ls-files", "--cached", "*.rs", ":!vendor/**"])
        .current_dir(root)
        .run_captured()
        .map_err(|err| ScanError::Git(format!("git ls-files failed: {err}")))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut files = Vec::new();
    for line in stdout.lines() {
        let path = root.join(line);
        if path.is_file() {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

/// Recursively collect `*.rs` files under `root`.
fn collect_rust_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), ScanError> {
    let entries =
        std::fs::read_dir(dir).map_err(|err| ScanError::Io(dir.to_path_buf(), err.to_string()))?;
    for entry in entries {
        let entry = entry.map_err(|err| ScanError::Io(dir.to_path_buf(), err.to_string()))?;
        let path = entry.path();
        let metadata = entry
            .metadata()
            .map_err(|err| ScanError::Io(path.clone(), err.to_string()))?;
        if metadata.is_dir() {
            collect_rust_files(&path, out)?;
        } else if metadata.is_file() && path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

/// Extract a `key = value` line's value from clippy.toml text, trimming
/// whitespace and trailing `# comment`. Returns the first match.
fn config_value(text: &str, key: &str) -> Option<String> {
    for line in text.lines() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix(key) else {
            continue;
        };
        let rest = rest.trim_start();
        let Some(after_eq) = rest.strip_prefix('=') else {
            continue;
        };
        // Value is everything after `=`, trimmed, with an optional trailing
        // `# comment` stripped.
        let mut value = after_eq.trim();
        if let Some(hash) = value.find('#') {
            value = value[..hash].trim_end();
        }
        return Some(value.to_string());
    }
    None
}

fn to_command_failed(command: &str, target: &str, err: &ScanError) -> CommandFailed {
    CommandFailed {
        program: "xtask".into(),
        args: vec![command.into(), target.into()],
        status: Some(1),
        stdout: Vec::new(),
        stderr: format!("clippy allow scanner failed: {err}").into_bytes(),
    }
}

// --- the scanner (faithful port of the Python lexer) -----------------------

/// Scan a source string for clippy allow/expect suppressions.
#[must_use]
pub fn scan_source(file: &Path, source: &str) -> Vec<Suppression> {
    let bytes: Vec<char> = source.chars().collect();
    let len = bytes.len();
    let mut index = 0usize;
    let mut found = Vec::new();
    while index < len {
        if starts_with(&bytes, index, "//") {
            index = skip_line_comment(&bytes, index);
        } else if starts_with(&bytes, index, "/*") {
            index = skip_block_comment(&bytes, index);
        } else {
            let raw_end = skip_raw_string(&bytes, index);
            if raw_end != index {
                index = raw_end;
            } else if bytes[index] == '"' {
                index = skip_string(&bytes, index);
            } else if bytes[index] == '\'' {
                index = skip_char_literal(&bytes, index);
            } else if bytes[index] == '#' {
                if let Some((attr, end)) = collect_attribute(&bytes, index) {
                    let normalized = sanitize(&attr);
                    if is_clippy_allow(&normalized) {
                        found.push(Suppression {
                            file: file.to_path_buf(),
                            attribute: normalized,
                        });
                    }
                    index = end;
                } else {
                    index += 1;
                }
            } else {
                index += 1;
            }
        }
    }
    found
}

/// Does `bytes[index..]` start with the given string of ASCII chars?
fn starts_with(bytes: &[char], index: usize, needle: &str) -> bool {
    let needle_chars: Vec<char> = needle.chars().collect();
    if index + needle_chars.len() > bytes.len() {
        return false;
    }
    for (offset, expected) in needle_chars.iter().enumerate() {
        if bytes[index + offset] != *expected {
            return false;
        }
    }
    true
}

fn skip_line_comment(bytes: &[char], index: usize) -> usize {
    let mut i = index + 2;
    while i < bytes.len() && bytes[i] != '\n' {
        i += 1;
    }
    if i < bytes.len() { i + 1 } else { bytes.len() }
}

fn skip_block_comment(bytes: &[char], index: usize) -> usize {
    let mut depth = 1i32;
    let mut i = index + 2;
    while i < bytes.len() && depth > 0 {
        if starts_with(bytes, i, "/*") {
            depth += 1;
            i += 2;
        } else if starts_with(bytes, i, "*/") {
            depth -= 1;
            i += 2;
        } else {
            i += 1;
        }
    }
    i
}

fn skip_string(bytes: &[char], index: usize) -> usize {
    let mut i = index + 1;
    while i < bytes.len() {
        if bytes[i] == '\\' {
            i += 2;
        } else if bytes[i] == '"' {
            return i + 1;
        } else {
            i += 1;
        }
    }
    i
}

/// Skip a char literal, but NOT a lifetime. A char literal is `'x'` or `'\x'`;
/// a lifetime is `'a` without a closing quote. If the char after the opening
/// quote is not followed by a closing quote, treat it as a lifetime and
/// advance only one char (the quote), preserving the rest of the file for
/// scanning.
fn skip_char_literal(bytes: &[char], index: usize) -> usize {
    let mut cursor = index + 1;
    if cursor >= bytes.len() {
        return index + 1;
    }
    if bytes[cursor] == '\\' {
        cursor += 2;
    } else {
        cursor += 1;
    }
    if cursor < bytes.len() && bytes[cursor] == '\'' {
        cursor + 1
    } else {
        // Not a char literal (likely a lifetime); only consumed the quote.
        index + 1
    }
}

/// Skip a raw string: `r"..."`, `r#"..."#`, `br"..."`, `br#"..."#`. Returns
/// the index after the closing terminator, or `index` if the bytes at `index`
/// do not begin a raw string.
fn skip_raw_string(bytes: &[char], index: usize) -> usize {
    let start = index;
    let mut i = index;
    if starts_with(bytes, i, "br") {
        i += 2;
    } else if bytes.get(i) == Some(&'r') {
        i += 1;
    } else {
        return start;
    }
    let mut hashes = 0usize;
    while i < bytes.len() && bytes[i] == '#' {
        hashes += 1;
        i += 1;
    }
    if i >= bytes.len() || bytes[i] != '"' {
        return start;
    }
    // Build the terminator: `"` followed by `hashes` `#` chars.
    let mut terminator: Vec<char> = vec!['"'];
    terminator.resize(hashes + 1, '#');
    let mut search = i + 1;
    while search + terminator.len() <= bytes.len() {
        if bytes[search..search + terminator.len()] == terminator[..] {
            return search + terminator.len();
        }
        search += 1;
    }
    bytes.len()
}

/// Collect a `#[...]` or `#![...]` attribute starting at `index` (which points
/// at `#`). Returns `(attribute_text_including_hashes, index_after_closing_bracket)`
/// or `None` if this is not a valid attribute start.
fn collect_attribute(bytes: &[char], start: usize) -> Option<(String, usize)> {
    let len = bytes.len();
    let mut index = start + 1;
    while index < len && bytes[index].is_whitespace() {
        index += 1;
    }
    if index < len && bytes[index] == '!' {
        index += 1;
        while index < len && bytes[index].is_whitespace() {
            index += 1;
        }
    }
    if index >= len || bytes[index] != '[' {
        return None;
    }
    let mut depth = 1i32;
    index += 1;
    while index < len && depth > 0 {
        if starts_with(bytes, index, "//") {
            index = skip_line_comment(bytes, index);
        } else if starts_with(bytes, index, "/*") {
            index = skip_block_comment(bytes, index);
        } else {
            let raw_end = skip_raw_string(bytes, index);
            if raw_end != index {
                index = raw_end;
            } else if bytes[index] == '"' {
                index = skip_string(bytes, index);
            } else if bytes[index] == '\'' {
                index = skip_char_literal(bytes, index);
            } else if bytes[index] == '[' {
                depth += 1;
                index += 1;
            } else if bytes[index] == ']' {
                depth -= 1;
                index += 1;
            } else {
                index += 1;
            }
        }
    }
    if depth != 0 {
        return None;
    }
    let attr: String = bytes[start..index].iter().collect();
    Some((attr, index))
}

/// Strip comments and literals from an attribute string and normalize
/// whitespace, so the suppression matcher sees only structural tokens.
fn sanitize(attr: &str) -> String {
    let bytes: Vec<char> = attr.chars().collect();
    let len = bytes.len();
    let mut index = 0usize;
    let mut out = String::new();
    while index < len {
        if starts_with(&bytes, index, "//") {
            out.push(' ');
            index = skip_line_comment(&bytes, index);
        } else if starts_with(&bytes, index, "/*") {
            out.push(' ');
            index = skip_block_comment(&bytes, index);
        } else {
            let raw_end = skip_raw_string(&bytes, index);
            if raw_end != index {
                out.push(' ');
                index = raw_end;
            } else if bytes[index] == '"' {
                out.push(' ');
                index = skip_string(&bytes, index);
            } else if bytes[index] == '\'' {
                out.push(' ');
                index = skip_char_literal(&bytes, index);
            } else {
                out.push(bytes[index]);
                index += 1;
            }
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Match `(allow|expect)\s*\([^)]*clippy\s*::` on a sanitized attribute,
/// accounting for optional `r#` raw-identifier prefixes and optional
/// whitespace around `::`.
fn is_clippy_allow(normalized: &str) -> bool {
    // Find `allow(` or `expect(` (with optional whitespace between the keyword
    // and the paren — sanitizer collapses runs to single spaces).
    for keyword in ["allow", "expect"] {
        let mut search = 0usize;
        while let Some(rel) = normalized[search..].find(keyword) {
            let abs = search + rel;
            // Ensure the keyword is a word boundary (preceded by non-ident).
            if abs > 0 {
                let prev = normalized.as_bytes()[abs - 1];
                if prev.is_ascii_alphanumeric() || prev == b'_' {
                    search = abs + keyword.len();
                    continue;
                }
            }
            // After the keyword, optional whitespace then `(`.
            let mut after = abs + keyword.len();
            while after < normalized.len() && normalized.as_bytes()[after].is_ascii_whitespace() {
                after += 1;
            }
            if after >= normalized.len() || normalized.as_bytes()[after] != b'(' {
                search = abs + keyword.len();
                continue;
            }
            // From the `(` onward, look for `(r#)?clippy\s*::` before any `)`.
            if clippy_path_appears_before_close(&normalized[after..]) {
                return true;
            }
            search = abs + keyword.len();
        }
    }
    false
}

/// Within `rest` (starting at `(`), check whether `(r#)?clippy\s*::` appears
/// before the matching closing `)`. The sanitizer already normalized
/// whitespace to single spaces and stripped literals, so nesting here is flat
/// enough to scan to the first `)`.
///
/// A word boundary is enforced before the `clippy` token so identifiers that
/// merely contain `clippy` as a substring (e.g. `my_clippy::lint`) are not
/// falsely classified.
fn clippy_path_appears_before_close(rest: &str) -> bool {
    let bytes = rest.as_bytes();
    let mut i = 0usize;
    let len = bytes.len();
    while i < len {
        if bytes[i] == b')' {
            return false;
        }
        // Optional `r#` prefix.
        let start = i;
        let mut j = i;
        if rest[j..].starts_with("r#") {
            j += 2;
        }
        if rest[j..].starts_with("clippy") {
            // Word boundary: the character before `clippy` (or `r#clippy`)
            // must not be an identifier-continue character. `start` is the
            // position of `r#` or of `clippy` itself; if `start > 0` the
            // preceding byte must not be alphanumeric or `_`.
            if start > 0 && is_ident_continue(bytes[start - 1]) {
                i = start + 1;
                continue;
            }
            j += "clippy".len();
            // Optional whitespace around `::`.
            while j < len && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if j + 1 < len && bytes[j] == b':' && bytes[j + 1] == b':' {
                return true;
            }
        }
        i = start + 1;
    }
    false
}

/// Is `b` a Rust identifier-continue character (alphanumeric or `_`)?
const fn is_ident_continue(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}
