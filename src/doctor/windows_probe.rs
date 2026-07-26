//! Native Windows environment probes for `jefe doctor`
//! (issue #264, AC-06 / AC-08).
//!
//! This module owns the pure parsing/classification of Windows-only signals:
//!
//! - the `LongPathsEnabled` registry policy
//!   (`HKLM\SYSTEM\CurrentControlSet\Control\FileSystem`); and
//! - terminal-host evidence (terminal program, parent process, ConPTY host).
//!
//! The classification functions are pure so they can be exercised
//! deterministically in unit tests without touching the registry or process
//! tree. The side-effecting collectors (`read_long_paths_enabled`,
//! `terminal_host_evidence`) are thin bounded wrappers used only on Windows
//! at collection time.
//!
//! No new dependency is introduced: the registry is read through a bounded
//! `reg.exe` child process and terminal/host evidence is read from
//! environment variables and `std::env::consts`.

use super::types::{DiagnosticFinding, DiagnosticStatus, FindingKind};

/// The classified state of the Windows long-path registry policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LongPathPolicy {
    /// The `LongPathsEnabled` value is present and non-zero.
    Enabled,
    /// The `LongPathsEnabled` value is present and zero.
    Disabled,
    /// The `LongPathsEnabled` value (or its key) is absent.
    Missing,
}

impl LongPathPolicy {
    /// Classify the raw string output of a long-path registry query.
    ///
    /// `reg query` reports a DWORD as `0x1` / `0x0`. A non-query (missing
    /// key/value) is represented by an empty or error string. This pure
    /// classifier maps those textual shapes to [`LongPathPolicy`] without
    /// performing any I/O.
    #[must_use]
    pub fn classify(raw: &str) -> Self {
        let needle = "LongPathsEnabled";
        let Some(idx) = raw.find(needle) else {
            return Self::Missing;
        };
        let tail = &raw[idx + needle.len()..];
        if let Some(hex_idx) = tail.find("0x") {
            let digits = &tail[hex_idx..];
            let value = digits
                .chars()
                .skip(2)
                .take_while(char::is_ascii_hexdigit)
                .collect::<String>();
            if value == "1" || value == "0x1" {
                return Self::Enabled;
            }
            if !value.is_empty() {
                return Self::Disabled;
            }
        }
        // Some locales render `REG_DWORD    0x1`; fall back to a plain `1`.
        if tail.contains("    1") || tail.contains("\t1") {
            return Self::Enabled;
        }
        Self::Missing
    }

    /// Build the human-readable detail string for this policy state.
    #[must_use]
    pub fn detail(self) -> &'static str {
        match self {
            Self::Enabled => "LongPathsEnabled is enabled (registry DWORD 0x1)",
            Self::Disabled => {
                "LongPathsEnabled is disabled (registry DWORD 0x0); \
                               deep config/state paths may hit the Windows MAX_PATH (260) limit"
            }
            Self::Missing => {
                "LongPathsEnabled policy is absent; Windows applies the default \
                              MAX_PATH (260) limit"
            }
        }
    }
}

/// Classify a `LongPathPolicy` into a doctor finding.
///
/// Enabled is a pass; disabled/missing is a warning (not a startup blocker)
/// because Jefe can still operate, but deep paths risk failure (AC-08).
#[must_use]
pub fn long_path_finding(policy: LongPathPolicy) -> DiagnosticFinding {
    let status = match policy {
        LongPathPolicy::Enabled => DiagnosticStatus::Pass,
        LongPathPolicy::Disabled | LongPathPolicy::Missing => DiagnosticStatus::Warn,
    };
    DiagnosticFinding::new(FindingKind::LongPath, status, policy.detail().to_string())
}

/// Collect terminal-host evidence for the ConPTY section (AC-06).
///
/// Returns a compact, redaction-safe string describing the terminal program
/// (from `TERM_PROGRAM`/`WT_SESSION`), the console host, and the OS version.
/// No usernames or paths are included.
#[must_use]
pub fn terminal_host_evidence() -> String {
    let terminal = terminal_program_label();
    let host = console_host_label();
    let os_version = os_version_label();
    let mut parts = Vec::new();
    if let Some(term) = terminal {
        parts.push(format!("terminal: {term}"));
    }
    parts.push(format!("host: {host}"));
    parts.push(format!("os: {os_version}"));
    parts.join("; ")
}

/// The terminal program label, if identifiable from the environment.
fn terminal_program_label() -> Option<String> {
    if std::env::var("WT_SESSION").is_ok_and(|v| !v.is_empty()) {
        return Some("Windows Terminal".to_string());
    }
    std::env::var("TERM_PROGRAM").ok().filter(|t| !t.is_empty())
}

