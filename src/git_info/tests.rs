//! Tests for the git_info module — URL parsing (pure) and GitRepoInfo formatting.

use super::*;

#[test]
fn parse_ssh_url() {
    assert_eq!(
        origin_display_shortform("git@github.com:vybestack/llxprt-jefe.git"),
        Some("vybestack/llxprt-jefe".to_owned())
    );
}

#[test]
fn parse_ssh_url_no_git_suffix() {
    assert_eq!(
        origin_display_shortform("git@github.com:vybestack/llxprt-jefe"),
        Some("vybestack/llxprt-jefe".to_owned())
    );
}

#[test]
fn parse_https_url() {
    assert_eq!(
        origin_display_shortform("https://github.com/vybestack/llxprt-jefe.git"),
        Some("vybestack/llxprt-jefe".to_owned())
    );
}

#[test]
fn parse_https_url_no_git_suffix() {
    assert_eq!(
        origin_display_shortform("https://github.com/vybestack/llxprt-jefe"),
        Some("vybestack/llxprt-jefe".to_owned())
    );
}

#[test]
fn parse_ssh_with_scheme() {
    assert_eq!(
        origin_display_shortform("ssh://git@github.com/vybestack/llxprt-jefe.git"),
        Some("vybestack/llxprt-jefe".to_owned())
    );
}

#[test]
fn parse_bare_form() {
    assert_eq!(
        origin_display_shortform("vybestack/llxprt-jefe"),
        Some("vybestack/llxprt-jefe".to_owned())
    );
}

#[test]
fn parse_bare_form_with_git_suffix() {
    assert_eq!(
        origin_display_shortform("vybestack/llxprt-jefe.git"),
        Some("vybestack/llxprt-jefe".to_owned())
    );
}

#[test]
fn parse_empty_url_returns_none() {
    assert_eq!(origin_display_shortform(""), None);
    assert_eq!(origin_display_shortform("   "), None);
}

#[test]
fn parse_url_missing_repo_name_returns_none() {
    assert_eq!(origin_display_shortform("git@github.com:owner/"), None);
    assert_eq!(origin_display_shortform("https://github.com/owner/"), None);
}

#[test]
fn parse_url_missing_owner_returns_none() {
    assert_eq!(origin_display_shortform("git@github.com:/repo"), None);
}

#[test]
fn parse_url_with_extra_segments_returns_none() {
    assert_eq!(
        origin_display_shortform("https://github.com/owner/repo/extra"),
        None
    );
}

#[test]
fn list_suffix_both_present() {
    let info = GitRepoInfo {
        origin_shortform: Some("vybestack/llxprt-jefe".to_owned()),
        branch: Some("main".to_owned()),
    };
    assert_eq!(info.list_suffix(), "vybestack/llxprt-jefe @ main");
}

#[test]
fn list_suffix_only_origin() {
    let info = GitRepoInfo {
        origin_shortform: Some("vybestack/llxprt-jefe".to_owned()),
        branch: None,
    };
    assert_eq!(info.list_suffix(), "vybestack/llxprt-jefe");
}

#[test]
fn list_suffix_only_branch() {
    let info = GitRepoInfo {
        origin_shortform: None,
        branch: Some("feature-foo".to_owned()),
    };
    assert_eq!(info.list_suffix(), "@ feature-foo");
}

#[test]
fn list_suffix_neither() {
    let info = GitRepoInfo::default();
    assert_eq!(info.list_suffix(), "");
}

#[test]
fn resolve_uses_github_repo_when_set() {
    let info = GitRepoInfo::resolve("acme/widgets", false, Path::new("/nonexistent"));
    assert_eq!(info.origin_shortform.as_deref(), Some("acme/widgets"));
}

#[test]
fn resolve_trims_github_repo() {
    let info = GitRepoInfo::resolve("  acme/widgets  ", false, Path::new("/nonexistent"));
    assert_eq!(info.origin_shortform.as_deref(), Some("acme/widgets"));
}

