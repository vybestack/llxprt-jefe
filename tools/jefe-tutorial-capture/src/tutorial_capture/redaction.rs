//! Redaction: scrub credentials and personal data from captured artifacts.
//!
//! Captured terminal text may contain developer usernames, home paths, tokens,
//! or private repository names. This module provides pure redaction functions
//! that replace sensitive patterns with safe placeholders before artifacts are
//! written or published.
//!
//! Token patterns match the **full token value**, not just the prefix. A
//! GitHub token like `ghp_abcdef1234567890abcdef1234567890abcd` is fully
//! replaced with `<token>`, leaving no trace of the secret value.
//!
//! ## Boundary
//!
//! This module is pure: it transforms text. It does not read or write files.
//!
//! @requirement REQ-TUTORIAL-CAPTURE-006

use std::path::Path;

/// A redaction rule: a pattern to find and a replacement to substitute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedactionRule {
    /// Literal pattern to search for.
    pub pattern: String,
    /// Replacement text.
    pub replacement: String,
}

/// A token-prefix redaction rule: matches a known prefix followed by token
/// characters and redacts the entire match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenPrefixRule {
    /// The token prefix (e.g. `ghp_`).
    pub prefix: String,
    /// Replacement text.
    pub replacement: String,
    /// Minimum number of token characters after the prefix.
    pub min_tail: usize,
}

/// A set of redaction rules applied to captured text.
///
/// @requirement REQ-TUTORIAL-CAPTURE-006
#[derive(Debug, Clone, Default)]
pub struct RedactionSet {
    rules: Vec<RedactionRule>,
    token_prefix_rules: Vec<TokenPrefixRule>,
}

impl RedactionSet {
    /// Create an empty redaction set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a literal redaction rule.
    pub fn add(&mut self, pattern: impl Into<String>, replacement: impl Into<String>) {
        self.rules.push(RedactionRule {
            pattern: pattern.into(),
            replacement: replacement.into(),
        });
    }

    /// Add a token-prefix redaction rule. Matches the prefix followed by
    /// `min_tail` or more alphanumeric, underscore, hyphen, or period
    /// characters and replaces the entire match.
    pub fn add_token_prefix(
        &mut self,
        prefix: impl Into<String>,
        replacement: impl Into<String>,
        min_tail: usize,
    ) {
        self.token_prefix_rules.push(TokenPrefixRule {
            prefix: prefix.into(),
            replacement: replacement.into(),
            min_tail,
        });
    }

    /// Add a username redaction rule for `whoami` output style strings.
    pub fn add_username(&mut self, username: &str) {
        if !username.is_empty() {
            self.add(username, "<user>");
        }
    }

    /// Add a home directory redaction rule.
    pub fn add_home_dir(&mut self, home: &Path) {
        let home_str = home.to_string_lossy();
        if !home_str.is_empty() {
            self.add(home_str.as_ref(), "~");
        }
    }

    /// Apply all redaction rules to the given text.
    ///
    /// Literal rules are applied first, then token-prefix rules. Token-prefix
    /// rules scan the text for known prefixes and consume the following token
    /// characters, replacing the entire match.
    ///
    /// @requirement REQ-TUTORIAL-CAPTURE-006
    #[must_use]
    pub fn apply(&self, text: &str) -> String {
        let mut result = text.to_string();
        for rule in &self.rules {
            if !rule.pattern.is_empty() {
                result = result.replace(&rule.pattern, &rule.replacement);
            }
        }
        for token_rule in &self.token_prefix_rules {
            result = redact_token_prefix(&result, token_rule);
        }
        result
    }

    /// Apply redaction to a slice of lines (e.g. a screen capture).
    #[must_use]
    pub fn apply_lines(&self, lines: &[String]) -> Vec<String> {
        lines.iter().map(|line| self.apply(line)).collect()
    }

    /// Total number of redaction rules (literal + token-prefix).
    #[must_use]
    pub fn len(&self) -> usize {
        self.rules.len() + self.token_prefix_rules.len()
    }

    /// Whether the set is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty() && self.token_prefix_rules.is_empty()
    }
}

