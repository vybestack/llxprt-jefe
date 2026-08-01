//! JSP launch coordinator, bootstrap material, and production host runtime.
//!
//! This module owns the lifecycle authority that reserves per-agent
//! credentials, creates owner-only bootstrap files, and drives the
//! production-owned host worker thread. It depends on the registry and host
//! types from the parent module but owns no protocol-state mutation itself.

use std::collections::HashMap;
use std::io::Write;
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use super::{JspHost, JspHostError, MAX_PUBLISHERS, PublisherRegistry};
use crate::domain::AgentId;
use crate::messages::RuntimeMessage;

/// Atomically create owner-only bootstrap material for an authorized local
/// launch. Callers supply a cryptographically random opaque credential.
pub fn create_bootstrap(
    runtime_dir: &Path,
    endpoint: SocketAddr,
    reservation: &super::PublisherReservation,
) -> Result<BootstrapMaterial, JspHostError> {
    if !endpoint.ip().is_loopback()
        || reservation.registration_id.is_empty()
        || reservation.publisher_credential.len() < 32
    {
        return Err(JspHostError::Forbidden);
    }
    std::fs::create_dir_all(runtime_dir)?;
    set_owner_only_dir(runtime_dir)?;
    let final_path = runtime_dir.join(format!(
        "jsp-{}-{}.json",
        reservation.agent_id.0, reservation.generation
    ));
    let temp_path = runtime_dir.join(format!(
        ".jsp-{}-{}.tmp",
        reservation.agent_id.0, reservation.generation
    ));
    let document = serde_json::json!({
        "schema": 1,
        "protocol": "jsp/1",
        "endpoint": format!("http://{endpoint}/jsp/1"),
        "registration_id": reservation.registration_id,
        "publisher_credential": reservation.publisher_credential,
        "agent_id": reservation.agent_id.0,
        "lifecycle_generation": reservation.generation,
    });
    let bytes = serde_json::to_vec(&document).map_err(|_| JspHostError::InvalidRequest)?;
    let result = (|| {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        set_owner_only_file_options(&mut options);
        let mut file = options.open(&temp_path)?;
        set_owner_only_file(&temp_path)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        std::fs::rename(&temp_path, &final_path)?;
        Ok(BootstrapMaterial { path: final_path })
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp_path);
    }
    result
}

/// Owner-only bootstrap file whose cleanup is explicit and idempotent.
#[derive(Debug)]
pub struct BootstrapMaterial {
    path: PathBuf,
}

impl BootstrapMaterial {
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Remove this launch's bootstrap material only.
    pub fn cleanup(&self) -> Result<(), JspHostError> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}

/// Attach the bootstrap path only to a fresh local LLxprt plan. Remote and
/// resume/reattach paths remain explicitly unsupported and receive no secret.

#[derive(Debug)]
struct ActiveLaunch {
    generation: u64,
    bootstrap: BootstrapMaterial,
}

#[derive(Debug, Clone, Default)]
pub(super) struct ObservationDelivery {
    pending: Arc<Mutex<HashMap<AgentId, RuntimeMessage>>>,
}

impl ObservationDelivery {
    pub(super) fn publish(&self, message: RuntimeMessage) -> Result<(), JspHostError> {
        let agent_id = match &message {
            RuntimeMessage::ObservationUpdated(agent_id, _, _)
            | RuntimeMessage::ObservationCleared(agent_id, _) => agent_id.clone(),
            RuntimeMessage::KillAgent(_)
            | RuntimeMessage::RelaunchAgent(_)
            | RuntimeMessage::RestartAgent(_)
            | RuntimeMessage::AgentStatusChanged(_, _) => return Err(JspHostError::InvalidRequest),
        };
        let mut pending = self.pending.lock().map_err(|_| JspHostError::Poisoned)?;
        if pending.len() >= MAX_PUBLISHERS && !pending.contains_key(&agent_id) {
            return Err(JspHostError::Capacity);
        }
        pending.insert(agent_id, message);
        drop(pending);
        Ok(())
    }

