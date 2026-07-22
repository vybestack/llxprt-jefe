//! Tests for the in-app device-code auth remediation boundary (issue #244).
//!
//! These cover the PURE helpers only: scope list, argv assembly, environment,
//! and stderr parsing of `gh auth login --web` output. The subprocess spawn
//! itself is exercised end-to-end via the tmux scenario.

use jefe::github::{
    AUTH_SCOPES, DeviceCode, build_auth_login_args, build_auth_login_env,
    is_not_authenticated_error, parse_device_code, redact_device_codes,
};

#[test]
fn auth_scopes_are_exactly_the_documented_set() {
    assert_eq!(AUTH_SCOPES, ["repo", "read:org", "gist"]);
}

#[test]
fn build_auth_login_args_assembles_noninteractive_web_flow() {
    let args = build_auth_login_args(AUTH_SCOPES);
    assert_eq!(
        args,
        [
            "auth",
            "login",
            "--hostname",
            "github.com",
            "--git-protocol",
            "https",
            "--web",
            "--scopes",
            "repo",
            "--scopes",
            "read:org",
            "--scopes",
            "gist",
        ]
        .map(String::from)
    );
}

#[test]
fn build_auth_login_args_superset_scopes_preserved() {
    // If requirements change, the builder must forward exactly what it is given
    // so the granted scopes stay auditable from the call site.
    let args = build_auth_login_args(&["repo", "read:org", "gist", "workflow"]);
    assert_eq!(
        args.iter().filter(|a| *a == "--scopes").count(),
        4,
        "one --scopes flag per requested scope"
    );
    assert!(args.contains(&"--web".to_string()));
}

#[test]
fn build_auth_login_env_sets_no_op_browser() {
    let env = build_auth_login_env();
    assert!(
        env.iter()
            .any(|(k, v)| *k == "GH_BROWSER" && *v == "/bin/true"),
        "GH_BROWSER must be a no-op so gh does not spawn a browser on a headless/remote host"
    );
}

#[test]
fn parse_device_code_extracts_code_and_url_from_real_stderr() {
    // Real `gh auth login --web` non-interactive stderr (see plan findings).
    let stderr = "! First copy your one-time code: 7701-C5F6\n\
                  Open this URL to continue in your web browser: https://github.com/login/device\n";
    let parsed = parse_device_code(stderr).unwrap_or_else(|| panic!("must parse real gh stderr"));
    assert_eq!(parsed.code, "7701-C5F6");
    assert_eq!(parsed.verification_url, "https://github.com/login/device");
}

#[test]
fn parse_device_code_strips_ansi_color_escapes() {
    // gh colorizes stderr when it believes it is a TTY; the parser must cope.
    let stderr = "\x1b[33m!\x1b[0m First copy your one-time code: \x1b[1mABCD-1234\x1b[0m\n\
                  \x1b[1mOpen this URL\x1b[0m to continue in your web browser: https://github.com/login/device\n";
    let parsed =
        parse_device_code(stderr).unwrap_or_else(|| panic!("must parse ANSI-colored stderr"));
    assert_eq!(parsed.code, "ABCD-1234");
    assert_eq!(parsed.verification_url, "https://github.com/login/device");
}

#[test]
fn parse_device_code_returns_none_without_a_code() {
    let stderr = "some unrelated gh output\n";
    assert!(parse_device_code(stderr).is_none());
}

#[test]
fn parse_device_code_returns_none_for_malformed_code() {
    // A code without the XXXX-XXXX shape is not a device code.
    let stderr = "! First copy your one-time code: not-a-code\n";
    assert!(parse_device_code(stderr).is_none());
}

#[test]
fn parse_device_code_extracts_url_even_when_embedded_in_sentence() {
    let stderr = "! First copy your one-time code: 1234-5678\n\
                  Open this URL to continue in your web browser: https://github.com/login/device/abc123\n";
    let parsed = parse_device_code(stderr).unwrap_or_else(|| panic!("must parse"));
    assert_eq!(parsed.code, "1234-5678");
    assert_eq!(
        parsed.verification_url,
        "https://github.com/login/device/abc123"
    );
}

#[test]
fn device_code_is_debug_and_eq() {
    // Sanity-check the derive so it can live in ModalState (Debug, Clone, PartialEq, Eq).
    let a = DeviceCode {
        code: "0000-0000".to_string(),
        verification_url: "https://github.com/login/device".to_string(),
    };
    let b = a.clone();
    assert_eq!(a, b);
    let _ = format!("{a:?}");
}

#[test]
fn is_not_authenticated_error_detects_gh_messages() {
    // Mirrors the substrings categorize_error() uses for NotAuthenticated,
    // so detection stays the single source of truth.
    assert!(is_not_authenticated_error(
        "gh is not authenticated. Run: gh auth login"
    ));
    assert!(is_not_authenticated_error(
        "You are not logged into any GitHub hosts."
    ));
    assert!(is_not_authenticated_error("authentication required"));
    assert!(is_not_authenticated_error("HTTP 401: unauthorized"));
}

#[test]
fn is_not_authenticated_error_rejects_unrelated_errors() {
    assert!(!is_not_authenticated_error(
        "network error: could not resolve host"
    ));
    assert!(!is_not_authenticated_error("HTTP 403: forbidden"));
    assert!(!is_not_authenticated_error("rate limit exceeded"));
    assert!(!is_not_authenticated_error("some API error"));
}

#[test]
fn redact_device_codes_replaces_code_shape() {
    assert_eq!(
        redact_device_codes("error: code WDJB-MJHT has expired"),
        "error: code <redacted> has expired"
    );
}

#[test]
fn redact_device_codes_leaves_non_code_text_intact() {
    assert_eq!(
        redact_device_codes("the device code expired"),
        "the device code expired"
    );
    // A 9-char token without the dash is not a code.
    assert_eq!(redact_device_codes("abcdefghi"), "abcdefghi");
}

#[test]
fn redact_device_codes_respects_word_boundaries() {
    // A code embedded in a longer alphanumeric token is NOT redacted.
    assert_eq!(
        redact_device_codes("token=ABCD-EFGH1234"),
        "token=ABCD-EFGH1234"
    );
    // But a standalone code at a word boundary is.
    assert_eq!(
        redact_device_codes("code ABCD-EFGH done"),
        "code <redacted> done"
    );
}

#[test]
fn redact_device_codes_handles_multiple_codes() {
    assert_eq!(
        redact_device_codes("ABCD-EFGH then WXYZ-1234"),
        "<redacted> then <redacted>"
    );
}
