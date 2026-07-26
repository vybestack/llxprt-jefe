//! Secret / PII redaction for `jefe doctor` output (issue #264, AC-09).
//!
//! [`redact_value`] is a pure function that replaces sensitive substrings with
//! stable structural markers while leaving actionable labels (path shapes,
//! host names, tool names, section headers) intact. It is applied to every
//! evidence string before the report is rendered.
//!
//! No regular-expression crate is used (the project does not depend on one);
//! each sensitive category is scanned by a small, focused matcher so the
//! behaviour is auditable and dependency-free.

/// Replace sensitive substrings in `value` with structural redaction markers.
///
/// The categories handled, in priority order, are:
///
/// 1. URL userinfo / embedded passwords (`scheme://user[:pass]@host`).
/// 2. SSH userinfo (`user@host:`).
/// 3. Windows SIDs (`S-1-...`).
/// 4. GitHub token shapes (`ghp_`, `gho_`, `ghs_`, `ghr_`, `github_pat_`).
/// 5. `Bearer` authorization tokens.
/// 6. Secret environment variable assignments (`<NAME>=<value>`).
/// 7. Long hex / base64-looking credential blobs.
/// 8. Prompt / passphrase evidence.
/// 9. Home directory paths and embedded usernames.
///
/// Structural labels (host names, tool names, env-var names, section headers)
/// are intentionally preserved so the report stays actionable.
#[must_use]
pub fn redact_value(value: &str) -> String {
    let redacted = redact_url_userinfo(value);
    let redacted = redact_ssh_userinfo(&redacted);
    let redacted = redact_windows_sid(&redacted);
    let redacted = redact_github_tokens(&redacted);
    let redacted = redact_bearer_token(&redacted);
    let redacted = redact_secret_env_assignment(&redacted);
    let redacted = redact_long_credentials(&redacted);
    let redacted = redact_prompt_evidence(&redacted);
    redact_home_and_usernames(&redacted)
}

/// Mask `scheme://user[:pass]@host` userinfo, preserving the host.
///
/// Each URL occurrence is processed independently so a line containing both
/// an HTTPS URL and a separate SSH/email reference does not have its content
/// span-corrupted between unrelated `://` and `@` markers.
fn redact_url_userinfo(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut consumed = 0;
    while let Some(at) = value[consumed..].find('@') {
        let at_abs = consumed + at;
        let before_at = &value[consumed..at_abs];
        let Some(scheme_rel) = before_at.rfind("://") else {
            let copy_to = at_abs + 1;
            result.push_str(&value[consumed..copy_to]);
            consumed = copy_to;
            continue;
        };
        let userinfo = &before_at[scheme_rel + 3..];
        if userinfo.chars().any(char::is_whitespace) {
            let copy_to = at_abs + 1;
            result.push_str(&value[consumed..copy_to]);
            consumed = copy_to;
            continue;
        }
        let scheme_end = consumed + scheme_rel + 3;
        result.push_str(&value[consumed..scheme_end]);
        result.push_str("[redacted]@");
        consumed = at_abs + 1;
    }
    result.push_str(&value[consumed..]);
    result
}

/// Mask `user@host:` SSH userinfo, scanning each `@` occurrence
/// independently so an HTTPS URL elsewhere in the line does not create a
/// blind spot for a separate SSH-style credential.
fn redact_ssh_userinfo(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut consumed = 0;
    while let Some(at) = value[consumed..].find('@') {
        let at_abs = consumed + at;
        let before_at = &value[..at_abs];
        let after_at = &value[at_abs + 1..];
        let host_end = after_at.find(char::is_whitespace).unwrap_or(after_at.len());
        if scheme_precedes_at(before_at) || !after_at[..host_end].contains(':') {
            let copy_to = at_abs + 1;
            result.push_str(&value[consumed..copy_to]);
            consumed = copy_to;
            continue;
        }
        let start = before_at
            .rfind(|c: char| c.is_whitespace() || c == ':')
            .map_or(0, |i| i + 1);
        if start >= at_abs || value[start..at_abs].starts_with("[redacted]") {
            let copy_to = at_abs + 1;
            result.push_str(&value[consumed..copy_to]);
            consumed = copy_to;
            continue;
        }
        result.push_str(&value[consumed..start]);
        result.push_str("[redacted]@");
        consumed = at_abs + 1;
    }
    result.push_str(&value[consumed..]);
    result
}

