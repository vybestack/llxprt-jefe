//! Field normalization helpers shared by the agent creation service and the
//! state layer. These were relocated here from `state::util` so that the
//! canonical creation policy and its supporting normalization live together in
//! the app/domain boundary layer.

use crate::domain::DEFAULT_SANDBOX_FLAGS;
use std::path::Path;

/// Platform policy used for local-path validation and comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalPathPlatform {
    /// Native Windows path semantics.
    Windows,
    /// Unix path semantics.
    Unix,
}

impl LocalPathPlatform {
    /// Return the current host path policy.
    #[must_use]
    pub(crate) const fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else {
            Self::Unix
        }
    }
}

/// Compare local paths without changing either user-visible value.
#[must_use]
pub fn local_paths_equivalent(left: &Path, right: &Path) -> bool {
    local_paths_equivalent_for_platform(
        &left.to_string_lossy(),
        &right.to_string_lossy(),
        LocalPathPlatform::current(),
    )
}

#[must_use]
fn local_paths_equivalent_for_platform(
    left: &str,
    right: &str,
    platform: LocalPathPlatform,
) -> bool {
    normalize_local_path(left, platform) == normalize_local_path(right, platform)
}

fn normalize_local_path(value: &str, platform: LocalPathPlatform) -> String {
    let separated = match platform {
        LocalPathPlatform::Windows => value.replace('\\', "/").to_lowercase(),
        LocalPathPlatform::Unix => value.to_owned(),
    };
    let separated = strip_windows_device_prefix(&separated, platform);
    let (prefix, rest) = windows_drive_prefix(separated, platform);
    let rooted = rest.starts_with('/');
    let mut parts = Vec::new();
    for part in rest.split('/') {
        match part {
            ".." if parts.last().is_some_and(|last| *last != "..") => {
                parts.pop();
            }
            ".." if !rooted => parts.push(part),
            "" | "." | ".." => {}
            _ => parts.push(part),
        }
    }
    let normalized = parts.join("/");
    if !prefix.is_empty() {
        let separator = if rest.starts_with('/') { "/" } else { "" };
        format!("{prefix}{separator}{normalized}")
    } else if rooted {
        format!("/{normalized}")
    } else {
        normalized
    }
}

fn strip_windows_device_prefix(value: &str, platform: LocalPathPlatform) -> &str {
    if platform == LocalPathPlatform::Windows
        && (value.starts_with("//?/") || value.starts_with("//./"))
    {
        &value[4..]
    } else {
        value
    }
}

fn windows_drive_prefix(value: &str, platform: LocalPathPlatform) -> (&str, &str) {
    if platform == LocalPathPlatform::Windows
        && value.as_bytes().get(1) == Some(&b':')
        && value
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphabetic)
    {
        value.split_at(2)
    } else {
        ("", value)
    }
}

/// Validate that a local work directory is supported on this host.
pub fn validate_local_path(path: &Path) -> Result<(), String> {
    validate_local_path_for_platform(path, LocalPathPlatform::current())
}

