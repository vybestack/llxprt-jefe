//! RED contract: secret/PII redaction for `jefe doctor` output (issue #264,
//! AC-09).
//!
//! `redact_value` is expected to be a pure function under `jefe::doctor` that
//! replaces sensitive substrings with stable, structural redaction markers
//! while leaving actionable labels (paths shapes, host names, tool names,
//! section headers) intact. The corpus below mirrors the sensitive categories
//! enumerated in AC-09:
//!
//! - usernames (Windows `\Username` suffix and POSIX `/home/username`);
//! - raw Windows SIDs (`S-1-5-21-...`);
//! - home directory paths;
//! - URL userinfo and embedded passwords;
//! - token / credential-shaped values (OAuth tokens, `Bearer` headers,
//!   `x-...` secret env vars, `ghp_...`/`gho_...` GitHub tokens);
//! - prompt/credential evidence (e.g. a captured `Password:` prompt).
//!
//! Every assertion checks that the sensitive fixture is absent from the
//! redacted output and that a meaningful structural label survives.

use jefe::doctor::redact_value;

/// Asserts the redacted output no longer contains the raw sensitive fixture,
/// and prints both for diagnosis on failure.
fn assert_redacted(raw: &str, fixture: &str) -> String {
    let redacted = redact_value(raw);
    assert!(
        !redacted.contains(fixture),
        "redacted output still contains the sensitive fixture {fixture:?}\n\
         raw:      {raw:?}\n\
         redacted: {redacted:?}"
    );
    assert!(
        !redacted.is_empty(),
        "redaction must preserve structure, not erase the entire value: {raw:?}"
    );
    redacted
}

#[test]
fn redacts_windows_username_in_user_home_path() {
    // `C:\Users\acoli\...` exposes the local account name.
    let raw = r"C:\Users\acoli\AppData\Local\jefe";
    let redacted = assert_redacted(raw, "acoli");
    assert!(
        redacted.contains("Users") && redacted.contains(r"\AppData"),
        "structural path and separator must survive redaction: {redacted:?}"
    );
}

#[test]
fn redacts_posix_home_username_path() {
    let raw = "/home/alice/.config/jefe";
    let redacted = assert_redacted(raw, "alice");
    assert!(
        redacted.contains("/home/") && redacted.contains("/.config"),
        "structural path and separator must survive redaction: {redacted:?}"
    );
}

#[test]
fn redacts_raw_windows_sid() {
    let raw = "owner SID: S-1-5-21-3623811015-3361044348-30300820-1001";
    let redacted = assert_redacted(raw, "S-1-5-21-3623811015-3361044348-30300820-1001");
    assert!(
        redacted.contains("SID"),
        "the structural 'SID' label must survive redaction: {redacted:?}"
    );
}

#[test]
fn redacts_home_directory_path_windows() {
    let raw = r"home: C:\Users\acoli";
    let redacted = assert_redacted(raw, r"C:\Users\acoli");
    assert!(
        redacted.contains("home"),
        "the 'home' label must survive redaction: {redacted:?}"
    );
}

#[test]
fn redacts_home_directory_path_posix() {
    let raw = "home: /home/bob";
    let redacted = assert_redacted(raw, "/home/bob");
    assert!(
        redacted.contains("home"),
        "the 'home' label must survive redaction: {redacted:?}"
    );
}

#[test]
fn redacts_url_userinfo() {
    let raw = "https://alice@github.com/acme/widgets.git";
    let redacted = assert_redacted(raw, "alice@");
    assert!(
        redacted.contains("github.com"),
        "the host must survive redaction so the origin stays actionable: {redacted:?}"
    );
}

#[test]
fn redacts_url_embedded_password() {
    let raw = "https://alice:s3cr3t@github.com/acme/widgets.git";
    let redacted = assert_redacted(raw, "s3cr3t");
    assert!(
        !redacted.contains("alice"),
        "the username must be redacted: {redacted:?}"
    );
    assert!(
        redacted.contains("[redacted]@github.com"),
        "the redacted userinfo delimiter and host must survive: {redacted:?}"
    );
}

#[test]
fn redacts_ssh_url_userinfo() {
    let raw = "git@github.com:acme/widgets.git";
    let redacted = assert_redacted(raw, "git@");
    assert!(
        redacted.contains("github.com"),
        "the host must survive SSH userinfo redaction: {redacted:?}"
    );
}

#[test]
fn redacts_bearer_token() {
    let raw = "Authorization: Bearer eyJhbGciOiJIUzI1NiJ9.payload.signature";
    let redacted = assert_redacted(raw, "eyJhbGciOiJIUzI1NiJ9.payload.signature");
    assert!(
        redacted.contains("Authorization") || redacted.contains("Bearer"),
        "the structural header label must survive redaction: {redacted:?}"
    );
}