    /// Drain accepted runtime messages. Returns a typed error on lock
    /// poisoning instead of silently returning an empty vec, so callers can
    /// distinguish "nothing delivered yet" from a genuine failure.
    pub(super) fn drain(&self) -> Result<Vec<RuntimeMessage>, JspHostError> {
        let mut pending = self.pending.lock().map_err(|_| JspHostError::Poisoned)?;
        Ok(pending.drain().map(|(_, value)| value).collect())
    }
}

/// Cloneable lifecycle authority shared with the runtime manager.
#[derive(Debug, Clone)]
pub struct JspLaunchCoordinator {
    registry: PublisherRegistry,
    endpoint: SocketAddr,
    runtime_dir: PathBuf,
    active: Arc<Mutex<HashMap<AgentId, ActiveLaunch>>>,
    pending_cleanup: Arc<Mutex<Vec<BootstrapMaterial>>>,
    delivery: ObservationDelivery,
}

impl JspLaunchCoordinator {
    /// Reserve a fresh credential and bootstrap file before a supported spawn.
    pub fn prepare_launch(
        &self,
        agent_id: &AgentId,
        generation: u64,
        plan: &crate::domain::agent_definition::AgentLaunchPlan,
    ) -> Result<Option<PreparedJspLaunch>, JspHostError> {
        if !launch_supports_jsp(plan) {
            return Ok(None);
        }
        if generation == 0 {
            return Err(JspHostError::Forbidden);
        }
        self.retry_pending_cleanup()?;
        let reservation = super::PublisherReservation {
            agent_id: agent_id.clone(),
            generation,
            registration_id: random_opaque_id("reg-", 24)?,
            publisher_credential: random_opaque_id("pub-", 32)?,
        };
        self.registry.reserve(reservation.clone())?;
        let bootstrap = match create_bootstrap(&self.runtime_dir, self.endpoint, &reservation) {
            Ok(bootstrap) => bootstrap,
            Err(error) => {
                let _ = self.registry.revoke(agent_id, generation);
                return Err(error);
            }
        };
        let bootstrap_path = bootstrap.path().to_path_buf();
        // From here the credential is reserved and its bootstrap file exists,
        // so every failure path must undo both. Returning early would leave a
        // live credential on disk that nothing tracks and nothing revokes.
        let previous = {
            let Ok(mut active) = self.active.lock() else {
                let _ = self.registry.revoke(agent_id, generation);
                let _ = self.cleanup_or_retain(bootstrap);
                return Err(JspHostError::Poisoned);
            };
            active.insert(
                agent_id.clone(),
                ActiveLaunch {
                    generation,
                    bootstrap,
                },
            )
        };
        self.finish_prepared_launch(agent_id, generation, previous)?;
        Ok(Some(PreparedJspLaunch {
            coordinator: self.clone(),
            agent_id: agent_id.clone(),
            generation,
            bootstrap_path,
            committed: false,
        }))
    }

    /// Retire any superseded launch and announce the new one.
    ///
    /// The launch is already tracked, so `revoke` is the correct undo for any
    /// failure here: returning early without it would leave a live credential
    /// and its bootstrap file behind.
    fn finish_prepared_launch(
        &self,
        agent_id: &AgentId,
        generation: u64,
        previous: Option<ActiveLaunch>,
    ) -> Result<(), JspHostError> {
        if let Some(previous) = previous {
            if let Err(error) = self.registry.revoke(agent_id, previous.generation) {
                let _ = self.revoke(agent_id);
                return Err(error);
            }
            if let Err(error) = self.cleanup_or_retain(previous.bootstrap) {
                let _ = self.revoke(agent_id);
                return Err(error);
            }
        }
        if let Err(error) = self.delivery.publish(RuntimeMessage::ObservationCleared(
            agent_id.clone(),
            generation,
        )) {
            let _ = self.revoke(agent_id);
            return Err(error);
        }
        Ok(())
    }