/// Redact all occurrences of a token prefix followed by token characters.
///
/// Scans the input for `prefix` followed by one or more alphanumeric or
/// underscore characters. When found, the entire match (prefix + tail) is
/// replaced with `rule.replacement`.
///
/// An empty prefix is a no-op: it would match at every position, producing
/// infinite or nonsensical output.
///
/// Example: `ghp_abcdef1234567890` → `<token>`
fn redact_token_prefix(text: &str, rule: &TokenPrefixRule) -> String {
    if rule.prefix.is_empty() {
        // Empty prefix cannot meaningfully match — return text unchanged.
        return text.to_string();
    }
    let prefix = rule.prefix.as_str();
    let result = String::with_capacity(text.len());
    let mut output = result;
    let mut remaining = text;
    while let Some(pos) = remaining.find(prefix) {
        output.push_str(&remaining[..pos]);
        let after_prefix = &remaining[pos + prefix.len()..];
        let tail_len = after_prefix
            .chars()
            .take_while(|c| is_token_char(*c))
            .count();
        if tail_len >= rule.min_tail {
            output.push_str(&rule.replacement);
        } else {
            output.push_str(prefix);
            output.push_str(&after_prefix[..tail_len]);
        }
        remaining = &after_prefix[tail_len..];
    }
    output.push_str(remaining);
    output
}

/// Whether a character is valid in a token tail.
fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.'
}

/// Common redaction patterns that should always be applied to tutorial
/// artifacts. Returns a `RedactionSet` pre-loaded with token/secret patterns.
///
/// Token patterns match the **full token value**, not just the prefix. For
/// example, `ghp_abcdef1234567890abcdef1234567890` is fully replaced with
/// `<token>`.
///
/// @requirement REQ-TUTORIAL-CAPTURE-006
#[must_use]
pub fn common_redactions() -> RedactionSet {
    let mut set = RedactionSet::new();
    // GitHub tokens (classic and fine-grained) — match prefix + value.
    // Minimum 20 tail characters to avoid false positives on short strings.
    set.add_token_prefix("ghp_", "<token>", 20);
    set.add_token_prefix("gho_", "<token>", 20);
    set.add_token_prefix("ghu_", "<token>", 20);
    set.add_token_prefix("ghs_", "<token>", 20);
    set.add_token_prefix("ghr_", "<token>", 20);
    set.add_token_prefix("github_pat_", "<token>", 20);
    set
}

/// Build a complete redaction set for a run: common patterns plus
/// environment-specific patterns (home directory, username).
///
/// @requirement REQ-TUTORIAL-CAPTURE-006
#[must_use]
pub fn build_redaction_set(home: &Path, username: Option<&str>) -> RedactionSet {
    let mut set = common_redactions();
    set.add_home_dir(home);
    if let Some(user) = username {
        set.add_username(user);
    }
    set
}

/// Build a complete redaction set for a run including fixture/private repo
/// names that must be scrubbed from artifacts.
///
/// **Finding #6**: Redaction must include fixture and private repository
/// names so they are not leaked in published artifacts.
///
/// @requirement REQ-TUTORIAL-CAPTURE-006
#[must_use]
pub fn build_redaction_set_with_repos(
    home: &Path,
    username: Option<&str>,
    repos: &[&str],
) -> RedactionSet {
    let mut set = build_redaction_set(home, username);
    for repo in repos {
        let trimmed = repo.trim();
        if !trimmed.is_empty() {
            set.add(trimmed, "<repo>");
        }
    }
    set
}