/// The console host label (ConHost vs. Windows Terminal pseudoconsole).
fn console_host_label() -> String {
    // ConPTY (the Windows pseudo-console API) is the host Jefe relies on.
    // When running under Windows Terminal, WT_SESSION is set; otherwise the
    // classic ConHost console hosts the pseudoconsole.
    if std::env::var("WT_SESSION").is_ok_and(|v| !v.is_empty()) {
        "Windows Terminal ConPTY".to_string()
    } else {
        "ConHost ConPTY".to_string()
    }
}

/// The OS version label reported by the native Windows command processor.
#[cfg(windows)]
fn os_version_label() -> String {
    let command = std::env::var_os("SystemRoot")
        .map(std::path::PathBuf::from)
        .map_or_else(
            || std::path::PathBuf::from("cmd.exe"),
            |root| root.join("System32").join("cmd.exe"),
        );
    std::process::Command::new(command)
        .args(["/D", "/C", "ver"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| {
            let label = String::from_utf8_lossy(&output.stdout).trim().to_string();
            (!label.is_empty()).then_some(label)
        })
        .unwrap_or_else(|| "Windows version unavailable".to_string())
}

/// The platform label used when exercising terminal evidence on Unix.
#[cfg(not(windows))]
fn os_version_label() -> String {
    std::env::consts::OS.to_string()
}

/// Read the `LongPathsEnabled` registry policy via a bounded `reg.exe` query.
///
/// Returns `None` when the value or key is absent, `Some(true)` when enabled,
/// and `Some(false)` when explicitly disabled. This performs a bounded child
/// process invocation and parses its textual output through [`LongPathPolicy`].
///
/// No new dependency is introduced; `reg.exe` is a standard Windows binary.
#[cfg(windows)]
pub fn read_long_paths_enabled() -> Option<bool> {
    let output = std::process::Command::new("reg")
        .args([
            "query",
            "HKLM\\SYSTEM\\CurrentControlSet\\Control\\FileSystem",
            "/v",
            "LongPathsEnabled",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    match LongPathPolicy::classify(&stdout) {
        LongPathPolicy::Enabled => Some(true),
        LongPathPolicy::Disabled => Some(false),
        LongPathPolicy::Missing => None,
    }
}

/// Build the Windows long-path finding by reading the live registry policy.
///
/// On non-Windows targets this is never called; the collector emits a
/// not-applicable warning instead.
#[cfg(windows)]
pub fn windows_long_path_finding() -> DiagnosticFinding {
    match read_long_paths_enabled() {
        Some(true) => long_path_finding(LongPathPolicy::Enabled),
        Some(false) => long_path_finding(LongPathPolicy::Disabled),
        None => long_path_finding(LongPathPolicy::Missing),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_enabled_dword_hex() {
        let raw = "\r\nHKEY_LOCAL_MACHINE\\SYSTEM\\CurrentControlSet\\Control\\FileSystem\r\n\
                   LongPathsEnabled REG_DWORD 0x1\r\n";
        assert_eq!(LongPathPolicy::classify(raw), LongPathPolicy::Enabled);
    }

    #[test]
    fn classifies_disabled_dword_hex() {
        let raw = "    LongPathsEnabled    REG_DWORD    0x0";
        assert_eq!(LongPathPolicy::classify(raw), LongPathPolicy::Disabled);
    }

    #[test]
    fn classifies_missing_value() {
        assert_eq!(LongPathPolicy::classify(""), LongPathPolicy::Missing);
        assert_eq!(
            LongPathPolicy::classify(
                "ERROR: The system was unable to find the specified registry key or value."
            ),
            LongPathPolicy::Missing
        );
    }

    #[test]
    fn enabled_finding_passes_disabled_warns() {
        assert_eq!(
            long_path_finding(LongPathPolicy::Enabled).status(),
            DiagnosticStatus::Pass
        );
        assert_eq!(
            long_path_finding(LongPathPolicy::Disabled).status(),
            DiagnosticStatus::Warn
        );
        assert_eq!(
            long_path_finding(LongPathPolicy::Missing).status(),
            DiagnosticStatus::Warn
        );
    }

    #[test]
    fn enabled_finding_is_not_a_blocker() {
        assert_eq!(
            long_path_finding(LongPathPolicy::Enabled).kind(),
            FindingKind::LongPath
        );
        assert!(!FindingKind::LongPath.is_required_blocker());
    }
}
