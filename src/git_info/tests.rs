//! Tests for the git_info module — URL parsing (pure) and GitRepoInfo formatting.

use super::*;

#[test]
fn parse_ssh_url() {
    assert_eq!(
        parse_origin_url("git@github.com:vybestack/llxprt-jefe.git"),
        Some("vybestack/llxprt-jefe".to_owned())
    );
}

#[test]
fn parse_ssh_url_no_git_suffix() {
    assert_eq!(
        parse_origin_url("git@github.com:vybestack/llxprt-jefe"),
        Some("vybestack/llxprt-jefe".to_owned())
    );
}

#[test]
fn parse_https_url() {
    assert_eq!(
        parse_origin_url("https://github.com/vybestack/llxprt-jefe.git"),
        Some("vybestack/llxprt-jefe".to_owned())
    );
}

#[test]
fn parse_https_url_no_git_suffix() {
    assert_eq!(
        parse_origin_url("https://github.com/vybestack/llxprt-jefe"),
        Some("vybestack/llxprt-jefe".to_owned())
    );
}

#[test]
fn parse_ssh_with_scheme() {
    assert_eq!(
        parse_origin_url("ssh://git@github.com/vybestack/llxprt-jefe.git"),
        Some("vybestack/llxprt-jefe".to_owned())
    );
}

#[test]
fn parse_bare_form() {
    assert_eq!(
        parse_origin_url("vybestack/llxprt-jefe"),
        Some("vybestack/llxprt-jefe".to_owned())
    );
}

#[test]
fn parse_bare_form_with_git_suffix() {
    assert_eq!(
        parse_origin_url("vybestack/llxprt-jefe.git"),
        Some("vybestack/llxprt-jefe".to_owned())
    );
}

#[test]
fn parse_empty_url_returns_none() {
    assert_eq!(parse_origin_url(""), None);
    assert_eq!(parse_origin_url("   "), None);
}

#[test]
fn parse_url_missing_repo_name_returns_none() {
    assert_eq!(parse_origin_url("git@github.com:owner/"), None);
    assert_eq!(parse_origin_url("https://github.com/owner/"), None);
}

#[test]
fn parse_url_missing_owner_returns_none() {
    assert_eq!(parse_origin_url("git@github.com:/repo"), None);
}

#[test]
fn parse_url_with_extra_segments_returns_none() {
    assert_eq!(
        parse_origin_url("https://github.com/owner/repo/extra"),
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