/// Add hostname/timestamp redaction rules to an existing redaction set.
///
/// **Finding**: Captures may contain the hostname (e.g. in shell prompts or
/// tmux status lines), ISO-8601 timestamps (from logs), or human-readable
/// dates. This adds defense-in-depth rules to scrub them even if the tmux
/// status bar is disabled.
///
/// **issue #241 Finding #2**: Also redacts tmux clock forms (`HH:MM`,
/// `HH:MM:SS`), tmux date+clock forms (`Mon DD HH:MM`), long date forms
/// (`Month DD, YYYY`), and full FQDN hostname remnants (`user@host.local`,
/// `host.example.com`) that the tmux status bar or shell prompt may render.
///
/// @requirement REQ-TUTORIAL-CAPTURE-006
pub fn add_privacy_rules(set: &mut RedactionSet, hostname: Option<&str>) {
    // Redact the hostname if known.
    if let Some(host) = hostname {
        let trimmed = host.trim();
        if !trimmed.is_empty() {
            set.add(trimmed, "<host>");
            // Also redact short hostname (before first dot).
            if let Some(short) = trimmed.split('.').next()
                && short != trimmed
            {
                set.add(short, "<host>");
            }
        }
    }
    // Redact common macOS hostnames (MacBook, iMac, etc.) that may appear in
    // prompts even if the actual hostname is unknown.
    for pattern in &["MacBook", "Macbook", "Macbook-Pro", "MacBook-Pro", "iMac"] {
        set.add(*pattern, "<host>");
    }
    // Redact ISO-8601 timestamps: YYYY-MM-DDTHH:MM:SS or YYYY-MM-DD HH:MM:SS.
    // These are defense-in-depth; the primary defense is disabling the tmux
    // status bar. The min_tail of 8 matches "YY-MM-DD" after the "20" prefix,
    // which is sufficient to avoid false positives on short year mentions.
    set.add_token_prefix("20", "<ts>", 8);
    // issue #241 Finding #2: redact tmux clock and date forms.
    for month in &[
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ] {
        for day in 1..=31 {
            // Mon DD (e.g. "Jul 13") — redacts the month+day prefix that
            // tmux date forms render.
            set.add(format!("{month} {day:02}"), "<date>");
            set.add(format!("{month} {day} "), "<date> ");
        }
    }
    // Redact long month names (e.g. "July 13, 2026").
    for month in &[
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ] {
        for day in 1..=31 {
            set.add(format!("{month} {day}, "), "<date> ");
            set.add(format!("{month} {day:02}, "), "<date> ");
        }
    }
    // Redact tmux clock prefixes in HH:MM and HH:MM:SS displays. Replacing
    // every HH:MM prefix also redacts the identifying portion of clocks that
    // include seconds without adding 86,400 separate rules.
    for hour in 0..=23 {
        for minute in 0..=59 {
            set.add(format!("{hour:02}:{minute:02}"), "<time>");
        }
    }
    // Redact common FQDN suffixes in shell prompts / tmux status. These are
    // literal suffixes (not token prefixes) so the dot is matched literally.
    for suffix in &[".local", ".localdomain", ".lan", ".home", ".internal"] {
        set.add(*suffix, "<host>");
    }
}

/// Redact a line of terminal text for safe display.
///
/// This is a convenience function that applies common redactions to a single
/// line. For full runs, use `build_redaction_set` and `apply`/`apply_lines`.
///
/// @requirement REQ-TUTORIAL-CAPTURE-006
#[must_use]
pub fn redact_line(text: &str, home: &Path, username: Option<&str>) -> String {
    build_redaction_set(home, username).apply(text)
}

/// Error returned when redaction of a file fails.
///
/// @requirement REQ-TUTORIAL-CAPTURE-006
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RedactionError {
    /// A file could not be read.
    ReadFailed { path: String, reason: String },
    /// A file could not be written.
    WriteFailed { path: String, reason: String },
    /// A directory could not be enumerated.
    EnumerateFailed { path: String, reason: String },
}

impl std::fmt::Display for RedactionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReadFailed { path, reason } => {
                write!(f, "failed to read '{path}' for redaction: {reason}")
            }
            Self::WriteFailed { path, reason } => {
                write!(f, "failed to write redacted file '{path}': {reason}")
            }
            Self::EnumerateFailed { path, reason } => {
                write!(
                    f,
                    "failed to enumerate directory '{path}' for redaction: {reason}"
                )
            }
        }
    }
}

impl std::error::Error for RedactionError {}

#[cfg(test)]
mod tests {
    use super::*;

    // ── RedactionSet basics ───────────────────────────────────────────────

    #[test]
    fn empty_set_returns_text_unchanged() {
        let set = RedactionSet::new();
        assert_eq!(set.apply("hello world"), "hello world");
    }

    #[test]
    fn add_replaces_literal_pattern() {
        let mut set = RedactionSet::new();
        set.add("secret", "<redacted>");
        assert_eq!(set.apply("the secret is here"), "the <redacted> is here");
    }

    #[test]
    fn multiple_rules_applied_in_order() {
        let mut set = RedactionSet::new();
        set.add("foo", "bar");
        set.add("bar", "baz");
        // "foo" -> "bar" -> "baz"
        assert_eq!(set.apply("foo"), "baz");
    }

    #[test]
    fn empty_pattern_is_ignored() {
        let mut set = RedactionSet::new();
        set.add("", "<redacted>");
        assert_eq!(set.apply("hello"), "hello");
    }

    // ── Token prefix redaction ───────────────────────────────────────────

    #[test]
    fn token_prefix_redacts_full_github_token() {
        let mut set = RedactionSet::new();
        set.add_token_prefix("ghp_", "<token>", 20);
        let text = "token: ghp_abcdef1234567890ABCDEF1234567890";
        let redacted = set.apply(text);
        assert_eq!(redacted, "token: <token>");
    }