/// Whether a `://` URL scheme immediately precedes the `@` at the end of
/// `before_at` (i.e. this `@` is URL userinfo already handled elsewhere).
fn scheme_precedes_at(before_at: &str) -> bool {
    // If there is a `://` with no path separator between it and the `@`, the
    // `@` belongs to URL userinfo.
    if let Some(scheme_idx) = before_at.rfind("://") {
        let between = &before_at[scheme_idx + 3..];
        // URL userinfo has no whitespace; if any whitespace separates the
        // scheme from the `@`, the `@` is a separate context.
        !between.chars().any(char::is_whitespace)
    } else {
        false
    }
}

/// Mask raw Windows SIDs of the form `S-1-5-21-...`.
fn redact_windows_sid(value: &str) -> String {
    replace_all_pattern(value, "S-1-", |rest, _| {
        let mut end = 0;
        for (idx, ch) in rest.char_indices() {
            if ch.is_ascii_digit() || ch == '-' {
                end = idx + ch.len_utf8();
            } else {
                break;
            }
        }
        (end >= 4).then_some(end)
    })
}

/// Mask GitHub PAT/OAuth/app/refresh/fine-grained token shapes.
fn redact_github_tokens(value: &str) -> String {
    let mut current = value.to_string();
    for prefix in ["github_pat_", "ghp_", "gho_", "ghs_", "ghr_", "ghu_"] {
        current = replace_token_with_prefix(&current, prefix);
    }
    current
}

/// Replace a `<prefix><token-char-run>` with the prefix plus `[redacted]`.
fn replace_token_with_prefix(value: &str, prefix: &str) -> String {
    replace_all_pattern(value, prefix, |rest, _| {
        let mut end = 0;
        for (idx, ch) in rest.char_indices() {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                end = idx + ch.len_utf8();
            } else {
                break;
            }
        }
        (end >= 8).then_some(end)
    })
}

/// Mask a `Bearer <token>` run, preserving the `Bearer` label.
fn redact_bearer_token(value: &str) -> String {
    let lower = value.to_ascii_lowercase();
    let Some(bearer_idx) = lower.find("bearer ") else {
        return value.to_string();
    };
    let token_start = bearer_idx + "bearer ".len();
    let token_end = token_start
        + value[token_start..]
            .char_indices()
            .take_while(|(_, c)| !c.is_whitespace())
            .last()
            .map_or(0, |(i, c)| i + c.len_utf8());
    if token_end <= token_start {
        return value.to_string();
    }
    let mut result = String::with_capacity(value.len());
    result.push_str(&value[..token_start]);
    result.push_str("[redacted]");
    result.push_str(&value[token_end..]);
    result
}

/// Mask the value of a secret-looking environment variable assignment
/// (`<NAME>=<value>`), preserving the variable name.
fn redact_secret_env_assignment(value: &str) -> String {
    let Some(eq) = value.find('=') else {
        return value.to_string();
    };
    let name = value[..eq].trim_start();
    if !looks_like_secret_env_name(name) {
        return value.to_string();
    }
    let mut result = String::with_capacity(value.len());
    result.push_str(&value[..=eq]);
    result.push_str("[redacted]");
    result
}

/// Whether an environment variable name looks like it holds a secret.
fn looks_like_secret_env_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    [
        "TOKEN",
        "SECRET",
        "KEY",
        "PASSWORD",
        "PASSPHRASE",
        "CREDENTIAL",
    ]
    .iter()
    .any(|marker| upper.contains(marker))
}

