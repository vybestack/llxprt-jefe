//! Reclaiming conformance namespaces stranded by an earlier run (issue #613).
//!
//! Conformance probing brings up a throwaway namespace and tears it down when
//! the run ends. A run that never reaches its end -- a process killed outright,
//! a machine losing power mid-startup -- leaves the namespace's server running
//! with nothing left that refers to it. The server does not exit with the jefe
//! that started it and no later run revisits an old namespace, so each such
//! ending strands a pair of multiplexer servers permanently.
//!
//! Startup therefore reclaims what earlier runs left behind. The evidence is
//! the multiplexer's own registry: a conformance namespace names the PID of the
//! jefe that created it, and each namespace records the identity of the servers
//! serving it. A namespace is reclaimed only when that jefe is gone *and* a
//! recorded server is still running -- anything ambiguous is left alone, since
//! the cost of being wrong is killing a live multiplexer.

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::domain::ServerProcessIdentity;

use super::multiplexer_conformance_io::{CONFORMANCE_NAMESPACE_PREFIX, execute_probe};
use super::process::{ProcessLiveness, pid_liveness, process_liveness};
use super::{MultiplexerIsolation, MultiplexerPlan};

/// Extension of the registry entry recording a server's identity.
const SERVER_IDENTITY_EXTENSION: &str = "pid";

/// Separator between the namespace and the session in a registry entry name.
///
/// A namespace is restricted to ASCII alphanumerics and dashes, so it can never
/// contain this separator itself.
const REGISTRY_NAME_SEPARATOR: &str = "__";

/// A conformance namespace that outlived the jefe which created it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ConformanceLeftover {
    namespace: String,
    owner_pid: u32,
    servers: Vec<ServerProcessIdentity>,
}

impl ConformanceLeftover {
    /// The namespace as the multiplexer knows it.
    pub(super) fn namespace(&self) -> &str {
        &self.namespace
    }

    /// The PID of the jefe that created the namespace.
    pub(super) const fn owner_pid(&self) -> u32 {
        self.owner_pid
    }

    /// Every server identity the namespace recorded.
    pub(super) fn servers(&self) -> &[ServerProcessIdentity] {
        &self.servers
    }
}

/// What to do with one leftover namespace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LeftoverVerdict {
    /// Kill the namespace's server: its jefe is gone and the server is running.
    Reclaim,
    /// Leave the namespace alone.
    Retain,
}

/// Read the owning jefe's PID out of a conformance namespace name.
///
/// Returns `None` for anything that is not one of this runner's own namespaces,
/// which is what keeps the sweep away from real jefe namespaces: those carry a
/// workspace-derived handle rather than a PID and a run counter.
#[must_use]
pub(super) fn conformance_owner_pid(namespace: &str) -> Option<u32> {
    namespace
        .strip_prefix(CONFORMANCE_NAMESPACE_PREFIX)?
        .split_once('-')
        .filter(|(_, invocation)| invocation.parse::<u64>().is_ok())
        .and_then(|(owner, _)| owner.parse::<u32>().ok())
        .filter(|owner| *owner != 0)
}

/// Decide the fate of one leftover from the liveness of everything it names.
///
/// Reclaiming needs positive evidence on both sides. A dead owner alone would
/// kill nothing and cost a process spawn per stale registry entry; a live
/// server alone belongs to a run still in progress, possibly this one.
#[must_use]
pub(super) fn classify_leftover(
    owner: ProcessLiveness,
    servers: &[ProcessLiveness],
) -> LeftoverVerdict {
    let owner_is_gone = matches!(owner, ProcessLiveness::Dead);
    let server_is_running = servers
        .iter()
        .any(|server| matches!(server, ProcessLiveness::Alive));
    if owner_is_gone && server_is_running {
        LeftoverVerdict::Reclaim
    } else {
        LeftoverVerdict::Retain
    }
}

/// Collect the conformance namespaces `registry` still holds entries for.
///
/// An unreadable registry yields nothing rather than an error: the sweep is a
/// courtesy to a previous run, and no part of startup depends on it.
#[must_use]
pub(super) fn discover_leftovers(registry: &Path) -> Vec<ConformanceLeftover> {
    let Ok(entries) = std::fs::read_dir(registry) else {
        return Vec::new();
    };

    let mut grouped: BTreeMap<(String, u32), Vec<ServerProcessIdentity>> = BTreeMap::new();
    for entry in entries.flatten() {
        if let Some((namespace, owner_pid, server)) = recorded_server(&entry.path()) {
            grouped
                .entry((namespace, owner_pid))
                .or_default()
                .push(server);
        }
    }

    grouped
        .into_iter()
        .map(|((namespace, owner_pid), mut servers)| {
            servers.sort_by_key(|server| server.pid());
            ConformanceLeftover {
                namespace,
                owner_pid,
                servers,
            }
        })
        .collect()
}