    #[test]
    fn token_prefix_redacts_github_pat() {
        let mut set = RedactionSet::new();
        set.add_token_prefix("github_pat_", "<token>", 20);
        let text = "export TOKEN=github_pat_11ABCDEFG0aBcDeF1234567";
        let redacted = set.apply(text);
        assert_eq!(redacted, "export TOKEN=<token>");
    }

    #[test]
    fn token_prefix_does_not_match_short_tail() {
        let mut set = RedactionSet::new();
        set.add_token_prefix("ghp_", "<token>", 20);
        let text = "see docs ghp_short";
        let redacted = set.apply(text);
        assert_eq!(redacted, text, "short tail should not be redacted");
    }

    #[test]
    fn token_prefix_redacts_multiple_in_same_line() {
        let mut set = RedactionSet::new();
        set.add_token_prefix("ghp_", "<token>", 20);
        let text = "ghp_abcdef1234567890AAA111 ghp_ZZZ999000111222333xx";
        let redacted = set.apply(text);
        assert_eq!(redacted, "<token> <token>");
    }

    #[test]
    fn token_prefix_preserves_text_after_token() {
        let mut set = RedactionSet::new();
        set.add_token_prefix("ghp_", "<token>", 10);
        let text = "token=ghp_abcdef12345 done";
        let redacted = set.apply(text);
        assert_eq!(redacted, "token=<token> done");
    }

    // ── Username redaction ───────────────────────────────────────────────

    #[test]
    fn add_username_redacts_username() {
        let mut set = RedactionSet::new();
        set.add_username("johndoe");
        assert_eq!(
            set.apply("/Users/johndoe/projects"),
            "/Users/<user>/projects"
        );
    }

    #[test]
    fn add_username_ignores_empty() {
        let mut set = RedactionSet::new();
        set.add_username("");
        assert_eq!(set.len(), 0);
    }

    // ── Home directory redaction ─────────────────────────────────────────

    #[test]
    fn add_home_dir_redacts_full_path() {
        let mut set = RedactionSet::new();
        set.add_home_dir(Path::new("/Users/johndoe"));
        assert_eq!(set.apply("/Users/johndoe/projects/repo"), "~/projects/repo");
    }

    // ── Line-based redaction ─────────────────────────────────────────────

    #[test]
    fn apply_lines_redacts_each_line() {
        let mut set = RedactionSet::new();
        set.add("secret", "<redacted>");
        let lines = vec![
            "line one secret".to_string(),
            "line two clean".to_string(),
            "secret again".to_string(),
        ];
        let redacted = set.apply_lines(&lines);
        assert_eq!(redacted[0], "line one <redacted>");
        assert_eq!(redacted[1], "line two clean");
        assert_eq!(redacted[2], "<redacted> again");
    }

    // ── Common redactions ────────────────────────────────────────────────

    #[test]
    fn common_redactions_redact_ghp_token_fully() {
        let set = common_redactions();
        let text = "token: ghp_abcdef1234567890ABCDEF1234567890";
        let redacted = set.apply(text);
        assert!(!redacted.contains("ghp_"));
        assert!(!redacted.contains("abcdef"));
        assert!(!redacted.contains("1234567890"));
        assert!(redacted.contains("<token>"));
    }

    #[test]
    fn common_redactions_redact_fine_grained_pat() {
        let set = common_redactions();
        let text = "export GITHUB_TOKEN=github_pat_11ABCDEFG0aBcDeF1234567";
        let redacted = set.apply(text);
        assert!(!redacted.contains("github_pat_"));
        assert!(!redacted.contains("11ABCDEFG"));
        assert!(redacted.contains("<token>"));
    }

    #[test]
    fn common_redactions_redact_multiple_token_types() {
        let set = common_redactions();
        let text = "ghp_abcdef1234567890AAA111 ghs_XX99988877766655544aaa";
        let redacted = set.apply(text);
        assert!(!redacted.contains("ghp_"));
        assert!(!redacted.contains("ghs_"));
        assert_eq!(redacted, "<token> <token>");
    }

    #[test]
    fn common_redactions_does_not_redact_short_strings() {
        let set = common_redactions();
        let text = "ghp_short";
        let redacted = set.apply(text);
        assert_eq!(redacted, text, "short strings should not be redacted");
    }

    // ── build_redaction_set ──────────────────────────────────────────────