#[test]
fn redacts_github_pat_token() {
    let raw = "token: ghp_0123456789abcdefghijklmnopqrstuvwxyzABCD";
    let redacted = assert_redacted(raw, "ghp_0123456789abcdefghijklmnopqrstuvwxyzABCD");
    assert!(
        redacted.contains("token"),
        "the 'token' label must survive redaction: {redacted:?}"
    );
}

#[test]
fn redacts_github_oauth_token() {
    let raw = "auth token gho_0123456789abcdefghijklmnopqrstuvwxyzABCD";
    let redacted = assert_redacted(raw, "gho_0123456789abcdefghijklmnopqrstuvwxyzABCD");
    assert!(
        redacted.contains("auth"),
        "the 'auth' label must survive redaction: {redacted:?}"
    );
}

#[test]
fn redacts_secret_environment_variable_value() {
    let raw = "JEFE_GITHUB_TOKEN=abcDEF1234567890ghiJKL";
    let redacted = assert_redacted(raw, "abcDEF1234567890ghiJKL");
    assert!(
        redacted.contains("JEFE_GITHUB_TOKEN"),
        "the env var name must survive redaction: {redacted:?}"
    );
}

#[test]
fn redacts_generic_long_hex_credential() {
    // A 48-char hex credential blob (not a standard commit/checksum length)
    // with no scheme should still be masked.
    let raw = "credential: 9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c";
    let redacted = assert_redacted(
        raw,
        "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c",
    );
    assert!(
        redacted.contains("credential"),
        "the 'credential' label must survive redaction: {redacted:?}"
    );
}

#[test]
fn redacts_password_prompt_evidence() {
    // A captured prompt fragment must never appear verbatim in a report.
    let raw = "gh stderr: Password: ";
    let redacted = assert_redacted(raw, "Password:");
    assert!(
        redacted.contains("gh"),
        "the tool label must survive prompt redaction: {redacted:?}"
    );
}

#[test]
fn redacts_enter_password_prompt_evidence() {
    let raw = "remote: Enter passphrase for key '/home/carol/.ssh/id_rsa': ";
    let redacted = assert_redacted(raw, "Enter passphrase for key");
    assert!(
        !redacted.contains("carol"),
        "the username embedded in the key path must also be redacted: {redacted:?}"
    );
    assert!(
        redacted.contains("/home/") && redacted.contains("/.ssh/id_rsa"),
        "the SSH key path skeleton must survive redaction: {redacted:?}"
    );
}

#[test]
fn redaction_preserves_purely_structural_content() {
    // A value with no sensitive content must pass through unchanged so the
    // report stays readable and the redactor is not over-eager.
    let raw = "multiplexer: psmux 0.9.2 at /usr/bin/psmux";
    let redacted = redact_value(raw);
    assert_eq!(
        redacted, raw,
        "non-sensitive structural content must be preserved verbatim"
    );
}

// ── Post-PR OCR hardening (issue #264): multiple occurrences, mixed
//    HTTPS+SSH contexts, userinfo boundary correctness, and legitimate
//    commit/checksum preservation. ────────────────────────────────────────

#[test]
fn redacts_every_occurrence_of_a_token_not_just_the_first() {
    // Two distinct GitHub tokens in one line must both be redacted.
    let raw = "primary ghp_0123456789abcdefghijklmnopqrstuvwxyzABCD and secondary ghp_0123456789abcdefghijklmnopqrstuvwxyzABCD";
    let redacted = redact_value(raw);
    assert!(
        !redacted.contains("ghp_0123456789abcdefghijklmnopqrstuvwxyzABCD"),
        "both token occurrences must be redacted: {redacted:?}"
    );
    assert!(
        redacted.matches("[redacted]").count() >= 2,
        "expected at least two redaction markers: {redacted:?}"
    );
}

#[test]
fn redacts_multiple_home_paths_in_one_line() {
    let raw = "config at /home/alice/.config and state at /home/bob/.local";
    let redacted = redact_value(raw);
    assert!(
        !redacted.contains("alice"),
        "alice must be redacted: {redacted:?}"
    );
    assert!(
        !redacted.contains("bob"),
        "bob must be redacted: {redacted:?}"
    );
}