#[test]
fn resolve_skips_branch_for_remote() {
    let info = GitRepoInfo::resolve("acme/widgets", true, Path::new("/nonexistent"));
    assert_eq!(info.origin_shortform.as_deref(), Some("acme/widgets"));
    assert!(info.branch.is_none());
}

#[test]
fn resolve_empty_github_repo_falls_back_to_git_detection() {
    // /nonexistent won't be a git repo → origin_shortform should be None.
    let info = GitRepoInfo::resolve("", false, Path::new("/nonexistent"));
    assert!(info.origin_shortform.is_none());
}

// ── parse_repository_origin: host-aware parsing (issue #190 MUST-FIX #3) ─

#[test]
fn parse_repository_origin_ssh_form() {
    assert_eq!(
        parse_repository_origin("git@github.com:acme/widgets.git"),
        Some(ParsedRepositoryOrigin {
            host: "github.com".to_owned(),
            owner_repo: "acme/widgets".to_owned(),
        })
    );
}

#[test]
fn parse_repository_origin_https_form() {
    assert_eq!(
        parse_repository_origin("https://github.com/acme/widgets.git"),
        Some(ParsedRepositoryOrigin {
            host: "github.com".to_owned(),
            owner_repo: "acme/widgets".to_owned(),
        })
    );
}

#[test]
fn parse_repository_origin_ssh_scheme_form() {
    assert_eq!(
        parse_repository_origin("ssh://git@github.com/acme/widgets.git"),
        Some(ParsedRepositoryOrigin {
            host: "github.com".to_owned(),
            owner_repo: "acme/widgets".to_owned(),
        })
    );
}

#[test]
fn parse_repository_origin_bare_form_has_empty_host() {
    assert_eq!(
        parse_repository_origin("acme/widgets"),
        Some(ParsedRepositoryOrigin {
            host: String::new(),
            owner_repo: "acme/widgets".to_owned(),
        })
    );
}

#[test]
fn parse_repository_origin_lowercases_host() {
    assert_eq!(
        parse_repository_origin("git@GitHub.COM:acme/widgets.git"),
        Some(ParsedRepositoryOrigin {
            host: "github.com".to_owned(),
            owner_repo: "acme/widgets".to_owned(),
        })
    );
}

#[test]
fn parse_repository_origin_https_uppercase_host() {
    assert_eq!(
        parse_repository_origin("https://GitHub.COM/acme/widgets.git"),
        Some(ParsedRepositoryOrigin {
            host: "github.com".to_owned(),
            owner_repo: "acme/widgets".to_owned(),
        })
    );
}

#[test]
fn parse_repository_origin_gitlab_host() {
    assert_eq!(
        parse_repository_origin("https://gitlab.com/acme/widgets.git"),
        Some(ParsedRepositoryOrigin {
            host: "gitlab.com".to_owned(),
            owner_repo: "acme/widgets".to_owned(),
        })
    );
}

#[test]
fn parse_repository_origin_attacker_host() {
    assert_eq!(
        parse_repository_origin("git@attacker.example:acme/widgets.git"),
        Some(ParsedRepositoryOrigin {
            host: "attacker.example".to_owned(),
            owner_repo: "acme/widgets".to_owned(),
        })
    );
}

#[test]
fn parse_repository_origin_no_git_suffix() {
    assert_eq!(
        parse_repository_origin("https://github.com/acme/widgets"),
        Some(ParsedRepositoryOrigin {
            host: "github.com".to_owned(),
            owner_repo: "acme/widgets".to_owned(),
        })
    );
}

#[test]
fn parse_repository_origin_strips_whitespace() {
    assert_eq!(
        parse_repository_origin("  git@github.com:acme/widgets.git  "),
        Some(ParsedRepositoryOrigin {
            host: "github.com".to_owned(),
            owner_repo: "acme/widgets".to_owned(),
        })
    );
}

