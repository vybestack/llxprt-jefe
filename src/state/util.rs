//! Text-editing utility functions and field normalization helpers.

use crate::domain::DEFAULT_SANDBOX_FLAGS;

pub(super) fn generate_id(prefix: &str) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!("{prefix}-{timestamp:x}")
}

pub(super) fn expand_tilde(path: &str) -> String {
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

pub(super) fn normalize_profile(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "[]" {
        String::new()
    } else {
        value.to_owned()
    }
}

pub(super) fn normalize_sandbox_flags(value: &str) -> String {
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

pub(super) fn normalize_llxprt_debug(value: &str) -> String {
    value.trim().to_owned()
}

pub(super) fn clamp_cursor(s: &str, cursor: usize) -> usize {
    cursor.min(s.chars().count())
}

pub(super) fn byte_index_at_char(s: &str, char_idx: usize) -> usize {
    if char_idx == 0 {
        return 0;
    }

    s.char_indices()
        .nth(char_idx)
        .map_or_else(|| s.len(), |(idx, _)| idx)
}

pub(super) fn insert_char_at(s: &mut String, cursor: usize, ch: char) -> usize {
    let clamped = clamp_cursor(s, cursor);
    let byte_idx = byte_index_at_char(s, clamped);
    s.insert(byte_idx, ch);
    clamped + 1
}

pub(super) fn delete_char_before(s: &mut String, cursor: usize) -> usize {
    let clamped = clamp_cursor(s, cursor);
    if clamped == 0 {
        return 0;
    }

    let start = byte_index_at_char(s, clamped - 1);
    let end = byte_index_at_char(s, clamped);
    s.replace_range(start..end, "");
    clamped - 1
}

pub(super) fn delete_char_at(s: &mut String, cursor: usize) {
    let clamped = clamp_cursor(s, cursor);
    let len = s.chars().count();
    if clamped >= len {
        return;
    }

    let start = byte_index_at_char(s, clamped);
    let end = byte_index_at_char(s, clamped + 1);
    s.replace_range(start..end, "");
}

pub(super) fn move_cursor_left(cursor: usize) -> usize {
    cursor.saturating_sub(1)
}

pub(super) fn move_cursor_right(s: &str, cursor: usize) -> usize {
    let len = s.chars().count();
    clamp_cursor(s, cursor).saturating_add(1).min(len)
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
}