/// Read one registry entry as a conformance namespace and its server.
///
/// Entries are named `<namespace>__<session>.<kind>`, and one namespace has
/// several sessions, so a namespace is met once per session it holds.
fn recorded_server(path: &Path) -> Option<(String, u32, ServerProcessIdentity)> {
    if path.extension().and_then(OsStr::to_str)? != SERVER_IDENTITY_EXTENSION {
        return None;
    }
    let (namespace, _session) = path
        .file_stem()
        .and_then(OsStr::to_str)?
        .split_once(REGISTRY_NAME_SEPARATOR)?;
    let owner_pid = conformance_owner_pid(namespace)?;
    let recorded = std::fs::read_to_string(path).ok()?;
    let server = parse_server_identity(&recorded)?;
    Some((namespace.to_owned(), owner_pid, server))
}

/// Parse a `<pid>:<creation discriminator>` registry record.
///
/// The creation discriminator is what makes the record safe to act on: a PID
/// the operating system has since handed to something else does not match it,
/// so the sweep cannot mistake an unrelated process for a stranded server.
fn parse_server_identity(recorded: &str) -> Option<ServerProcessIdentity> {
    let (pid, started_at) = recorded.trim().split_once(':')?;
    let pid = pid.parse::<u32>().ok().filter(|pid| *pid != 0)?;
    let started_at = started_at.parse::<u64>().ok()?;
    Some(ServerProcessIdentity::new(pid, started_at))
}

/// Where the multiplexer records the namespaces it is serving.
///
/// jefe never writes here and never removes an entry: it only reads the
/// registry to find stranded namespaces, and lets the multiplexer clean up its
/// own bookkeeping when the server it belongs to ends.
fn registry_directory() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").map(|profile| PathBuf::from(profile).join(".psmux"))
}

/// End the servers of every conformance namespace whose jefe is gone.
///
/// Best-effort by construction: every failure is logged and startup continues,
/// because a namespace that cannot be reclaimed now is exactly as reclaimable
/// on the next start.
pub(super) fn reclaim_stranded_conformance_namespaces(plan: &MultiplexerPlan) {
    // Only namespace isolation has a registry to sweep. Socket isolation
    // addresses a path the caller chose, which this runner cannot enumerate.
    if !matches!(plan.isolation(), MultiplexerIsolation::Namespace(_)) {
        return;
    }
    let Some(registry) = registry_directory() else {
        return;
    };

    for leftover in discover_leftovers(&registry) {
        if matches!(leftover_verdict(&leftover), LeftoverVerdict::Reclaim) {
            reclaim(plan, &leftover);
        }
    }
}

/// Classify one leftover against the running processes it names.
fn leftover_verdict(leftover: &ConformanceLeftover) -> LeftoverVerdict {
    let servers: Vec<ProcessLiveness> = leftover
        .servers()
        .iter()
        .map(|server| process_liveness(Some(server.identity())))
        .collect();
    classify_leftover(pid_liveness(leftover.owner_pid()), &servers)
}

/// Kill the server holding one stranded namespace.
fn reclaim(plan: &MultiplexerPlan, leftover: &ConformanceLeftover) {
    let namespace = leftover.namespace();
    let Ok(stranded) = plan.with_isolation(MultiplexerIsolation::Namespace(namespace.to_owned()))
    else {
        tracing::warn!(
            namespace,
            "stranded conformance namespace cannot be addressed"
        );
        return;
    };

    let outcome = execute_probe(&stranded, &["kill-server".to_owned()]);
    if outcome.exit_code == Some(0) {
        tracing::info!(
            namespace,
            owner = leftover.owner_pid(),
            "reclaimed a conformance namespace stranded by an earlier run"
        );
    } else {
        tracing::warn!(
            namespace,
            owner = leftover.owner_pid(),
            detail = outcome.stderr.trim(),
            "a stranded conformance namespace could not be reclaimed"
        );
    }
}