    /// Revoke one agent's active credential and remove its bootstrap file.
    pub fn revoke(&self, agent_id: &AgentId) -> Result<(), JspHostError> {
        self.retry_pending_cleanup()?;
        let launch = self
            .active
            .lock()
            .map_err(|_| JspHostError::Poisoned)?
            .remove(agent_id);
        let Some(launch) = launch else {
            return Ok(());
        };
        self.registry.revoke(agent_id, launch.generation)?;
        self.delivery.publish(RuntimeMessage::ObservationCleared(
            agent_id.clone(),
            launch.generation,
        ))?;
        self.cleanup_or_retain(launch.bootstrap)
    }

    fn revoke_generation(&self, agent_id: &AgentId, generation: u64) -> Result<(), JspHostError> {
        let launch = {
            let mut active = self.active.lock().map_err(|_| JspHostError::Poisoned)?;
            if active
                .get(agent_id)
                .is_some_and(|launch| launch.generation == generation)
            {
                active.remove(agent_id)
            } else {
                None
            }
        };
        let Some(launch) = launch else {
            return Ok(());
        };
        self.registry.revoke(agent_id, generation)?;
        self.delivery.publish(RuntimeMessage::ObservationCleared(
            agent_id.clone(),
            generation,
        ))?;
        self.cleanup_or_retain(launch.bootstrap)
    }

    fn cleanup_or_retain(&self, bootstrap: BootstrapMaterial) -> Result<(), JspHostError> {
        if let Err(error) = bootstrap.cleanup() {
            self.pending_cleanup
                .lock()
                .map_err(|_| JspHostError::Poisoned)?
                .push(bootstrap);
            return Err(error);
        }
        Ok(())
    }

    fn retry_pending_cleanup(&self) -> Result<(), JspHostError> {
        let pending = self
            .pending_cleanup
            .lock()
            .map_err(|_| JspHostError::Poisoned)?
            .drain(..)
            .collect::<Vec<_>>();
        for bootstrap in pending {
            self.cleanup_or_retain(bootstrap)?;
        }
        Ok(())
    }

    fn revoke_all(&self) -> Result<(), JspHostError> {
        let launches = self
            .active
            .lock()
            .map_err(|_| JspHostError::Poisoned)?
            .drain()
            .collect::<Vec<_>>();
        // Revocation is teardown: every launch must be attempted even if an
        // earlier one fails, or a single failure would strand credentials for
        // every remaining agent. The first error is reported after the loop.
        let mut first_error = None;
        for (agent_id, launch) in launches {
            let generation = launch.generation;
            let mut record = |result: Result<(), JspHostError>| {
                if let Err(error) = result
                    && first_error.is_none()
                {
                    first_error = Some(error);
                }
            };
            record(self.registry.revoke(&agent_id, generation));
            record(
                self.delivery
                    .publish(RuntimeMessage::ObservationCleared(agent_id, generation)),
            );
            record(self.cleanup_or_retain(launch.bootstrap));
        }
        let retry = self.retry_pending_cleanup();
        match first_error {
            Some(error) => Err(error),
            None => retry,
        }
    }
}

/// One pending supported spawn. Dropping it before commit revokes the launch.
#[derive(Debug)]
pub struct PreparedJspLaunch {
    coordinator: JspLaunchCoordinator,
    agent_id: AgentId,
    generation: u64,
    bootstrap_path: PathBuf,
    committed: bool,
}

impl PreparedJspLaunch {
    #[must_use]
    pub fn bootstrap_path(&self) -> &Path {
        &self.bootstrap_path
    }

    pub fn commit(mut self) {
        self.committed = true;
    }
}

impl Drop for PreparedJspLaunch {
    fn drop(&mut self) {
        if !self.committed {
            let _ = self
                .coordinator
                .revoke_generation(&self.agent_id, self.generation);
        }
    }
}