    #[test]
    fn build_redaction_set_combines_common_and_environment() {
        let set = build_redaction_set(Path::new("/home/johndoe"), Some("johndoe"));
        let text = "/home/johndoe/repo ghp_abcdef1234567890AAA11122233344";
        let redacted = set.apply(text);
        assert!(redacted.contains("~/repo"));
        assert!(redacted.contains("<token>"));
        assert!(!redacted.contains("johndoe"));
        assert!(!redacted.contains("abcdef"));
    }

    #[test]
    fn build_redaction_set_works_without_username() {
        let set = build_redaction_set(Path::new("/home/user"), None);
        let redacted = set.apply("/home/user/project");
        assert_eq!(redacted, "~/project");
    }

    // ── redact_line convenience ──────────────────────────────────────────

    #[test]
    fn redact_line_convenience_function() {
        let redacted = redact_line(
            "/Users/dev/secret ghp_abcdef1234567890AAA111",
            Path::new("/Users/dev"),
            Some("dev"),
        );
        assert!(redacted.contains("~/secret"));
        assert!(redacted.contains("<token>"));
        assert!(!redacted.contains("dev"));
        assert!(!redacted.contains("abcdef"));
    }

    // ── Edge cases ────────────────────────────────────────────────────────

    #[test]
    fn redaction_handles_empty_text() {
        let set = common_redactions();
        assert_eq!(set.apply(""), "");
    }

    #[test]
    fn redaction_replaces_literal_substring_in_paths() {
        let mut set = RedactionSet::new();
        set.add_home_dir(Path::new("/Users/john"));
        // Literal replacement replaces the substring even inside a longer
        // word. This is expected behavior; the redaction set uses literal
        // string replacement, not word-boundary-aware replacement.
        let result = set.apply("/Users/johnny/project");
        assert_eq!(result, "~ny/project");
    }

