//! Runtime diagnostic collection for `jefe doctor` (issue #264, AC-04..08).
//!
//! [`collect`] gathers a [`DoctorReport`] by probing the local host through
//! existing public primitives only:
//!
//! - version/commit/platform/architecture from the crate identity constants;
//! - the local multiplexer plan (`MultiplexerPlan`) and its version/capability
//!   preflight plus private-isolation evidence;
//! - transient ConPTY readiness on Windows via `portable-pty` (a NotApplicable
//!   informational finding elsewhere);
//! - Git and `gh`/auth via `local_command` and `GhClient::check_auth`;
//! - both agent runtimes via `agent_detection::available_agent_type_ids`;
//! - config/state writability via the read-only [`probe_persistence`];
//! - Windows long-path policy limitations.
//!
//! The collector never initializes a missing config directory, never mutates
//! settings/state, and never creates a session. If an individual probe cannot
//! be reached without widening an owner's boundary, it surfaces a truthful
//! warning or unavailable finding rather than duplicating private behaviour.

use std::path::Path;

use super::persistence_probe::{PersistenceProbeOutcome, probe_persistence};
use super::report::DoctorReport;
use super::types::{DiagnosticFinding, DiagnosticStatus, FindingKind};

/// Collect a full diagnostic report for the local host.
///
/// `config_dir` is the optional `--config <dir>` value; when `None` the
/// default platform config directory is probed. The collector is read-only
/// with respect to persistence payloads.
#[must_use]
pub fn collect(config_dir: Option<&Path>) -> DoctorReport {
    let mut findings = Vec::new();
    collect_multiplexer(&mut findings);
    collect_conpty(&mut findings);
    collect_git(&mut findings);
    collect_gh_auth(&mut findings);
    collect_agent_runtimes(&mut findings);
    collect_persistence(config_dir, &mut findings);
    collect_long_path(&mut findings);
    build_report(&findings)
}

/// Construct the report, falling back to a command-error finding if the
/// baked-in version metadata is somehow empty (it never is in a real build).
fn build_report(findings: &[DiagnosticFinding]) -> DoctorReport {
    let version = version_or_unknown();
    DoctorReport::new(
        version,
        crate::GIT_COMMIT.to_string(),
        platform_label(),
        arch_label(),
        findings.to_vec(),
    )
    .unwrap_or_else(|_error| {
        let mut fallback = vec![DiagnosticFinding::new(
            FindingKind::DiagnosticsInternal,
            DiagnosticStatus::CommandError,
            "report version metadata was empty".to_string(),
        )];
        fallback.extend_from_slice(findings);
        DoctorReport::new(
            "unknown".to_string(),
            crate::GIT_COMMIT.to_string(),
            platform_label(),
            arch_label(),
            fallback,
        )
        .unwrap_or_else(|_| DoctorReport::minimal(platform_label(), arch_label()))
    })
}

/// Return the crate version, or `"unknown"` when the baked-in value is empty.
fn version_or_unknown() -> String {
    let version = crate::VERSION;
    if version.trim().is_empty() {
        "unknown".to_string()
    } else {
        version.to_string()
    }
}

/// Probe the local multiplexer plan and its version/capability preflight.
fn collect_multiplexer(findings: &mut Vec<DiagnosticFinding>) {
    match crate::runtime::MultiplexerPlan::current() {
        Ok(plan) => {
            record_multiplexer_plan(&plan, findings);
            record_namespace_isolation(&plan, findings);
            record_namespace_drift(findings);
        }
        Err(error) => {
            findings.push(DiagnosticFinding::new(
                FindingKind::Multiplexer,
                DiagnosticStatus::Fail,
                error.to_string(),
            ));
        }
    }
}