/// Production-owned host worker and its synchronous delivery queue.
pub struct JspHostRuntime {
    endpoint: SocketAddr,
    coordinator: JspLaunchCoordinator,
    delivery: ObservationDelivery,
    shutdown: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl JspHostRuntime {
    /// Bind the host and start request processing before any instrumented child.
    pub fn start(runtime_dir: PathBuf) -> Result<Self, JspHostError> {
        cleanup_stale_bootstraps(&runtime_dir)?;
        let registry = PublisherRegistry::default();
        let host = JspHost::bind(registry.clone())?;
        let endpoint = host.local_addr()?;
        host.set_nonblocking()?;
        let delivery = ObservationDelivery::default();
        let worker_delivery = delivery.clone();
        let shutdown = Arc::new(AtomicBool::new(false));
        let worker_shutdown = Arc::clone(&shutdown);
        let worker = std::thread::Builder::new()
            .name("jefe-jsp-host".to_owned())
            .spawn(move || run_host_worker(host, worker_delivery, &worker_shutdown))?;
        Ok(Self {
            endpoint,
            coordinator: JspLaunchCoordinator {
                registry,
                endpoint,
                runtime_dir,
                active: Arc::new(Mutex::new(HashMap::new())),
                pending_cleanup: Arc::new(Mutex::new(Vec::new())),
                delivery: delivery.clone(),
            },
            delivery,
            shutdown,
            worker: Some(worker),
        })
    }

    #[must_use]
    pub const fn endpoint(&self) -> SocketAddr {
        self.endpoint
    }

    #[must_use]
    pub fn coordinator(&self) -> JspLaunchCoordinator {
        self.coordinator.clone()
    }

    /// Drain accepted runtime messages without blocking the render loop.
    ///
    /// Returns a typed error if the delivery lock is poisoned, so callers can
    /// distinguish a genuine failure from "nothing delivered yet."
    pub fn drain_messages(&self) -> Result<Vec<RuntimeMessage>, JspHostError> {
        self.delivery.drain()
    }
}

impl Drop for JspHostRuntime {
    fn drop(&mut self) {
        let _ = self.coordinator.revoke_all();
        self.shutdown.store(true, Ordering::Release);
        let _ = TcpStream::connect(self.endpoint);
        if let Some(worker) = self.worker.take() {
            match worker.join() {
                Ok(()) => {}
                // A Drop impl cannot return an error, but a worker panic must
                // still be diagnosable. Log a credential-free diagnostic so
                // the failure is visible without leaking bootstrap contents.
                Err(_) => {
                    tracing::error!("JSP host worker thread panicked during shutdown");
                }
            }
        }
    }
}

fn run_host_worker(host: JspHost, delivery: ObservationDelivery, shutdown: &AtomicBool) {
    while !shutdown.load(Ordering::Acquire) {
        match host.serve_once() {
            Ok(Some(message)) => {
                if let Err(error) = delivery.publish(message) {
                    tracing::warn!(error = %error, "JSP observation delivery failed");
                }
            }
            Ok(None) => {}
            Err(JspHostError::Io(error)) if error.kind() == std::io::ErrorKind::WouldBlock => {
                match host.registry.tick(Instant::now()) {
                    Ok(messages) => {
                        for message in messages {
                            if let Err(error) = delivery.publish(message) {
                                tracing::warn!(error = %error, "JSP lease delivery failed");
                            }
                        }
                    }
                    Err(error) => tracing::warn!(error = %error, "JSP lease tick failed"),
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => {
                tracing::warn!(error = %error, "JSP host request failed");
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

fn cleanup_stale_bootstraps(runtime_dir: &Path) -> Result<(), JspHostError> {
    std::fs::create_dir_all(runtime_dir)?;
    let runtime_metadata = std::fs::symlink_metadata(runtime_dir)?;
    if !runtime_metadata.file_type().is_dir() || runtime_metadata.file_type().is_symlink() {
        return Err(JspHostError::Forbidden);
    }
    set_owner_only_dir(runtime_dir)?;
    for entry in std::fs::read_dir(runtime_dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let candidate = (name.starts_with("jsp-") && name.ends_with(".json"))
            || (name.starts_with(".jsp-") && name.ends_with(".tmp"));
        if !candidate {
            continue;
        }
        let metadata = std::fs::symlink_metadata(entry.path())?;
        if metadata.file_type().is_file() && owned_by_runtime_owner(&runtime_metadata, &metadata) {
            std::fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn owned_by_runtime_owner(
    runtime_metadata: &std::fs::Metadata,
    candidate_metadata: &std::fs::Metadata,
) -> bool {
    use std::os::unix::fs::MetadataExt;
    runtime_metadata.uid() == candidate_metadata.uid()
}

#[cfg(not(unix))]
fn owned_by_runtime_owner(
    _runtime_metadata: &std::fs::Metadata,
    _candidate_metadata: &std::fs::Metadata,
) -> bool {
    true
}

const HEX: &[u8; 16] = b"0123456789abcdef";

fn random_opaque_id(prefix: &str, byte_count: usize) -> Result<String, JspHostError> {
    let mut bytes = vec![0_u8; byte_count];
    getrandom::fill(&mut bytes).map_err(JspHostError::Random)?;
    let mut value = String::with_capacity(prefix.len() + byte_count.saturating_mul(2));
    value.push_str(prefix);
    // Hex-encode directly. Formatting would introduce a Result that has no
    // meaningful failure for this input and was previously misreported as a
    // malformed request.
    for byte in bytes {
        value.push(char::from(HEX[usize::from(byte >> 4)]));
        value.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    Ok(value)
}

fn launch_supports_jsp(plan: &crate::domain::agent_definition::AgentLaunchPlan) -> bool {
    use crate::domain::agent_definition::Target;
    plan.type_id.as_str() == "core.llxprt" && matches!(plan.target, Target::Local { .. })
}

pub fn authorize_launch_environment(
    plan: &mut crate::domain::agent_definition::AgentLaunchPlan,
    bootstrap: &BootstrapMaterial,
) -> bool {
    authorize_launch_environment_path(plan, bootstrap.path())
}

pub fn authorize_launch_environment_path(
    plan: &mut crate::domain::agent_definition::AgentLaunchPlan,
    bootstrap_path: &Path,
) -> bool {
    if !launch_supports_jsp(plan) {
        return false;
    }
    // Replace any existing entry rather than appending a second one. A plan
    // reused across a relaunch would otherwise carry two bootstrap variables
    // and the child's resolution order would decide which one wins.
    let key = std::ffi::OsString::from(super::BOOTSTRAP_ENV);
    let value = bootstrap_path.as_os_str().to_os_string();
    if let Some(existing) = plan.env.iter_mut().find(|(name, _)| name == &key) {
        existing.1 = value;
    } else {
        plan.env.push((key, value));
    }
    true
}

#[cfg(unix)]
fn set_owner_only_dir(path: &Path) -> Result<(), JspHostError> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(windows)]
fn set_owner_only_dir(path: &Path) -> Result<(), JspHostError> {
    set_windows_owner_acl(path, true)
}

#[cfg(unix)]
fn set_owner_only_file(path: &Path) -> Result<(), JspHostError> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(windows)]
fn set_owner_only_file(path: &Path) -> Result<(), JspHostError> {
    set_windows_owner_acl(path, false)
}

#[cfg(unix)]
fn set_owner_only_file_options(options: &mut std::fs::OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt;
    options.mode(0o600);
}

#[cfg(windows)]
fn set_owner_only_file_options(_options: &mut std::fs::OpenOptions) {}

#[cfg(windows)]
fn set_windows_owner_acl(path: &Path, directory: bool) -> Result<(), JspHostError> {
    let account = whoami::account().map_err(|error| {
        JspHostError::Io(std::io::Error::other(format!(
            "current account lookup failed: {error}"
        )))
    })?;
    let inheritance = if directory { "(OI)(CI)F" } else { "F" };
    let grant = format!("{account}:{inheritance}");
    let status = std::process::Command::new("icacls.exe")
        .arg(path)
        .args(["/inheritance:r", "/grant:r"])
        .arg(grant)
        .status()?;
    if !status.success() {
        return Err(JspHostError::Io(std::io::Error::other(
            "owner-only Windows ACL application failed",
        )));
    }
    Ok(())
}