#[test]
fn parse_repository_origin_empty_returns_none() {
    assert!(parse_repository_origin("").is_none());
    assert!(parse_repository_origin("   ").is_none());
}

#[test]
fn parse_repository_origin_missing_owner_returns_none() {
    assert!(parse_repository_origin("git@github.com:/widgets.git").is_none());
    assert!(parse_repository_origin("https://github.com//widgets.git").is_none());
}

#[test]
fn parse_repository_origin_missing_repo_returns_none() {
    assert!(parse_repository_origin("git@github.com:acme/").is_none());
    assert!(parse_repository_origin("https://github.com/acme/").is_none());
}

#[test]
fn parse_repository_origin_extra_segments_returns_none() {
    assert!(parse_repository_origin("https://github.com/acme/widgets/extra").is_none());
}

#[test]
fn parse_repository_origin_rejects_file_scheme() {
    // file:// reads the local filesystem, NOT a remote host. It must be
    // rejected regardless of the authority string.
    assert!(parse_repository_origin("file://github.com/acme/widgets.git").is_none());
    assert!(parse_repository_origin("file:///srv/repos/widgets.git").is_none());
}

#[test]
fn parse_repository_origin_rejects_unknown_scheme() {
    // Git supports pluggable remote helpers for arbitrary schemes; an unknown
    // scheme cannot be trusted to target the named host.
    assert!(parse_repository_origin("ftp://github.com/acme/widgets.git").is_none());
    assert!(parse_repository_origin("myhelper://github.com/acme/widgets.git").is_none());
}

#[test]
fn parse_repository_origin_scheme_is_case_insensitive() {
    // HTTPS:// and https:// are the same scheme.
    assert_eq!(
        parse_repository_origin("HTTPS://github.com/acme/widgets.git"),
        Some(ParsedRepositoryOrigin {
            host: "github.com".to_owned(),
            owner_repo: "acme/widgets".to_owned(),
        })
    );
}

#[test]
fn parse_repository_origin_accepts_git_scheme() {
    assert_eq!(
        parse_repository_origin("git://github.com/acme/widgets.git"),
        Some(ParsedRepositoryOrigin {
            host: "github.com".to_owned(),
            owner_repo: "acme/widgets".to_owned(),
        })
    );
}

#[test]
fn parse_repository_origin_https_with_port_strips_port() {
    assert_eq!(
        parse_repository_origin("https://github.com:443/acme/widgets.git"),
        Some(ParsedRepositoryOrigin {
            host: "github.com".to_owned(),
            owner_repo: "acme/widgets".to_owned(),
        })
    );
}

#[test]
fn parse_repository_origin_ipv6_literal_with_port() {
    // Bracketed IPv6 with a port: the host is the full bracketed literal and
    // the port (after ']') is stripped. This must NOT split on a colon
    // inside the address.
    assert_eq!(
        parse_repository_origin("https://[::1]:8443/acme/widgets.git"),
        Some(ParsedRepositoryOrigin {
            host: "[::1]".to_owned(),
            owner_repo: "acme/widgets".to_owned(),
        })
    );
}

#[test]
fn parse_repository_origin_ipv6_literal_without_port() {
    // Bracketed IPv6 without a port: the full bracketed address is the host.
    // A naive rfind(':') would truncate it to "[2001:db8:" — this test pins
    // the correct behavior.
    assert_eq!(
        parse_repository_origin("https://[2001:db8::1]/acme/widgets.git"),
        Some(ParsedRepositoryOrigin {
            host: "[2001:db8::1]".to_owned(),
            owner_repo: "acme/widgets".to_owned(),
        })
    );
}

#[test]
fn parse_repository_origin_ipv6_literal_is_not_github_host() {
    // An IPv6 literal is never github.com, so origins_match must reject it.
    let parsed = parse_repository_origin("https://[::1]/acme/widgets.git");
    assert!(parsed.is_some(), "IPv6 literal must parse");
    let host = parsed.map(|p| p.host);
    assert_ne!(host.as_deref(), Some("github.com"));
}
