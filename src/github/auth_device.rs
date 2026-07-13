//! In-app device-code auth remediation boundary (issue #244).
//!
//! Pure helpers for driving `gh auth login --web` non-interactively from the
//! TUI: the exact scopes Jefe requests, the assembled argv, the no-op-browser
//! environment, and the parser that extracts the one-time code + verification
//! URL from `gh`'s stderr.
//!
//! This module performs NO I/O. The runtime layer (`runtime/gh_auth.rs`) owns
//! the subprocess spawn; the state layer owns the dialog state machine.
//!
//! # Why these scopes and flags
//!
//! `gh` always requests the minimum scopes `["repo", "read:org", "gist"]`
//! (see `internal/authflow/flow.go` in cli/cli). Passing them explicitly via
//! `--scopes` keeps the grant auditable at the call site even if `gh`'s
//! defaults change. `--web` selects the device-code flow; with stdin not a
//! TTY, `gh` takes its non-interactive path (no "Press Enter" prompt) and
//! prints the code + URL to stderr. `GH_BROWSER=/bin/true` prevents `gh` from
//! spawning a browser on a headless/remote host — the user opens the URL on
//! any device themselves.

/// The exact OAuth scopes Jefe requests for its `gh` token (issue #244).
///
/// `repo` (repo read/write + private), `read:org` (read org membership),
/// `gist` (create gists). Mirrors `gh`'s own minimum scope set so the token
/// is minimally privileged for Jefe's needs.
pub const AUTH_SCOPES: &[&str] = &["repo", "read:org", "gist"];

/// The fixed hostname Jefe authenticates against (github.com per the issue).
const AUTH_HOSTNAME: &str = "github.com";
/// The fixed git protocol (https per the issue).
const AUTH_GIT_PROTOCOL: &str = "https";

/// A parsed one-time device code and the URL the user must open in a browser.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceCode {
    /// The one-time code, e.g. `7701-C5F6`.
    pub code: String,
    /// The verification URL, e.g. `https://github.com/login/device`.
    pub verification_url: String,
}

/// Build the `gh auth login` argv for the non-interactive device-code flow.
///
/// Returns one `--scopes <scope>` pair per requested scope so the granted
/// scopes are exactly the set passed in (no reliance on `gh`'s interactive
/// default-scope prompt, which does not fit a TUI).
#[must_use]
pub fn build_auth_login_args(scopes: &[&str]) -> Vec<String> {
    let mut args = vec![
        "auth".to_string(),
        "login".to_string(),
        "--hostname".to_string(),
        AUTH_HOSTNAME.to_string(),
        "--git-protocol".to_string(),
        AUTH_GIT_PROTOCOL.to_string(),
        "--web".to_string(),
    ];
    for scope in scopes {
        args.push("--scopes".to_string());
        args.push((*scope).to_string());
    }
    args
}

/// Build the environment overrides for the non-interactive device-code flow.
///
/// `GH_BROWSER=/bin/true` makes `gh`'s browser-open step a no-op so it never
/// spawns a browser on the Jefe host (which may be headless or remote over
/// SSH). The user opens the verification URL on whatever device they choose.
#[must_use]
pub fn build_auth_login_env() -> Vec<(&'static str, &'static str)> {
    vec![("GH_BROWSER", "/bin/true")]
}