/// Record multiplexer path/version/capability evidence from a resolved plan.
fn record_multiplexer_plan(
    plan: &crate::runtime::MultiplexerPlan,
    findings: &mut Vec<DiagnosticFinding>,
) {
    let required = [
        crate::runtime::MultiplexerCapability::AttachSession,
        crate::runtime::MultiplexerCapability::PaneCapture,
    ];
    match plan.preflight(&required) {
        Ok(version) => {
            let detail = format!(
                "path: {}; version: {}",
                plan.executable().display(),
                version
            );
            findings.push(DiagnosticFinding::new(
                FindingKind::Multiplexer,
                DiagnosticStatus::Pass,
                detail,
            ));
        }
        Err(error) => {
            findings.push(DiagnosticFinding::new(
                FindingKind::Multiplexer,
                DiagnosticStatus::Fail,
                error.to_string(),
            ));
        }
    }
}

/// Describe how the multiplexer is isolated and where that identity came from.
///
/// The rendered socket path or namespace name alone cannot tell an operator
/// why their agents are or are not visible: two installations differ only by
/// an opaque hash. Reporting the provenance alongside it turns "unknown
/// namespace" into "this namespace, derived from this state directory", which
/// is the question anyone running `jefe doctor` after a rename is actually
/// asking.
fn isolation_evidence(
    isolation: &crate::runtime::MultiplexerIsolation,
    identity: &crate::runtime::namespace::InstallationIdentity,
) -> String {
    use crate::runtime::MultiplexerIsolation;
    let rendered = match isolation {
        MultiplexerIsolation::Socket(path) => {
            format!("private socket isolation at {}", path.display())
        }
        MultiplexerIsolation::Namespace(ns) => {
            format!("private namespace isolation: {ns}")
        }
    };
    let variable = crate::runtime::installation::NAMESPACE_OVERRIDE_ENV;
    let provenance = identity.origin().state_path().map_or_else(
        || {
            format!(
                "set deliberately by {variable}, not from this installation's state directory; \
                 unset {variable} to return to the default namespace for this installation"
            )
        },
        |state_path| format!("derived from state directory {}", state_path.display()),
    );
    format!("{rendered}; {provenance}")
}

/// Record private-isolation (socket / namespace) evidence from a plan.
fn record_namespace_isolation(
    plan: &crate::runtime::MultiplexerPlan,
    findings: &mut Vec<DiagnosticFinding>,
) {
    let detail = isolation_evidence(plan.isolation(), crate::runtime::installation::current());
    let status = if plan.supports(crate::runtime::MultiplexerCapability::NamespaceIsolation)
        || plan.supports(crate::runtime::MultiplexerCapability::SocketIsolation)
    {
        DiagnosticStatus::Pass
    } else {
        DiagnosticStatus::Warn
    };
    findings.push(DiagnosticFinding::new(
        FindingKind::Namespace,
        status,
        detail,
    ));
}

/// Report a namespace that has moved away from the one this installation last
/// recorded, without disturbing the record.
///
/// Doctor exists to be run *after* something looks wrong -- typically "my
/// agents have vanished" -- so this is the place an operator is most likely to
/// see the explanation. It inspects rather than reconciles: a diagnostic that
/// quietly repaired the record would erase the evidence (issue #547).
fn record_namespace_drift(findings: &mut Vec<DiagnosticFinding>) {
    let identity = crate::runtime::installation::current();
    let drift = crate::runtime::namespace_record::inspect(
        &crate::runtime::installation::active_state_path(),
        identity.origin(),
        identity.id(),
    );
    if let Some(finding) = drift_finding(&drift, identity.id()) {
        findings.push(finding);
    }
}

/// Turn a drift assessment into a finding, or nothing if there is no news.
fn drift_finding(
    drift: &crate::runtime::namespace::NamespaceDrift,
    active: &crate::runtime::namespace::InstallationId,
) -> Option<DiagnosticFinding> {
    if !drift.is_actionable() {
        return None;
    }
    crate::runtime::namespace_record::describe(drift, active).map(|detail| {
        DiagnosticFinding::new(FindingKind::Namespace, DiagnosticStatus::Warn, detail)
    })
}

