//! Remembering which namespace an installation last used (#547 V2).
//!
//! The namespace is a pure function of the installation's state path, so under
//! a fixed build it cannot move. Builds are not fixed. Any change to how the
//! identity is computed silently repoints this process at a different
//! multiplexer server, and every agent still running on the old one becomes
//! unreachable -- not stopped, not reported, just invisible.
//!
//! Nothing can recover a session pool after its name is lost, because the name
//! is the only handle on it. The one thing that helps is noticing at the
//! moment of the change and saying so while the old name is still known. That
//! is all this module does: it writes the active namespace next to the state
//! it belongs to, and compares on the next launch.
//!
//! The record deliberately holds only derived namespaces. Recording an
//! explicit `JEFE_NAMESPACE` override would make the operator's next ordinary
//! launch look like drift, turning a temporary, deliberate isolation into a
//! permanent false alarm.

use super::namespace::{InstallationHistory, InstallationId, NamespaceDrift, NamespaceOrigin};
use std::io::Write as _;
use std::path::{Path, PathBuf};

/// File name of the namespace record, kept beside `state.json`.
const RECORD_FILE: &str = "runtime-namespace.json";

/// The on-disk shape of the record.
///
/// `state_path` is written for the operator's benefit rather than the
/// program's: someone reading this file while hunting for a lost session pool
/// should not have to guess which installation it describes.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct NamespaceRecord {
    namespace: String,
    state_path: String,
}

/// Where the record lives for a given `state.json` location.
fn record_path(state_path: &Path) -> Option<PathBuf> {
    state_path.parent().map(|dir| dir.join(RECORD_FILE))
}

/// Compare the active namespace against the recorded one, then record it.
///
/// Reporting is the caller's job; this returns what happened so that startup
/// can log it and `jefe doctor` can show it without either duplicating the
/// judgement.
///
/// An override is never recorded and never compared: it is deliberate,
/// temporary isolation, and treating it as this installation's namespace would
/// strand the operator's real sessions on the next ordinary launch.
pub fn reconcile(
    state_path: &Path,
    origin: &NamespaceOrigin,
    active: &InstallationId,
) -> NamespaceDrift {
    let drift = inspect(state_path, origin, active);
    if !matches!(drift, NamespaceDrift::Stable)
        && !origin.is_override()
        && let Some(record_path) = record_path(state_path)
    {
        write_record(&record_path, state_path, active);
    }
    drift
}

/// Assess drift without recording anything.
///
/// `jefe doctor` needs to report the situation without changing it: a
/// diagnostic that silently repaired the very thing it was asked to diagnose
/// would hide the problem from the next run.
#[must_use]
pub fn inspect(
    state_path: &Path,
    origin: &NamespaceOrigin,
    active: &InstallationId,
) -> NamespaceDrift {
    if origin.is_override() {
        return NamespaceDrift::Stable;
    }
    let Some(record_path) = record_path(state_path) else {
        return NamespaceDrift::Stable;
    };

    let history = if state_path.exists() {
        InstallationHistory::Preexisting
    } else {
        InstallationHistory::New
    };
    NamespaceDrift::assess(read_record(&record_path).as_deref(), active, history)
}

/// Read the recorded namespace, treating any unreadable record as absent.
///
/// A corrupt record is indistinguishable from no record for our purposes: in
/// both cases the previous namespace cannot be named, and that is exactly what
/// [`NamespaceDrift::PreviousNamespaceUnknown`] means.
fn read_record(path: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(path).ok()?;
    match serde_json::from_str::<NamespaceRecord>(&raw) {
        Ok(record) => Some(record.namespace),
        Err(error) => {
            tracing::warn!(
                path = %path.display(),
                %error,
                "namespace record is unreadable; treating the previous namespace as unknown"
            );
            None
        }
    }
}

/// Publish the record through a temporary file so a torn write cannot be
/// mistaken for a namespace that was never recorded.
///
/// Failure is logged, never fatal. Being unable to remember the namespace is a
/// worse diagnostic next launch, not a reason to refuse to start.
fn write_record(path: &Path, state_path: &Path, active: &InstallationId) {
    let record = NamespaceRecord {
        namespace: active.as_str().to_owned(),
        state_path: state_path.display().to_string(),
    };
    if let Err(error) = publish(path, &record) {
        tracing::warn!(
            path = %path.display(),
            %error,
            "could not record the active namespace; a later namespace change will \
             be reported as an unknown previous namespace"
        );
    }
}

fn publish(path: &Path, record: &NamespaceRecord) -> std::io::Result<()> {
    let serialized = serde_json::to_vec_pretty(record).map_err(std::io::Error::other)?;
    let temporary = path.with_extension("json.tmp");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let result = std::fs::File::create(&temporary).and_then(|mut file| {
        file.write_all(&serialized)
            .and_then(|()| file.sync_all())
            .map(|()| drop(file))
    });
    if let Err(error) = result {
        let _ = std::fs::remove_file(&temporary);
        return Err(error);
    }
    if let Err(error) = std::fs::rename(&temporary, path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error);
    }
    Ok(())
}

/// How a drift outcome should be reported to the operator.
///
/// Kept next to the reconciliation rather than at the call site so that
/// startup and `jefe doctor` cannot describe the same situation differently.
#[must_use]
pub fn describe(drift: &NamespaceDrift, active: &InstallationId) -> Option<String> {
    match drift {
        NamespaceDrift::Stable | NamespaceDrift::FirstRun => None,
        NamespaceDrift::PreviousNamespaceUnknown => Some(format!(
            "this installation has existing state but no recorded multiplexer namespace, \
             so it was last used by a build that computed namespaces differently; agents \
             still running from that build are on another namespace and will not appear \
             here. The namespace in force now is `{active}`."
        )),
        NamespaceDrift::Changed { previous } => Some(format!(
            "the multiplexer namespace for this installation changed from `{previous}` to \
             `{active}`; any agent still running under `{previous}` will not appear here \
             and must be reattached on that namespace."
        )),
    }
}