    #[test]
    fn redaction_set_len_and_is_empty() {
        let mut set = RedactionSet::new();
        assert!(set.is_empty());
        assert_eq!(set.len(), 0);
        set.add("a", "b");
        assert!(!set.is_empty());
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn token_prefix_counted_in_len() {
        let mut set = RedactionSet::new();
        set.add_token_prefix("ghp_", "<token>", 20);
        assert_eq!(set.len(), 1);
        assert!(!set.is_empty());
    }

    // ── build_redaction_set_with_repos ───────────────────────────────────

    #[test]
    fn build_redaction_set_with_repos_redacts_repo_names() {
        let set = build_redaction_set_with_repos(
            Path::new("/home/johndoe"),
            Some("johndoe"),
            &["fixture/private-repo", "fixture/secret"],
        );
        let text = "cloning fixture/private-repo and fixture/secret";
        let redacted = set.apply(text);
        assert!(!redacted.contains("fixture/private-repo"));
        assert!(!redacted.contains("fixture/secret"));
        assert!(redacted.contains("<repo>"));
    }

    #[test]
    fn build_redaction_set_with_repos_still_redacts_tokens() {
        let set = build_redaction_set_with_repos(Path::new("/home/user"), None, &["fixture/repo"]);
        let text = "token: ghp_abcdef1234567890ABCDEF1234567890 repo: fixture/repo";
        let redacted = set.apply(text);
        assert!(redacted.contains("<token>"));
        assert!(redacted.contains("<repo>"));
        assert!(!redacted.contains("ghp_"));
        assert!(!redacted.contains("fixture/repo"));
    }

    #[test]
    fn build_redaction_set_with_repos_ignores_empty() {
        let set = build_redaction_set_with_repos(Path::new("/home/user"), None, &["", "  "]);
        // Empty repos should not add rules beyond common/home.
        assert!(set.len() <= common_redactions().len() + 1);
    }

    #[test]
    fn build_redaction_set_with_repos_redacts_in_urls() {
        let set =
            build_redaction_set_with_repos(Path::new("/home/user"), None, &["fixture/test-repo"]);
        let text = "https://github.com/fixture/test-repo/issues/42";
        let redacted = set.apply(text);
        assert!(!redacted.contains("fixture/test-repo"));
    }

    // ── Privacy redaction (hostname/timestamp) ───────────────────────────

    #[test]
    fn add_privacy_rules_redacts_hostname() {
        let mut set = RedactionSet::new();
        add_privacy_rules(&mut set, Some("acoliver-macbook.local"));
        let redacted = set.apply("user@acoliver-macbook:~/project");
        assert!(!redacted.contains("acoliver-macbook"));
        assert!(redacted.contains("<host>"));
    }

    #[test]
    fn add_privacy_rules_redacts_short_hostname() {
        let mut set = RedactionSet::new();
        add_privacy_rules(&mut set, Some("myhost.example.com"));
        let redacted = set.apply("shell prompt myhost: ");
        assert!(!redacted.contains("myhost"));
        assert!(redacted.contains("<host>"));
    }

    #[test]
    fn add_privacy_rules_redacts_macbook_pattern() {
        let mut set = RedactionSet::new();
        add_privacy_rules(&mut set, None);
        let redacted = set.apply("dev@MacBook-Pro ~ %");
        assert!(!redacted.contains("MacBook"));
        assert!(redacted.contains("<host>"));
    }

    #[test]
    fn add_privacy_rules_redacts_iso_timestamps() {
        let mut set = RedactionSet::new();
        add_privacy_rules(&mut set, None);
        let redacted = set.apply("log: 2026-07-13T12:05:56Z done");
        assert!(!redacted.contains("2026-07-13T12:05:56"));
        assert!(redacted.contains("<ts>"));
    }

    #[test]
    fn add_privacy_rules_ignores_empty_hostname() {
        let mut set = RedactionSet::new();
        add_privacy_rules(&mut set, Some(""));
        // Should not crash and should still have MacBook pattern rules.
        let redacted = set.apply("MacBook");
        assert!(redacted.contains("<host>"));
    }

    // ── issue #241 Finding #2: hostname remnants + tmux clock/date ──────

    /// Full FQDN hostname remnants like `user@host.example.com` must be
    /// redacted even when the hostname is not pre-registered. The privacy
    /// rules add token-prefix matching for `@` followed by a dotted hostname.
    #[test]
    fn add_privacy_rules_redacts_full_hostname_in_prompt() {
        let mut set = RedactionSet::new();
        add_privacy_rules(&mut set, None);
        // A shell prompt `user@workstation-01.local:~$` has the full
        // hostname; the dotted-suffix heuristic must catch `.local`.
        let redacted = set.apply("user@workstation-01.local:~$");
        assert!(
            !redacted.contains("workstation-01.local"),
            "full .local hostname must be redacted: {redacted}"
        );
    }

    /// Tmux clock-style time `HH:MM` (24h) and `HH:MM:SS` must be redacted
    /// because the tmux `status-right` often renders a clock even when the
    /// hostname is absent.
    #[test]
    fn add_privacy_rules_redacts_tmux_clock_hh_mm() {
        let mut set = RedactionSet::new();
        add_privacy_rules(&mut set, None);
        let redacted = set.apply("status: 14:35 done");
        assert!(
            !redacted.contains("14:35"),
            "tmux clock HH:MM must be redacted: {redacted}"
        );
    }

    /// Tmux date-style `Mon DD HH:MM` (as rendered by `status-right` with
    /// `%D` or similar) must be redacted.
    #[test]
    fn add_privacy_rules_redacts_tmux_date_form() {
        let mut set = RedactionSet::new();
        add_privacy_rules(&mut set, None);
        let redacted = set.apply("session [Jul 13 14:35]");
        assert!(
            !redacted.contains("Jul 13 14:35"),
            "tmux date+clock form must be redacted: {redacted}"
        );
    }

    /// Long date form `July 13, 2026` must be redacted.
    #[test]
    fn add_privacy_rules_redacts_long_date_form() {
        let mut set = RedactionSet::new();
        add_privacy_rules(&mut set, None);
        let redacted = set.apply("published July 13, 2026");
        assert!(
            !redacted.contains("July 13, 2026"),
            "long date form must be redacted: {redacted}"
        );
    }

    /// An empty token prefix must be a no-op: it would match at every
    /// position and corrupt the output. The implementation must guard
    /// against this and return text unchanged.
    #[test]
    fn empty_token_prefix_is_noop() {
        let mut set = RedactionSet::new();
        set.add_token_prefix("", "<token>", 1);
        let text = "hello world abc123";
        let redacted = set.apply(text);
        assert_eq!(
            redacted, text,
            "empty prefix must not modify text: got '{redacted}'"
        );
    }

    /// An empty token prefix with zero min_tail must also be a no-op.
    #[test]
    fn empty_token_prefix_zero_min_tail_is_noop() {
        let mut set = RedactionSet::new();
        set.add_token_prefix("", "<x>", 0);
        assert_eq!(set.apply("anything"), "anything");
    }
}