/// Probe transient ConPTY readiness on Windows; informational elsewhere.
fn collect_conpty(findings: &mut Vec<DiagnosticFinding>) {
    #[cfg(windows)]
    {
        let evidence = super::windows_probe::terminal_host_evidence();
        match probe_conpty_allocation() {
            Ok(()) => findings.push(DiagnosticFinding::new(
                FindingKind::ConPty,
                DiagnosticStatus::Pass,
                format!("transient ConPTY pseudo-console opened and released ({evidence})"),
            )),
            Err(reason) => findings.push(DiagnosticFinding::new(
                FindingKind::ConPty,
                DiagnosticStatus::Fail,
                format!("ConPTY allocation failed: {reason} ({evidence})"),
            )),
        }
    }
    #[cfg(not(windows))]
    {
        findings.push(DiagnosticFinding::new(
            FindingKind::ConPty,
            DiagnosticStatus::Warn,
            "ConPTY is a Windows-only pseudo-console; not applicable on this platform".to_string(),
        ));
    }
}

/// Open and immediately drop a transient portable-pty pair to prove ConPTY can
/// be allocated on this Windows host.
#[cfg(windows)]
fn probe_conpty_allocation() -> Result<(), String> {
    use portable_pty::{PtySize, native_pty_system};
    let system = native_pty_system();
    let pair = system
        .openpty(PtySize {
            rows: 1,
            cols: 1,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| e.to_string())?;
    drop(pair);
    Ok(())
}

/// Probe Git presence and version.
fn collect_git(findings: &mut Vec<DiagnosticFinding>) {
    match crate::local_command::command(crate::local_command::LocalTool::Git) {
        Ok(mut command) => match command.arg("--version").output() {
            Ok(output) if output.status.success() => {
                let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
                findings.push(DiagnosticFinding::new(
                    FindingKind::Git,
                    DiagnosticStatus::Pass,
                    version,
                ));
            }
            Ok(output) => findings.push(DiagnosticFinding::new(
                FindingKind::Git,
                DiagnosticStatus::Warn,
                format!(
                    "git exited non-zero: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                ),
            )),
            Err(error) => findings.push(DiagnosticFinding::new(
                FindingKind::Git,
                DiagnosticStatus::Warn,
                format!("git could not be launched: {error}"),
            )),
        },
        Err(error) => findings.push(DiagnosticFinding::new(
            FindingKind::Git,
            DiagnosticStatus::Warn,
            error.to_string(),
        )),
    }
}

/// Probe `gh` presence and authentication status.
fn collect_gh_auth(findings: &mut Vec<DiagnosticFinding>) {
    let client = crate::github::GhClient::new();
    match client.check_auth() {
        Ok(()) => findings.push(DiagnosticFinding::new(
            FindingKind::GhAuth,
            DiagnosticStatus::Pass,
            "gh authenticated".to_string(),
        )),
        Err(error) => findings.push(DiagnosticFinding::new(
            FindingKind::GhAuth,
            DiagnosticStatus::Warn,
            error.to_string(),
        )),
    }
}

/// Probe both agent runtimes via the shared detection cache.
fn collect_agent_runtimes(findings: &mut Vec<DiagnosticFinding>) {
    let installed = crate::agent_detection::available_agent_type_ids();
    for definition in crate::domain::agent_definition::AgentDefinition::shipped() {
        let status = if installed.contains(&definition.id) {
            DiagnosticStatus::Pass
        } else {
            DiagnosticStatus::Warn
        };
        let state = if status == DiagnosticStatus::Pass {
            "detected"
        } else {
            "not detected"
        };
        findings.push(DiagnosticFinding::new(
            FindingKind::DiagnosticsInternal,
            status,
            format!("{} runtime {state}", definition.display_name),
        ));
    }
}

/// Probe config/state directory writability without initializing it.
fn collect_persistence(config_dir: Option<&Path>, findings: &mut Vec<DiagnosticFinding>) {
    let resolved = config_dir.map_or_else(
        crate::persistence::default_config_dir,
        std::path::PathBuf::from,
    );
    match probe_persistence(&resolved) {
        Ok(PersistenceProbeOutcome::Writable) => findings.push(DiagnosticFinding::new(
            FindingKind::Persistence,
            DiagnosticStatus::Pass,
            format!("config directory writable: {}", resolved.display()),
        )),
        Ok(PersistenceProbeOutcome::Absent) => findings.push(DiagnosticFinding::new(
            FindingKind::Persistence,
            DiagnosticStatus::Warn,
            format!(
                "config directory absent (will be created on first run): {}",
                resolved.display()
            ),
        )),
        Ok(PersistenceProbeOutcome::NotWritable) => findings.push(DiagnosticFinding::new(
            FindingKind::Persistence,
            DiagnosticStatus::Fail,
            format!("config directory not writable: {}", resolved.display()),
        )),
        Err(error) => findings.push(DiagnosticFinding::new(
            FindingKind::Persistence,
            DiagnosticStatus::CommandError,
            format!("could not probe config directory: {error}"),
        )),
    }
    // On Windows, warn when the resolved path approaches MAX_PATH (AC-08).
    #[cfg(windows)]
    if let Some(warning) = long_path_length_warning(&resolved) {
        findings.push(DiagnosticFinding::new(
            FindingKind::LongPath,
            DiagnosticStatus::Warn,
            warning,
        ));
    }
}

/// Report Windows long-path policy limitations; informational elsewhere.
fn collect_long_path(findings: &mut Vec<DiagnosticFinding>) {
    #[cfg(windows)]
    {
        findings.push(super::windows_probe::windows_long_path_finding());
    }
    #[cfg(not(windows))]
    {
        findings.push(DiagnosticFinding::new(
            FindingKind::LongPath,
            DiagnosticStatus::Warn,
            "Windows long-path policy is not applicable on this platform".to_string(),
        ));
    }
}

/// Warn when the resolved config directory path approaches the Windows
/// MAX_PATH (260) limit. This is a read-only classification over the
/// already-resolved path; it never mutates the path.
#[cfg(windows)]
fn long_path_length_warning(path: &std::path::Path) -> Option<String> {
    const MAX_PATH: usize = 260;
    const WARN_THRESHOLD: usize = 240;
    // Windows MAX_PATH is defined in UTF-16 code units (WCHARs), not UTF-8
    // bytes. Measuring with `to_string_lossy().len()` would over-count
    // non-ASCII characters (e.g. a CJK char is 3 UTF-8 bytes but 1 WCHAR),
    // producing false-positive warnings. Count UTF-16 code units instead.
    let len = path_utf16_unit_count(path);
    if len >= MAX_PATH {
        Some(format!(
            "config directory path length {len} reaches or exceeds Windows MAX_PATH ({MAX_PATH})"
        ))
    } else if len >= WARN_THRESHOLD {
        Some(format!(
            "config directory path length {len} approaches Windows MAX_PATH ({MAX_PATH})"
        ))
    } else {
        None
    }
}

/// Count the number of UTF-16 code units required to encode `path`.
///
/// Windows path limits are expressed in WCHAR (UTF-16 code unit) counts, so
/// this is the correct unit for comparing against `MAX_PATH`. Measuring with
/// `to_string_lossy().len()` (UTF-8 bytes) would over-count non-ASCII
/// characters (e.g. a CJK character is 3 UTF-8 bytes but 1 WCHAR), producing
/// false-positive warnings. Returns 0 for the empty path.
#[cfg(any(windows, test))]
fn path_utf16_unit_count(path: &std::path::Path) -> usize {
    path.to_string_lossy().encode_utf16().count()
}

/// The platform label for the report.
fn platform_label() -> String {
    if cfg!(windows) {
        "windows".to_string()
    } else if cfg!(target_os = "macos") {
        "macos".to_string()
    } else if cfg!(target_os = "linux") {
        "linux".to_string()
    } else {
        std::env::consts::OS.to_string()
    }
}

/// The architecture label for the report.
fn arch_label() -> String {
    std::env::consts::ARCH.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_unit_count_matches_ascii_byte_length() {
        // ASCII paths have identical UTF-8 byte and UTF-16 unit counts, so the
        // helper must not inflate the length for the common case.
        let path = std::path::Path::new(r"C:\Users\someone\.config\jefe");
        assert_eq!(path_utf16_unit_count(path), path.to_string_lossy().len());
    }

    #[test]
    fn utf16_unit_count_uses_wchar_units_not_utf8_bytes() {
        // A CJK character is 3 UTF-8 bytes but 1 UTF-16 code unit. Windows
        // MAX_PATH is defined in WCHARs, so the helper must count 1 per CJK
        // character, not 3. This guards against false-positive long-path
        // warnings for non-ASCII profile directories.
        let path = std::path::Path::new("/home/\u{4e2d}\u{6587}/jefe");
        let utf16_units = path_utf16_unit_count(path);
        let utf8_bytes = path.to_string_lossy().len();
        assert!(
            utf16_units < utf8_bytes,
            "UTF-16 unit count ({utf16_units}) must be smaller than the UTF-8 byte count ({utf8_bytes}) for non-ASCII paths"
        );
        // The two CJK characters contribute 2 units, not 6 bytes.
        assert_eq!(utf16_units, utf8_bytes - 4);
    }

    /// A namespace that moved must be reported, and must name the namespace it
    /// moved away from: that name is the only route back to agents still
    /// running under it.
    #[test]
    fn drift_is_reported_with_the_namespace_that_was_left_behind() {
        use crate::runtime::namespace::{InstallationId, NamespaceDrift};

        let active = InstallationId::for_state_path(std::path::Path::new("/home/dev/state.json"));
        let finding = drift_finding(
            &NamespaceDrift::Changed {
                previous: "jefe-76134a0ba22f56e9".to_owned(),
            },
            &active,
        )
        .unwrap_or_else(|| panic!("a namespace change must produce a finding"));

        assert_eq!(finding.status(), DiagnosticStatus::Warn);
        assert!(
            finding.detail().contains("jefe-76134a0ba22f56e9"),
            "the abandoned namespace must be named, got: {}",
            finding.detail()
        );
    }

    /// A steady installation must not manufacture a warning.
    #[test]
    fn a_stable_namespace_produces_no_finding() {
        use crate::runtime::namespace::{InstallationId, NamespaceDrift};

        let active = InstallationId::for_state_path(std::path::Path::new("/home/dev/state.json"));

        assert!(drift_finding(&NamespaceDrift::Stable, &active).is_none());
        assert!(drift_finding(&NamespaceDrift::FirstRun, &active).is_none());
    }

    #[test]
    fn isolation_evidence_names_the_state_path_the_namespace_came_from() {
        // An operator diagnosing "why is this agent not in my session list"
        // needs to know which installation the namespace was derived from.
        // Reporting the opaque hash alone cannot answer that question, so the
        // originating state path has to travel with it.
        let state_path = std::path::Path::new("/home/someone/.local/state/jefe");
        let identity = crate::runtime::namespace::InstallationIdentity::for_state_path(state_path);
        let detail = isolation_evidence(
            &crate::runtime::MultiplexerIsolation::Namespace(identity.id().as_str().to_owned()),
            &identity,
        );

        assert!(
            detail.contains(identity.id().as_str()),
            "the active namespace must be visible, got: {detail}"
        );
        assert!(
            detail.contains("state"),
            "the originating state path must be visible, got: {detail}"
        );
        assert!(
            detail.contains("derived"),
            "a derived namespace must say so, got: {detail}"
        );
    }

    #[test]
    fn isolation_evidence_calls_out_a_deliberate_override() {
        // An overridden namespace is the one case where the operator has
        // deliberately stepped outside the per-installation default, so the
        // report must not present it as if it followed from the state path.
        let identity = crate::runtime::namespace::InstallationIdentity::from_override("ab-testing")
            .unwrap_or_else(|error| panic!("a plain override should be accepted: {error}"));
        let detail = isolation_evidence(
            &crate::runtime::MultiplexerIsolation::Namespace("ab-testing".to_owned()),
            &identity,
        );

        assert!(
            detail.contains("JEFE_NAMESPACE"),
            "the operator must be told which variable is steering them, got: {detail}"
        );
        assert!(
            !detail.contains("derived from state directory"),
            "an override must not be attributed to a state directory, got: {detail}"
        );
    }
}