/// Replace anything matching the GitHub device-code shape (`XXXX-XXXX`,
/// case-insensitive alphanumeric, 4+dash+4) with `<redacted>`.
///
/// `gh auth login` can echo the one-time code back to stderr on failure
/// (expired, denied). Because failure messages flow into `AuthDialogPhase::Failed`
/// — which is part of `AppState` and therefore logged/printed — scrub the code
/// shape before the string enters state, so a short-lived bearer credential
/// cannot leak via crash reports or logs (issue #244 OCR review).
#[must_use]
pub fn redact_device_codes(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if is_code_at_boundary(&chars, i) {
            out.push_str("<redacted>");
            // Skip the 9-char code (4 + '-' + 4).
            i += 9;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// True if `chars[pos..]` starts a standalone `XXXX-XXXX` token (4 alphanumerics,
/// dash, 4 alphanumerics) with non-alphanumeric boundaries on both sides.
fn is_code_at_boundary(chars: &[char], pos: usize) -> bool {
    // 4 alnum, '-', 4 alnum = 9 chars at indices pos..=pos+8.
    if pos + 8 >= chars.len() {
        return false;
    }
    let code_chars = &chars[pos..=pos + 8];
    if code_chars[4] != '-' {
        return false;
    }
    let first_four = &code_chars[0..4];
    let last_four = &code_chars[5..9];
    if !first_four.iter().all(char::is_ascii_alphanumeric)
        || !last_four.iter().all(char::is_ascii_alphanumeric)
    {
        return false;
    }
    // Word boundaries: the char before pos and the char after pos+8 must not
    // be alphanumeric (so we don't redact a substring of a longer token).
    let left_ok = pos == 0 || !chars[pos - 1].is_ascii_alphanumeric();
    let right_ok = pos + 9 >= chars.len() || !chars[pos + 9].is_ascii_alphanumeric();
    left_ok && right_ok
}

/// Returns true when an error string indicates a `gh` authentication failure.
///
/// Delegates to the shared `not_authenticated_matcher` (in
/// `crate::github::parse`) that [`crate::github::categorize_error`] also uses
/// for its `NotAuthenticated` arm, so detection stays a single source of truth
/// — the dispatch layer cannot drift from the error categorizer (issue #244).
/// Used to decide whether to open the auth remediation dialog instead of
/// surfacing a bare error string.
#[must_use]
pub fn is_not_authenticated_error(error_text: &str) -> bool {
    super::parse::not_authenticated_matcher(&error_text.to_lowercase())
}

/// Strip ANSI/VT100 escape sequences from a string.
///
/// `gh` colorizes stderr when it believes the stream is a TTY; the parser must
/// cope with colorized output. This removes CSI sequences (`\x1b[...m`) and
/// other common escape forms.
fn strip_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip the rest of an escape sequence: ESC [ <params> <final byte>
            // (0x40..=0x7E), or ESC ] ... BEL/ST. Be permissive: consume until
            // a byte that cannot be inside a CSI sequence.
            if matches!(chars.peek(), Some('[')) {
                chars.next();
                for inner in chars.by_ref() {
                    if inner.is_ascii() && (0x40..=0x7E).contains(&(inner as u32)) {
                        break;
                    }
                }
            } else {
                // OSC or other: consume until BEL (\x07) or a following ESC.
                for inner in chars.by_ref() {
                    if inner == '\x07' {
                        break;
                    }
                    if inner == '\x1b' {
                        // A nested ESC starts a new sequence; consume it and
                        // stop this scan (the outer loop's next `chars.next()`
                        // picks up after it). gh's stderr uses SGR codes only,
                        // so this branch is rarely hit.
                        break;
                    }
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Parse the one-time device code and verification URL from `gh auth login
/// --web` stderr.
///
/// `gh` writes (to stderr):
///   `! First copy your one-time code: XXXX-XXXX`
///   `Open this URL to continue in your web browser: https://github.com/login/device[...]`
///
/// Returns `None` if no well-formed code (`XXXX-XXXX`, 4 hex/alphanumerics, a
/// dash, 4 more) is present.
#[must_use]
pub fn parse_device_code(stderr: &str) -> Option<DeviceCode> {
    let clean = strip_ansi(stderr);

    let code = extract_device_code(&clean)?;
    let verification_url = extract_verification_url(&clean).unwrap_or_else(|| {
        // Fall back to the canonical device-login URL if gh omitted it.
        "https://github.com/login/device".to_string()
    });

    Some(DeviceCode {
        code,
        verification_url,
    })
}

/// Extract the `XXXX-XXXX` one-time code following the `one-time code:` label.
fn extract_device_code(clean: &str) -> Option<String> {
    let label = clean.find("one-time code:")?;
    let rest = &clean[label + "one-time code:".len()..];
    let token = rest.trim_start();
    // Take the first run of non-whitespace chars as the candidate code.
    let candidate = token.split_whitespace().next()?;
    if is_valid_device_code(candidate) {
        Some(candidate.to_string())
    } else {
        None
    }
}

/// A device code is exactly `XXXX-XXXX`: 4 ASCII alphanumerics, a dash, 4
/// ASCII alphanumerics (GitHub's device-code format). Kept in lockstep with
/// [`is_code_at_boundary`] so anything [`parse_device_code`] accepts is also
/// redacted by [`redact_device_codes`] (defense-in-depth for the credential).
fn is_valid_device_code(candidate: &str) -> bool {
    let Some((left, right)) = candidate.split_once('-') else {
        return false;
    };
    left.len() == 4
        && right.len() == 4
        && segment_is_alphanumeric(left)
        && segment_is_alphanumeric(right)
}

/// True when every char in the segment is an ASCII letter or digit. Extracted
/// so the closure form does not trip clippy's `redundant_closure` lint (the
/// `char::is_ascii_alphanumeric` method takes `&self`, which is incompatible
/// with `Iterator::all`'s `Fn(Item)` bound).
fn segment_is_alphanumeric(segment: &str) -> bool {
    segment.chars().all(|c| c.is_ascii_alphanumeric())
}

/// Extract the verification URL from the `Open this URL ... : <url>` line.
fn extract_verification_url(clean: &str) -> Option<String> {
    // Find the github.com device URL anywhere in the output.
    let url_start = clean.find("https://github.com/login/device")?;
    let rest = &clean[url_start..];
    // URL ends at the first whitespace; then strip trailing sentence
    // punctuation that gh may append when the URL ends a clause (issue #244
    // OCR review: only trimming periods missed ')', ']', '}', etc.).
    let url = rest
        .split_whitespace()
        .next()
        .unwrap_or(rest)
        .trim_end_matches(['.', ')', ']', '}', ',', ';', ':', '!', '?', '\'', '"']);
    Some(url.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_ansi_removes_color_codes() {
        let input = "\x1b[33m!\x1b[0m hello \x1b[1mworld\x1b[0m";
        assert_eq!(strip_ansi(input), "! hello world");
    }

    #[test]
    fn is_valid_device_code_accepts_standard_shape() {
        assert!(is_valid_device_code("7701-C5F6"));
        assert!(is_valid_device_code("ABCD-1234"));
    }

    #[test]
    fn is_valid_device_code_rejects_garbage() {
        assert!(!is_valid_device_code("not-a-code"));
        assert!(!is_valid_device_code("1234"));
        assert!(!is_valid_device_code("-1234"));
        assert!(!is_valid_device_code("1234-"));
    }

    #[test]
    fn is_valid_device_code_requires_strict_4_plus_4() {
        // Anything other than exactly 4+4 is rejected, so the parser and the
        // redactor stay in lockstep (issue #244 OCR review).
        assert!(!is_valid_device_code("A-B"));
        assert!(!is_valid_device_code("ABC-DEFG"));
        assert!(!is_valid_device_code("ABCDE-FGHI"));
        assert!(is_valid_device_code("7701-C5F6"));
    }

    #[test]
    fn extract_verification_url_strips_trailing_punctuation() {
        // Trailing sentence punctuation must not become part of the URL.
        assert_eq!(
            extract_verification_url("Open this URL: https://github.com/login/device/abc."),
            Some("https://github.com/login/device/abc".to_string())
        );
        assert_eq!(
            extract_verification_url("(see https://github.com/login/device/abc))"),
            Some("https://github.com/login/device/abc".to_string())
        );
        assert_eq!(
            extract_verification_url("url: https://github.com/login/device/abc]."),
            Some("https://github.com/login/device/abc".to_string())
        );
    }

    #[test]
    fn extract_verification_url_returns_none_when_absent() {
        assert!(extract_verification_url("no url here").is_none());
    }

    #[test]
    fn is_code_at_boundary_matches_standalone_code() {
        let chars: Vec<char> = "code 7701-C5F6 done".chars().collect();
        let pos = "code ".len();
        assert!(is_code_at_boundary(&chars, pos));
    }

    #[test]
    fn is_code_at_boundary_rejects_mid_token() {
        // The 4+4 shape exists starting at index 4 of "tokenABCD-EFGH", but it
        // is preceded by an alphanumeric, so it must not be treated as a code.
        let s = "tokenABCD-EFGH";
        let chars: Vec<char> = s.chars().collect();
        let pos = "token".len();
        assert!(!is_code_at_boundary(&chars, pos));
    }
}