fn validate_local_path_for_platform(
    path: &Path,
    platform: LocalPathPlatform,
) -> Result<(), String> {
    if platform == LocalPathPlatform::Windows {
        let value = path.to_string_lossy().replace('/', "\\");
        let lower = value.to_lowercase();
        let extended_local = lower.starts_with(r"\\?\") && !lower.starts_with(r"\\?\unc\");
        let device_local = lower.starts_with(r"\\.\") && !lower.starts_with(r"\\.\unc\");
        if value.starts_with(r"\\") && !extended_local && !device_local {
            return Err(format!(
                "UNC work directories are not supported yet: {}. Choose a path on a local drive.",
                path.display()
            ));
        }
    }
    Ok(())
}

/// Expand a leading `~` or `~/` to the current host's user-home directory.
pub fn expand_tilde(path: &str) -> String {
    expand_tilde_for_platform(
        path,
        LocalPathPlatform::current(),
        std::env::var_os("HOME").as_deref(),
        std::env::var_os("USERPROFILE").as_deref(),
    )
}

fn expand_tilde_for_platform(
    path: &str,
    platform: LocalPathPlatform,
    home: Option<&std::ffi::OsStr>,
    user_profile: Option<&std::ffi::OsStr>,
) -> String {
    let windows_suffix = if platform == LocalPathPlatform::Windows {
        path.strip_prefix(r"~\")
    } else {
        None
    };
    let suffix = path.strip_prefix("~/").or(windows_suffix);
    if path != "~" && suffix.is_none() {
        return path.to_owned();
    }
    let home = match platform {
        LocalPathPlatform::Windows => user_profile.or(home),
        LocalPathPlatform::Unix => home,
    };
    let Some(home) = home else {
        return path.to_owned();
    };
    let home = home.to_string_lossy();
    match platform {
        LocalPathPlatform::Windows => {
            let home = home.replace('/', "\\");
            let home = home.trim_end_matches('\\');
            suffix.map_or_else(
                || home.to_owned(),
                |suffix| {
                    let suffix = suffix.replace('/', "\\");
                    format!(r"{home}\{suffix}")
                },
            )
        }
        LocalPathPlatform::Unix => match suffix {
            Some(suffix) => format!("{home}/{suffix}"),
            None => home.into_owned(),
        },
    }
}

/// Normalize a profile value, treating blank input and the literal `"[]"` as
/// "no profile".
pub fn normalize_profile(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "[]" {
        String::new()
    } else {
        trimmed.to_owned()
    }
}

/// Normalize sandbox flags, defaulting to [`DEFAULT_SANDBOX_FLAGS`] when blank
/// and converting memory units to MiB otherwise.
pub fn normalize_sandbox_flags(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        DEFAULT_SANDBOX_FLAGS.to_owned()
    } else {
        normalize_memory_flag_to_mib(trimmed)
    }
}

fn normalize_memory_flag_to_mib(flags: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    for token in flags.split_whitespace() {
        if let Some(raw_mem) = token.strip_prefix("--memory=") {
            if let Some(normalized) = memory_value_to_mib(raw_mem) {
                out.push(format!("--memory={normalized}m"));
            } else {
                out.push(token.to_owned());
            }
        } else {
            out.push(token.to_owned());
        }
    }
    out.join(" ")
}

fn memory_value_to_mib(value: &str) -> Option<u64> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    let lower = trimmed.to_ascii_lowercase();
    let split_at = lower
        .find(|ch: char| !ch.is_ascii_digit())
        .unwrap_or(lower.len());
    let number_part = &lower[..split_at];
    if number_part.is_empty() {
        return None;
    }
    let numeric = number_part.parse::<u64>().ok()?;
    let unit = lower[split_at..].trim();

    match unit {
        "" | "m" | "mb" => Some(numeric),
        "g" | "gb" => Some(numeric.saturating_mul(1024)),
        "k" | "kb" => {
            let mib = numeric / 1024;
            if mib == 0 && numeric > 0 {
                None
            } else {
                Some(mib)
            }
        }
        "b" => {
            let mib = numeric / (1024 * 1024);
            if mib == 0 && numeric > 0 {
                None
            } else {
                Some(mib)
            }
        }
        _ => None,
    }
}

/// Normalize the llxprt debug field by trimming surrounding whitespace.
pub fn normalize_llxprt_debug(value: &str) -> String {
    value.trim().to_owned()
}

#[cfg(test)]
mod tests {

    #[test]
    fn windows_path_comparison_handles_case_and_separator_variants() {
        assert!(local_paths_equivalent_for_platform(
            r"C:\Users\Acoli\Repo\",
            r"c:/users/acoli/repo",
            LocalPathPlatform::Windows,
        ));
        assert!(!local_paths_equivalent_for_platform(
            r"C:\Users\Acoli\Repo",
            r"C:\Users\Acoli\Repository",
            LocalPathPlatform::Windows,
        ));
    }

    #[test]
    fn unix_path_comparison_preserves_case() {
        assert!(!local_paths_equivalent_for_platform(
            "/srv/Repo/",
            "/srv/repo",
            LocalPathPlatform::Unix,
        ));
        assert!(local_paths_equivalent_for_platform(
            "/srv/repo/",
            "/srv/repo",
            LocalPathPlatform::Unix,
        ));
    }
    #[test]
    fn windows_path_comparison_handles_unicode_case_and_components() {
        assert!(local_paths_equivalent_for_platform(
            r"C:\MÜLLER\.\Repo\child\..",
            r"c:/müller/repo",
            LocalPathPlatform::Windows,
        ));
    }

    #[test]
    fn windows_drive_anchor_survives_parent_components() {
        assert!(local_paths_equivalent_for_platform(
            r"C:\..\foo",
            r"c:\foo",
            LocalPathPlatform::Windows,
        ));
        assert!(!local_paths_equivalent_for_platform(
            r"C:\..\foo",
            r"foo",
            LocalPathPlatform::Windows,
        ));
    }

    #[test]
    fn windows_drive_relative_parent_is_not_collapsed_as_rooted() {
        assert!(!local_paths_equivalent_for_platform(
            r"C:..\foo",
            r"C:foo",
            LocalPathPlatform::Windows,
        ));
    }

    #[test]
    fn windows_extended_drive_path_matches_ordinary_drive_path() {
        assert!(local_paths_equivalent_for_platform(
            r"\\?\C:\workspace\repo",
            r"C:\workspace\repo",
            LocalPathPlatform::Windows,
        ));
    }

    #[test]
    fn windows_device_unc_path_is_rejected() {
        let error = validate_local_path_for_platform(
            std::path::Path::new(r"\\.\UNC\server\share\repo"),
            LocalPathPlatform::Windows,
        );
        assert!(error.is_err_and(|message| message.contains("UNC")));
    }

    #[test]
    fn root_is_not_equivalent_to_empty_path() {
        assert!(!local_paths_equivalent_for_platform(
            "/",
            "",
            LocalPathPlatform::Unix,
        ));
    }

    #[test]
    fn windows_unc_path_is_rejected_with_actionable_error() {
        let error = validate_local_path_for_platform(
            std::path::Path::new(r"\\server\share\repo"),
            LocalPathPlatform::Windows,
        );
        assert!(
            error.is_err_and(|message| message.contains("UNC") && message.contains("local drive")),
            "UNC rejection must explain the supported alternative"
        );
    }

    #[test]
    fn windows_extended_local_path_is_accepted() {
        assert_eq!(
            validate_local_path_for_platform(
                std::path::Path::new(r"\\?\C:\workspace\repo"),
                LocalPathPlatform::Windows,
            ),
            Ok(())
        );
    }

    #[test]
    fn windows_long_local_path_is_preserved() {
        let original = format!(r"C:\workspace\{}\repo", "long segment".repeat(30));
        assert_eq!(
            validate_local_path_for_platform(
                std::path::Path::new(&original),
                LocalPathPlatform::Windows,
            ),
            Ok(())
        );
    }
    use super::*;

    #[test]
    fn windows_tilde_uses_user_profile_when_home_is_absent() {
        assert_eq!(
            expand_tilde_for_platform(
                "~/somedir",
                LocalPathPlatform::Windows,
                None,
                Some(std::ffi::OsStr::new(r"C:\Users\Acoli Ω")),
            ),
            r"C:\Users\Acoli Ω\somedir"
        );
    }

    #[test]
    fn windows_tilde_prefers_user_profile_when_home_conflicts() {
        assert_eq!(
            expand_tilde_for_platform(
                "~/somedir",
                LocalPathPlatform::Windows,
                Some(std::ffi::OsStr::new(r"C:\CustomHome")),
                Some(std::ffi::OsStr::new(r"C:\Users\Default")),
            ),
            r"C:\Users\Default\somedir"
        );
    }

    #[test]
    fn windows_tilde_normalizes_user_profile_separators() {
        assert_eq!(
            expand_tilde_for_platform(
                "~/somedir",
                LocalPathPlatform::Windows,
                None,
                Some(std::ffi::OsStr::new("C:/Users/Acoli Ω")),
            ),
            r"C:\Users\Acoli Ω\somedir"
        );
    }

    #[test]
    fn remote_style_tilde_can_remain_verbatim_without_local_expansion() {
        assert_eq!(
            expand_tilde_for_platform("~/remote/worktrees", LocalPathPlatform::Unix, None, None),
            "~/remote/worktrees"
        );
    }

    #[test]
    fn normalize_sandbox_flags_converts_gib_to_mib() {
        assert_eq!(
            normalize_sandbox_flags("--cpus=2 --memory=12g --pids-limit=256"),
            "--cpus=2 --memory=12288m --pids-limit=256"
        );
    }

    #[test]
    fn normalize_sandbox_flags_leaves_unknown_memory_unit_unchanged() {
        assert_eq!(
            normalize_sandbox_flags("--cpus=2 --memory=12gi --pids-limit=256"),
            "--cpus=2 --memory=12gi --pids-limit=256"
        );
    }

    #[test]
    fn normalize_sandbox_flags_uses_default_when_empty() {
        assert_eq!(
            normalize_sandbox_flags("   "),
            DEFAULT_SANDBOX_FLAGS.to_owned()
        );
    }

    #[test]
    fn normalize_sandbox_flags_preserves_sub_mib_kib_values() {
        assert_eq!(
            normalize_sandbox_flags("--cpus=2 --memory=512k --pids-limit=256"),
            "--cpus=2 --memory=512k --pids-limit=256"
        );
    }

    #[test]
    fn normalize_sandbox_flags_preserves_sub_mib_byte_values() {
        assert_eq!(
            normalize_sandbox_flags("--cpus=2 --memory=500000b --pids-limit=256"),
            "--cpus=2 --memory=500000b --pids-limit=256"
        );
    }

    #[test]
    fn unix_tilde_expansion_preserves_existing_home_and_suffix_semantics() {
        let home = std::ffi::OsStr::new("/home/alice/");
        let expanded =
            expand_tilde_for_platform(r"~/work\branch", LocalPathPlatform::Unix, Some(home), None);
        assert_eq!(expanded, r"/home/alice//work\branch");
    }

    #[test]
    fn normalize_profile_treats_blank_and_brackets_as_empty() {
        assert_eq!(normalize_profile("   "), "");
        assert_eq!(normalize_profile("[]"), "");
        assert_eq!(normalize_profile("custom"), "custom");
    }

    #[test]
    fn normalize_profile_trims_surrounding_whitespace() {
        assert_eq!(normalize_profile("  custom  "), "custom");
    }

    #[test]
    fn normalize_llxprt_debug_trims_whitespace() {
        assert_eq!(normalize_llxprt_debug("  trace=1  "), "trace=1");
        assert_eq!(normalize_llxprt_debug("   "), "");
    }
}