/// Mask long hex / alphanumeric credential blobs (>= 32 chars) that are not
/// part of a known structural token.
fn redact_long_credentials(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut result = String::with_capacity(value.len());
    let mut i = 0;
    while i < bytes.len() {
        match find_credential_run_start(bytes, i) {
            Some((s, e)) if is_credential_like(&value[s..e]) => {
                result.push_str(&value[i..s]);
                result.push_str("[redacted]");
                i = e;
            }
            Some((_s, e)) => {
                result.push_str(&value[i..e]);
                i = e;
            }
            None => {
                result.push_str(&value[i..]);
                break;
            }
        }
    }
    result
}

/// Find the next maximal run of credential characters starting at or after `i`.
fn find_credential_run_start(bytes: &[u8], mut i: usize) -> Option<(usize, usize)> {
    while i < bytes.len() {
        if is_credential_char(bytes[i]) {
            let start = i;
            while i < bytes.len() && is_credential_char(bytes[i]) {
                i += 1;
            }
            return Some((start, i));
        }
        i += 1;
    }
    None
}

/// Characters considered part of a credential blob (hex + base64 alphabet).
fn is_credential_char(byte: u8) -> bool {
    byte.is_ascii_hexdigit() || matches!(byte, b'_' | b'-' | b'+' | b'/' | b'=')
}

/// Whether a credential-character run looks like a secret (length + entropy).
///
/// Pure-hex runs that exactly match a Git SHA-1 (40 hex chars) or SHA-256
/// (64 hex chars) commit/checksum are preserved because they are legitimate
/// diagnostic data, not credentials. A bare hex run of any other length that
/// is still 32+ chars is treated as a credential blob.
fn is_credential_like(run: &str) -> bool {
    if run.len() < 32 {
        return false;
    }
    if run.chars().all(|c| c.is_ascii_hexdigit()) {
        // Preserve common commit/checksum lengths so doctor reports stay
        // actionable (e.g. `git rev-parse HEAD` output).
        return !matches!(run.len(), 40 | 64);
    }
    let has_digit = run.chars().any(|c| c.is_ascii_digit());
    let has_alpha = run.chars().any(|c| c.is_ascii_alphabetic());
    has_digit && has_alpha && run.chars().all(|c| c.is_ascii_alphanumeric())
}

/// Mask prompt / passphrase evidence fragments.
fn redact_prompt_evidence(value: &str) -> String {
    let mut current = value.replace("Enter passphrase for key", "[redacted prompt]");
    current = current.replace("Password:", "[redacted prompt]");
    current.replace("Password for", "[redacted prompt]")
}

/// Mask home directory paths and Windows `\Username` suffixes.
fn redact_home_and_usernames(value: &str) -> String {
    let redacted = redact_posix_home(value);
    redact_windows_user_home(&redacted)
}

/// Replace `/home/<user>` and `/Users/<user>` style paths with markers.
fn redact_posix_home(value: &str) -> String {
    let mut current = value.to_string();
    for prefix in ["/home/", "/Users/"] {
        current = replace_segment_after(&current, prefix);
    }
    current
}

/// Replace `C:\Users\<user>` Windows profile paths with a structural marker.
/// Every occurrence is redacted, not just the first.
fn redact_windows_user_home(value: &str) -> String {
    const PREFIX: &str = "users\\";
    let lower = value.to_ascii_lowercase();
    let mut result = String::with_capacity(value.len());
    let mut consumed = 0;
    while let Some(relative) = lower[consumed..].find(PREFIX) {
        let prefix_start = consumed + relative;
        let segment_start = prefix_start + PREFIX.len();
        let end = segment_end(value, segment_start);
        if end <= segment_start {
            break;
        }
        result.push_str(&value[consumed..segment_start]);
        result.push_str("[redacted-user]");
        consumed = end;
    }
    result.push_str(&value[consumed..]);
    result
}

/// Replace the path segment immediately following `prefix` with a redaction
/// marker, preserving `prefix` itself. Every occurrence is redacted.
fn replace_segment_after(value: &str, prefix: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut consumed = 0;
    while let Some(relative) = value[consumed..].find(prefix) {
        let prefix_start = consumed + relative;
        let segment_start = prefix_start + prefix.len();
        let end = segment_end(value, segment_start);
        if end <= segment_start {
            break;
        }
        result.push_str(&value[consumed..segment_start]);
        result.push_str("[redacted-user]");
        consumed = end;
    }
    result.push_str(&value[consumed..]);
    result
}

