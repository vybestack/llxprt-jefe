//! Shared validated target-resolution predicates for remote settings.
//!
//! This module provides the **single** contract for determining whether a
//! repository's [`RemoteRepositorySettings`] represents a valid remote target,
//! a local target, or an invalid configuration.
//!
//! All layers that need to know "is this remote?" or "is this remote config
//! complete?" delegate here:
//!
//! - **Runtime layer** (`runtime::commands::remote_is_enabled`): the tmux
//!   launch path uses [`is_valid_remote`] so a half-configured remote is
//!   treated as local (never silently sent to SSH).
//! - **App-input layer** (`app_input::target_resolution`): the availability,
//!   issue-prep, and PR-prep paths use [`resolve_target`] /
//!   [`validate_remote_settings`] which *reject* an incomplete enabled remote
//!   rather than silently treating it as local.
//!
//! Keeping the predicate in the domain layer (not `app_input`) preserves the
//! module-dependency direction: `runtime` and `app_input` both depend on
//! `domain`, never on each other.

use super::RemoteRepositorySettings;

/// Whether the settings represent a valid remote target.
///
/// Returns `true` only when `enabled == true` AND both `login_user` and
/// `host` are nonempty (after trimming) AND both fields pass SSH identity
/// validation ([`is_valid_ssh_identity`]). This is the low-level predicate
/// shared by every layer so the definition of "remote" can never drift.
#[must_use]
pub fn is_valid_remote(remote: &RemoteRepositorySettings) -> bool {
    remote.enabled
        && is_valid_ssh_identity(&remote.login_user)
        && is_valid_ssh_identity(&remote.host)
}

/// Validate a single SSH identity field (`login_user` or `host`).
///
/// This prevents SSH **destination option injection**: a malicious or
/// mistyped value that starts with `-` would be parsed by `ssh` as an option
/// (e.g. `-oProxyCommand=...`), and whitespace/control characters could
/// smuggle additional arguments past `ssh user@host` construction.
///
/// # Accepted forms
///
/// - **login_user**: POSIX username (`ubuntu`), optionally with a leading
///   dot/dash segment (`.local`, `my-user`).
/// - **host**: DNS hostname, IPv4, or bracket-free IPv6 (e.g. `::1`,
///   `fe80::1`).
///
/// The allowed character set is deliberately conservative: ASCII
/// alphanumerics, plus `-`, `.`, `_` (user/host segments), and `:` (IPv6 and
/// host:port forms). Every character must fall in this set; the field must
/// be nonempty after trimming, must not start with `-`, and must not contain
/// whitespace or control characters.
///
/// This is the **single** validation gate — every SSH command construction
/// site relies on `is_valid_remote` / `validate_remote` having run first.
/// `validate_remote` additionally validates `run_as_user` (when non-empty)
/// because it flows into `sudo -n su - <user> -c '...'`.
#[must_use]
pub fn is_valid_ssh_identity(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return false;
    }
    // Reject leading dash — ssh would interpret this as an option.
    if trimmed.starts_with('-') {
        return false;
    }
    // Reject leading '@' — a bare '@host' is not a valid user or host and
    // could confuse destination parsing.
    if trimmed.starts_with('@') {
        return false;
    }
    // Allow only safe identity characters on the trimmed value. This rejects
    // embedded whitespace, control characters, and shell metacharacters
    // (any of which could smuggle arguments past `ssh user@host`). Covers
    // POSIX usernames, DNS hostnames, IPv4, and IPv6 (including `::` and
    // `host:port`). Leading/trailing whitespace is already removed by trim
    // (mirroring the SSH construction sites), so the trimmed value must be
    // entirely safe characters.
    trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_' | ':'))
}

/// Build the user-facing error message for an incomplete remote config.
///
/// Centralized so every layer that surfaces this error uses the same wording.
#[must_use]
pub fn invalid_remote_message() -> String {
    "Remote is enabled but login_user or host is empty. \
     Disable remote or provide both login_user and host."
        .to_owned()
}

/// Build the user-facing error message for an SSH identity field that fails
/// validation (option-injection risk or invalid syntax).
#[must_use]
pub fn ssh_identity_validation_message() -> String {
    "Remote login_user or host contains invalid characters. \
     Only letters, digits, '-', '.', '_', and ':' are allowed, \
     and the field must not start with '-' or '@'."
        .to_owned()
}

