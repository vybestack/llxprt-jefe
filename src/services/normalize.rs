//! Field normalization helpers shared by the agent creation service and the
//! state layer. These were relocated here from `state::util` so that the
//! canonical creation policy and its supporting normalization live together in
//! the app/domain boundary layer.

use crate::domain::DEFAULT_SANDBOX_FLAGS;

/// Expand a leading `~` or `~/` to the user's home directory.
pub fn expand_tilde(path: &str) -> String {
    if (path == "~" || path.starts_with("~/"))
        && let Some(home) = std::env::var_os("HOME")
    {
        let home = home.to_string_lossy();
        return if path == "~" {
            home.into_owned()
        } else {
            format!("{home}{}", &path[1..])
        };
    }
    path.to_owned()
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
    use super::*;

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