/// Compute the byte end of the path segment starting at `segment_start`.
fn segment_end(value: &str, segment_start: usize) -> usize {
    value[segment_start..]
        .char_indices()
        .take_while(|(_, c)| *c != '/' && *c != '\\')
        .last()
        .map_or(0, |(i, c)| i + c.len_utf8())
        + if segment_start <= value.len() {
            segment_start
        } else {
            value.len()
        }
}

/// Repeatedly locate occurrences of `pattern` in `value` and, when `consume`
/// returns a byte end offset, splice in `pattern` + `[redacted]` over
/// `[start..end)`. Continues scanning after each redaction so every
/// occurrence of a token/SID/credential is redacted, not just the first.
fn replace_all_pattern(
    value: &str,
    pattern: &str,
    consume: impl Fn(&str, usize) -> Option<usize>,
) -> String {
    let mut result = String::with_capacity(value.len());
    let mut consumed = 0;
    let mut search = &value[consumed..];
    while let Some(relative_start) = search.find(pattern) {
        let start = consumed + relative_start;
        let after_pattern = start + pattern.len();
        let tail = &value[after_pattern..];
        let next_consumed = if let Some(relative_end) = consume(tail, pattern.len()) {
            let absolute_end = after_pattern + relative_end;
            result.push_str(&value[consumed..start]);
            result.push_str(pattern);
            result.push_str("[redacted]");
            absolute_end
        } else {
            // Not a credential run; copy through the pattern and continue.
            result.push_str(&value[consumed..after_pattern]);
            after_pattern
        };
        consumed = next_consumed;
        search = &value[consumed..];
    }
    if consumed < value.len() {
        result.push_str(&value[consumed..]);
    }
    if result.is_empty() {
        return value.to_string();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_redacted(raw: &str, fixture: &str) {
        let redacted = redact_value(raw);
        assert!(
            !redacted.contains(fixture),
            "still contains {fixture:?}\nraw: {raw:?}\nredacted: {redacted:?}"
        );
    }

    #[test]
    fn preserves_purely_structural_content() {
        let raw = "multiplexer: psmux 0.9.2 at /usr/bin/psmux";
        assert_eq!(redact_value(raw), raw);
    }

    #[test]
    fn redacts_url_userinfo_and_password() {
        assert_redacted("https://alice:s3cr3t@github.com/acme/widgets.git", "s3cr3t");
        assert_redacted("https://alice@github.com/acme/widgets.git", "alice@");
        assert!(redact_value("https://alice@github.com/x").contains("github.com"));
    }

    #[test]
    fn redacts_bearer_and_github_tokens() {
        assert_redacted(
            "Authorization: Bearer eyJhbGciOiJIUzI1NiJ9.payload.signature",
            "eyJhbGciOiJIUzI1NiJ9.payload.signature",
        );
        assert_redacted(
            "token: ghp_0123456789abcdefghijklmnopqrstuvwxyzABCD",
            "ghp_0123456789abcdefghijklmnopqrstuvwxyzABCD",
        );
    }

    #[test]
    fn redacts_long_hex_credential() {
        // A 48-char hex run is not a standard commit/checksum length, so it is
        // treated as a credential blob.
        let raw = "credential: 9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c";
        let redacted = redact_value(raw);
        assert!(!redacted.contains("9f86d081"));
        assert!(redacted.contains("credential"));
    }

    #[test]
    fn preserves_sha1_and_sha256_hex_lengths() {
        // 40-char (SHA-1) and 64-char (SHA-256) hex runs are legitimate
        // commit/checksum data and must not be redacted.
        let sha1 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let sha256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        assert_eq!(redact_value(sha1), sha1, "SHA-1 commit hash must survive");
        assert_eq!(
            redact_value(sha256),
            sha256,
            "SHA-256 checksum must survive"
        );
    }

    #[test]
    fn redacts_prompt_evidence() {
        assert_redacted("gh stderr: Password: ", "Password:");
    }
}