#[test]
fn redacts_multiple_raw_sids_in_one_line() {
    let raw = "owner S-1-5-21-3623811015-3361044348-30300820-1001 and group S-1-5-21-3623811015-3361044348-30300820-1002";
    let redacted = redact_value(raw);
    assert!(
        !redacted.contains("S-1-5-21-3623811015-3361044348-30300820-1001"),
        "first SID must be redacted: {redacted:?}"
    );
    assert!(
        !redacted.contains("S-1-5-21-3623811015-3361044348-30300820-1002"),
        "second SID must be redacted: {redacted:?}"
    );
}

#[test]
fn mixed_https_and_ssh_contexts_both_redacted() {
    // A line with both an HTTPS URL with userinfo AND a separate SSH-style
    // reference must redact both credentials while preserving both hosts.
    let raw = "https://alice@github.com/acme/widgets.git and git@github.com:acme/widgets.git";
    let redacted = redact_value(raw);
    assert!(
        !redacted.contains("alice@"),
        "HTTPS userinfo must be redacted in mixed context: {redacted:?}"
    );
    assert!(
        !redacted.contains("git@github.com"),
        "SSH userinfo must be redacted in mixed context: {redacted:?}"
    );
    // Both host references must remain actionable.
    assert!(
        redacted.matches("github.com").count() >= 2,
        "both hosts must survive redaction: {redacted:?}"
    );
}

#[test]
fn redacts_ssh_userinfo_even_when_an_https_url_also_present() {
    // The SSH redactor must not short-circuit when a `://` scheme appears
    // elsewhere in the line.
    let raw = "origin: https://github.com/a/b.git (also git@github.com:a/b.git)";
    let redacted = redact_value(raw);
    assert!(
        !redacted.contains("git@github.com"),
        "SSH userinfo must be redacted despite co-occurring HTTPS URL: {redacted:?}"
    );
}

#[test]
fn redacts_multiple_url_userinfo_occurrences() {
    let raw = "https://alice@github.com/a and https://bob@example.com/b";
    let redacted = redact_value(raw);
    assert!(
        !redacted.contains("alice@"),
        "first URL userinfo leaked: {redacted:?}"
    );
    assert!(
        !redacted.contains("bob@"),
        "second URL userinfo leaked: {redacted:?}"
    );
    assert!(redacted.contains("github.com/a"));
    assert!(redacted.contains("example.com/b"));
}

#[test]
fn redacts_multiple_ssh_userinfo_occurrences() {
    let raw = "git@github.com:a/b.git and deploy@example.com:c/d.git";
    let redacted = redact_value(raw);
    assert!(
        !redacted.contains("git@"),
        "first SSH userinfo leaked: {redacted:?}"
    );
    assert!(
        !redacted.contains("deploy@"),
        "second SSH userinfo leaked: {redacted:?}"
    );
    assert!(redacted.contains("github.com:a/b.git"));
    assert!(redacted.contains("example.com:c/d.git"));
}

#[test]
fn redacts_multiple_bearer_tokens() {
    let raw = "Authorization: Bearer first-secret Proxy: Bearer second-secret";
    let redacted = redact_value(raw);
    assert!(!redacted.contains("first-secret"));
    assert!(!redacted.contains("second-secret"));
    assert_eq!(redacted.matches("Bearer [redacted]").count(), 2);
}

#[test]
fn multiple_at_signs_do_not_panic_or_leak_ssh_userinfo() {
    let raw = "contact@example.com then deploy@github.com:acme/widgets.git";
    let redacted = redact_value(raw);
    assert!(redacted.contains("contact@example.com"));
    assert!(!redacted.contains("deploy@github.com"));
    assert!(redacted.contains("[redacted]@github.com:acme/widgets.git"));
}

#[test]
fn preserves_sha1_commit_hash() {
    // A 40-character hex SHA-1 commit hash is legitimate diagnostic data,
    // not a credential, and must remain visible.
    let raw = "HEAD commit: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let redacted = redact_value(raw);
    assert!(
        redacted.contains("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        "a 40-char SHA-1 commit hash must not be redacted: {redacted:?}"
    );
}

#[test]
fn preserves_sha256_checksum() {
    // A 64-character hex SHA-256 checksum is legitimate diagnostic data and
    // must remain visible.
    let raw = "sha256: e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    let redacted = redact_value(raw);
    assert!(
        redacted.contains("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"),
        "a 64-char SHA-256 checksum must not be redacted: {redacted:?}"
    );
}

#[test]
fn preserves_sha1_commit_hash_with_short_label() {
    // Common diagnostic form: "commit <40-hex>".
    let raw = "commit aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let redacted = redact_value(raw);
    assert!(
        redacted.contains("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        "a commit hash after the word 'commit' must not be redacted: {redacted:?}"
    );
}
