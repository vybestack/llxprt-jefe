//! `${workspace}` interpolation for env values and launch argv (issue #380).
//!
//! The grammar is closed: `${workspace}` may appear only as the complete
//! prefix of a value, `$$` is a literal `$`, and every other `$` use —
//! unknown `${name}`, embedded `${workspace}`, unterminated `${`, or a bare
//! `$` — is `HAR-E003`. Paths never interpolate and never pass through here.

use super::error::HarnessError;

/// Validate a value's interpolation grammar without applying it. Runs at
/// parse time so violations fail before any workspace or launch work.
///
/// # Errors
///
/// `HAR-E003` on any violation.
pub fn validate_value(field: &str, value: &str) -> Result<(), HarnessError> {
    scan(field, value, None).map(|_| ())
}

/// Apply interpolation: substitute a complete `${workspace}` prefix with
/// `workspace_root` and decode `$$` escapes.
///
/// # Errors
///
/// `HAR-E003` on any violation.
pub fn apply(field: &str, value: &str, workspace_root: &str) -> Result<String, HarnessError> {
    scan(field, value, Some(workspace_root))
}

const WORKSPACE_REF: &str = "${workspace}";

fn scan(field: &str, value: &str, root: Option<&str>) -> Result<String, HarnessError> {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    if let Some(after) = rest.strip_prefix(WORKSPACE_REF) {
        out.push_str(root.unwrap_or(WORKSPACE_REF));
        rest = after;
    }
    let mut chars = rest.char_indices().peekable();
    while let Some((index, ch)) = chars.next() {
        if ch != '$' {
            out.push(ch);
            continue;
        }
        match chars.peek().map(|(_, next)| *next) {
            Some('$') => {
                chars.next();
                out.push('$');
            }
            Some('{') => {
                let tail = &rest[index..];
                let detail = if tail.starts_with(WORKSPACE_REF) {
                    format!("{field}: ${{workspace}} is only allowed as the complete value prefix")
                } else {
                    match tail[2..].find('}') {
                        Some(end) => format!(
                            "{field}: unknown interpolation '${{{}}}'",
                            &tail[2..2 + end]
                        ),
                        None => format!("{field}: unterminated '${{'"),
                    }
                };
                return Err(HarnessError::interpolation(detail));
            }
            _ => {
                return Err(HarnessError::interpolation(format!(
                    "{field}: bare '$' must be written as '$$'"
                )));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
#[path = "interp_tests.rs"]
mod interp_tests;
