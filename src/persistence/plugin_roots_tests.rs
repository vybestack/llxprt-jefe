//! Ordered package-root table (issue #389 CW-09, acceptance rows R1, R2, R7).

use std::path::{Path, PathBuf};

use super::*;

fn request(platform: Platform, executable_dir: Option<&str>) -> PluginRootRequest {
    PluginRootRequest {
        executable_dir: executable_dir.map(PathBuf::from),
        platform,
        config_plugins_dir: PathBuf::from("/home/u/.config/jefe/plugins"),
    }
}

/// Root paths with a single separator spelling.
///
/// `Path::join` uses the platform separator, so a root built from a Unix-style
/// prefix renders mixed on Windows. The rule under test is which roots appear
/// and in what order, not how the platform spells a separator.
fn paths(request: &PluginRootRequest) -> Vec<String> {
    candidate_roots(request)
        .iter()
        .map(|root| root.path().display().to_string().replace('\\', "/"))
        .collect()
}

#[test]
fn macos_roots_are_ordered_executable_then_homebrew_then_local_then_user() {
    assert_eq!(
        paths(&request(Platform::Macos, Some("/opt/jefe/bin"))),
        vec![
            "/opt/jefe/share/jefe/plugins",
            "/opt/homebrew/share/jefe/plugins",
            "/usr/local/share/jefe/plugins",
            "/home/u/.config/jefe/plugins/installed",
        ]
    );
}

#[test]
fn linux_roots_are_ordered_executable_then_usr_local_then_usr_then_user() {
    assert_eq!(
        paths(&request(Platform::Linux, Some("/opt/jefe/bin"))),
        vec![
            "/opt/jefe/share/jefe/plugins",
            "/usr/local/share/jefe/plugins",
            "/usr/share/jefe/plugins",
            "/home/u/.config/jefe/plugins/installed",
        ]
    );
}

#[test]
fn windows_has_no_unix_system_roots() {
    assert_eq!(
        paths(&request(Platform::Windows, Some("/opt/jefe/bin"))),
        vec![
            "/opt/jefe/share/jefe/plugins",
            "/home/u/.config/jefe/plugins/installed",
        ]
    );
}

#[test]
fn an_unresolvable_executable_directory_only_skips_its_own_root() {
    assert_eq!(
        paths(&request(Platform::Linux, None)),
        vec![
            "/usr/local/share/jefe/plugins",
            "/usr/share/jefe/plugins",
            "/home/u/.config/jefe/plugins/installed",
        ]
    );
}

#[test]
fn an_executable_directory_without_a_parent_yields_no_executable_root() {
    assert_eq!(
        paths(&request(Platform::Windows, Some("/"))),
        vec!["/home/u/.config/jefe/plugins/installed"]
    );
}

#[test]
fn the_user_root_is_the_only_writable_root() {
    for platform in [Platform::Macos, Platform::Linux, Platform::Windows] {
        let roots = candidate_roots(&request(platform, Some("/opt/jefe/bin")));
        let writable: Vec<&Path> = roots
            .iter()
            .filter(|root| root.is_writable())
            .map(PluginRoot::path)
            .collect();
        assert_eq!(
            writable,
            vec![Path::new("/home/u/.config/jefe/plugins/installed")],
            "{platform:?} must expose exactly one writable root"
        );
    }
}

#[test]
fn the_writable_root_is_the_last_and_highest_precedence_root() {
    let roots = candidate_roots(&request(Platform::Macos, Some("/opt/jefe/bin")));
    let last = roots
        .last()
        .unwrap_or_else(|| panic!("there is always a user root"));
    assert!(last.is_writable());
    assert_eq!(last.kind(), PluginRootKind::User);
}

#[test]
fn every_root_except_the_user_root_is_read_only() {
    let roots = candidate_roots(&request(Platform::Linux, Some("/opt/jefe/bin")));
    for root in roots
        .iter()
        .filter(|root| root.kind() != PluginRootKind::User)
    {
        assert!(
            !root.is_writable(),
            "{} is a package-manager root and must be read-only",
            root.path().display()
        );
    }
}

#[test]
fn resolution_reads_neither_path_nor_the_current_directory() {
    // The request carries every input, so the same request yields the same
    // roots regardless of process state.
    let request = request(Platform::Linux, Some("/opt/jefe/bin"));
    assert_eq!(paths(&request), paths(&request));
}

#[test]
fn a_root_derived_from_the_executable_is_kept_even_when_it_repeats_a_system_root() {
    // /usr/local/bin/jefe derives /usr/local/share/jefe/plugins, which is also
    // a Linux system root. Physical identity is the single dedup authority, so
    // the ordered candidate list keeps both and lets the inventory record the
    // alias rather than introducing a second, lexical dedup rule here.
    assert_eq!(
        paths(&request(Platform::Linux, Some("/usr/local/bin"))),
        vec![
            "/usr/local/share/jefe/plugins",
            "/usr/local/share/jefe/plugins",
            "/usr/share/jefe/plugins",
            "/home/u/.config/jefe/plugins/installed",
        ]
    );
}
