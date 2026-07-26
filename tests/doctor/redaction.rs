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
fn redacts_windows_username_in_sid_style_path() {
    // `C:\Users\acoli\...` exposes the local account name.
    let raw = r"C:\Users\acoli\AppData\Local\jefe";
    let redacted = assert_redacted(raw, "acoli");
    assert!(
        redacted.contains("Users"),
        "structural path label must survive redaction: {redacted:?}"
    );
}

#[test]
fn redacts_posix_home_username_path() {
    let raw = "/home/alice/.config/jefe";
    let redacted = assert_redacted(raw, "alice");
    assert!(
        redacted.contains(".config") || redacted.contains("home"),
        "structural path label must survive redaction: {redacted:?}"
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
        redacted.contains("github.com"),
        "the host must survive password redaction: {redacted:?}"
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
    // A long hex/random credential blob with no scheme should still be masked.
    let raw = "credential: 9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";
    let redacted = assert_redacted(
        raw,
        "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08",
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