/// Validate a user-supplied OpenSSH `-o` value.
///
/// Options must be one shell-free `name=value` argument. Only options that do
/// not alter Jefe-owned destination, authentication, host-key, forwarding,
/// connection-sharing, command, timeout, or terminal policies are accepted.
pub fn validate_ssh_option(option: &str) -> Result<(), String> {
    const ALLOWED_OPTIONS: &[&str] = &[
        "compression",
        "ipqos",
        "loglevel",
        "logverbose",
        "rekeylimit",
        "tcpkeepalive",
    ];

    let Some((name, value)) = option.split_once('=') else {
        return Err("SSH options must use name=value syntax".to_owned());
    };
    if name.is_empty()
        || !name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
        || !ALLOWED_OPTIONS.contains(&name.to_ascii_lowercase().as_str())
    {
        return Err(format!("SSH option {name:?} is not permitted"));
    }
    if value.is_empty() {
        return Err(format!("SSH option {name:?} requires a value"));
    }
    if value.chars().any(char::is_control) {
        return Err(format!("SSH option {name:?} contains control characters"));
    }
    Ok(())
}

/// Validate remote settings for form submission.
///
/// Returns `Ok(())` when the settings are either disabled or a complete remote
/// config (enabled + valid login_user + valid host). Returns
/// `Err(message)` when remote is enabled but incomplete or when an SSH
/// identity field fails the injection guard.
pub fn validate_remote(remote: &RemoteRepositorySettings) -> Result<(), String> {
    if !remote.enabled {
        return Ok(());
    }
    if remote.login_user.trim().is_empty() || remote.host.trim().is_empty() {
        return Err(invalid_remote_message());
    }
    if !is_valid_ssh_identity(&remote.login_user) || !is_valid_ssh_identity(&remote.host) {
        return Err(ssh_identity_validation_message());
    }
    // run_as_user is optional (empty = use login_user), but when set it flows
    // into `sudo -n su - <user> -c '...'`. Validate it against the same
    // identity rules so a malicious or mistyped value cannot inject options
    // into `su` or smuggle arguments past the shell-escaped command.
    if !remote.run_as_user.trim().is_empty() && !is_valid_ssh_identity(&remote.run_as_user) {
        return Err(ssh_identity_validation_message());
    }
    if remote.port == Some(0) {
        return Err("SSH port must be between 1 and 65535".to_owned());
    }
    for option in &remote.options {
        validate_ssh_option(option)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn remote(enabled: bool, user: &str, host: &str) -> RemoteRepositorySettings {
        RemoteRepositorySettings {
            enabled,
            login_user: user.to_owned(),
            host: host.to_owned(),
            run_as_user: String::new(),
            setup_env_default: false,
            ..RemoteRepositorySettings::default()
        }
    }

    #[test]
    fn validate_remote_rejects_zero_port_and_transport_owned_options() {
        let mut settings = remote(true, "ubuntu", "host");
        settings.port = Some(0);
        assert!(validate_remote(&settings).is_err());

        settings.port = Some(22);
        settings.options = vec!["Port=2200".to_owned()];
        assert!(validate_remote(&settings).is_err());

        settings.options = vec!["Compression=yes".to_owned()];
        assert_eq!(validate_remote(&settings), Ok(()));
    }

    #[test]
    fn is_valid_remote_false_when_disabled() {
        assert!(!is_valid_remote(&remote(false, "ubuntu", "host")));
    }

    #[test]
    fn is_valid_remote_true_when_enabled_and_complete() {
        assert!(is_valid_remote(&remote(true, "ubuntu", "host")));
    }

    #[test]
    fn is_valid_remote_false_when_enabled_but_incomplete() {
        assert!(!is_valid_remote(&remote(true, "", "host")));
        assert!(!is_valid_remote(&remote(true, "ubuntu", "")));
    }

    #[test]
    fn is_valid_remote_false_when_enabled_with_whitespace_only_fields() {
        assert!(!is_valid_remote(&remote(true, "  ", "host")));
        assert!(!is_valid_remote(&remote(true, "ubuntu", "  ")));
    }

    #[test]
    fn invalid_remote_message_mentions_fields() {
        let msg = invalid_remote_message();
        assert!(msg.contains("login_user"));
        assert!(msg.contains("host"));
    }

    // ── SSH identity validation (injection prevention) ────────────────

    #[test]
    fn ssh_identity_rejects_leading_dash() {
        // A leading dash would make ssh parse the value as an option.
        assert!(!is_valid_ssh_identity(r"-oProxyCommand=evil"));
        assert!(!is_valid_ssh_identity("-p"));
        assert!(!is_valid_ssh_identity(r"-l ubuntu"));
    }

    #[test]
    fn ssh_identity_rejects_proxycommand_injection() {
        // Classic injection: inject -oProxyCommand via the host/user field.
        assert!(!is_valid_ssh_identity(r"ubuntu -oProxyCommand=evil"));
        assert!(!is_valid_ssh_identity(r"-oProxyCommand=; rm -rf /"));
    }

    #[test]
    fn ssh_identity_rejects_whitespace() {
        // Embedded whitespace smuggles additional arguments past user@host
        // and survives trim.
        assert!(!is_valid_ssh_identity("ubuntu root"));
        assert!(!is_valid_ssh_identity("host example.com"));
        assert!(!is_valid_ssh_identity("ubuntu\tev"));
    }

    #[test]
    fn ssh_identity_rejects_control_characters() {
        // Embedded control characters survive trim and are rejected.
        assert!(!is_valid_ssh_identity("ubu\x00ntu"));
        assert!(!is_valid_ssh_identity("ho\nst"));
        assert!(!is_valid_ssh_identity("ubu\rnt"));
        assert!(!is_valid_ssh_identity("ub\tu"));
    }

    #[test]
    fn ssh_identity_rejects_shell_metacharacters() {
        assert!(!is_valid_ssh_identity("ub; rm -rf /"));
        assert!(!is_valid_ssh_identity("$(whoami)"));
        assert!(!is_valid_ssh_identity("`whoami`"));
        assert!(!is_valid_ssh_identity("ub'untu"));
        assert!(!is_valid_ssh_identity(r#"ub"untu"#));
        assert!(!is_valid_ssh_identity("ub|ntu"));
        assert!(!is_valid_ssh_identity("ub&ntu"));
        assert!(!is_valid_ssh_identity("ub>ntu"));
        assert!(!is_valid_ssh_identity("ub<ntu"));
        assert!(!is_valid_ssh_identity("ub;ntu"));
    }

    #[test]
    fn ssh_identity_rejects_leading_at() {
        assert!(!is_valid_ssh_identity("@host"));
        assert!(!is_valid_ssh_identity("@ubuntu"));
    }

    #[test]
    fn ssh_identity_rejects_empty_and_whitespace_only() {
        assert!(!is_valid_ssh_identity(""));
        assert!(!is_valid_ssh_identity("   "));
        assert!(!is_valid_ssh_identity("\t"));
    }

    #[test]
    fn ssh_identity_accepts_valid_usernames() {
        assert!(is_valid_ssh_identity("ubuntu"));
        assert!(is_valid_ssh_identity("my-user"));
        assert!(is_valid_ssh_identity("my_user"));
        assert!(is_valid_ssh_identity("user.name"));
        assert!(is_valid_ssh_identity("a"));
        assert!(is_valid_ssh_identity("1user"));
    }

    #[test]
    fn ssh_identity_accepts_valid_hostnames() {
        assert!(is_valid_ssh_identity("host"));
        assert!(is_valid_ssh_identity("build.example.com"));
        assert!(is_valid_ssh_identity("sub-domain.example.org"));
        assert!(is_valid_ssh_identity("my-host"));
        assert!(is_valid_ssh_identity("host123"));
    }

    #[test]
    fn ssh_identity_accepts_ipv4() {
        assert!(is_valid_ssh_identity("192.168.1.1"));
        assert!(is_valid_ssh_identity("10.0.0.1"));
        assert!(is_valid_ssh_identity("127.0.0.1"));
    }

    #[test]
    fn ssh_identity_accepts_ipv6() {
        assert!(is_valid_ssh_identity("::1"));
        assert!(is_valid_ssh_identity("fe80::1"));
        assert!(is_valid_ssh_identity("2001:db8::1"));
        assert!(is_valid_ssh_identity("2001:db8:85a3::8a2e:370:7334"));
    }

    #[test]
    fn ssh_identity_accepts_host_port() {
        assert!(is_valid_ssh_identity("host:2222"));
        assert!(is_valid_ssh_identity("build.example.com:22"));
    }

    #[test]
    fn is_valid_remote_rejects_proxycommand_in_login_user() {
        let bad = remote(true, r"-oProxyCommand=evil", "host");
        assert!(
            !is_valid_remote(&bad),
            "proxycommand injection in login_user must be rejected"
        );
    }

    #[test]
    fn is_valid_remote_rejects_proxycommand_in_host() {
        let bad = remote(true, "ubuntu", r"-oProxyCommand=evil");
        assert!(
            !is_valid_remote(&bad),
            "proxycommand injection in host must be rejected"
        );
    }

    #[test]
    fn is_valid_remote_rejects_injection_with_space() {
        let bad = remote(true, r"ubuntu -oProxyCommand=evil", "host");
        assert!(!is_valid_remote(&bad));
    }

    #[test]
    fn validate_remote_rejects_injection_identity() {
        let bad = remote(true, "ubuntu", r"-oProxyCommand=; rm -rf /");
        let result = validate_remote(&bad);
        assert!(result.is_err());
        let err = result.err().unwrap_or_default();
        assert!(
            err.contains("invalid characters"),
            "error should explain the rejection: {err}"
        );
    }

    #[test]
    fn validate_remote_accepts_normal_fields() {
        let good = remote(true, "ubuntu", "build.example.com");
        assert!(validate_remote(&good).is_ok());
    }

    #[test]
    fn validate_remote_accepts_ipv6_host() {
        let good = remote(true, "ubuntu", "fe80::1");
        assert!(validate_remote(&good).is_ok());
    }

    #[test]
    fn validate_remote_accepts_host_with_port() {
        let good = remote(true, "ubuntu", "build.example.com:2222");
        assert!(validate_remote(&good).is_ok());
    }

    #[test]
    fn validate_remote_disabled_passes_regardless_of_identity() {
        // A disabled remote never reaches SSH, so identity validation is moot.
        let disabled = remote(false, r"-oProxyCommand=evil", "host");
        assert!(validate_remote(&disabled).is_ok());
    }

    #[test]
    fn validate_remote_rejects_injection_in_run_as_user() {
        let mut bad = remote(true, "ubuntu", "build.example.com");
        bad.run_as_user = r"-oProxyCommand=evil".to_owned();
        assert!(
            validate_remote(&bad).is_err(),
            "proxycommand injection in run_as_user must be rejected"
        );
    }

    #[test]
    fn validate_remote_accepts_valid_run_as_user() {
        let mut good = remote(true, "ubuntu", "build.example.com");
        good.run_as_user = "deploy".to_owned();
        assert!(validate_remote(&good).is_ok());
    }
    #[test]
    fn validate_remote_rejects_proxyjump_option() {
        let mut bad = remote(true, "ubuntu", "build.example.com");
        bad.options = vec!["ProxyJump=gateway.example.com".to_owned()];
        assert!(validate_remote(&bad).is_err());
    }

    #[test]
    fn validate_remote_allows_only_non_policy_ssh_options() {
        let mut good = remote(true, "ubuntu", "build.example.com");
        good.options = vec!["Compression=yes".to_owned(), "LogLevel=ERROR".to_owned()];
        assert!(validate_remote(&good).is_ok());

        for option in [
            "UserKnownHostsFile=NUL",
            "ForwardAgent=yes",
            "CertificateFile=client-cert.pub",
            "PasswordAuthentication=yes",
            "ControlMaster=auto",
            "RequestTTY=force",
        ] {
            let mut bad = good.clone();
            bad.options = vec![option.to_owned()];
            assert!(validate_remote(&bad).is_err(), "accepted {option}");
        }
    }

    #[test]
    fn ssh_option_validation_distinguishes_value_errors() {
        assert_eq!(
            validate_ssh_option("Compression="),
            Err("SSH option \"Compression\" requires a value".to_owned())
        );
        assert_eq!(
            validate_ssh_option("Compression=yes\n"),
            Err("SSH option \"Compression\" contains control characters".to_owned())
        );
    }

    #[test]
    fn validate_remote_accepts_empty_run_as_user() {
        // Empty run_as_user is valid — it means "use login_user".
        let good = remote(true, "ubuntu", "build.example.com");
        assert!(validate_remote(&good).is_ok());
    }
}
