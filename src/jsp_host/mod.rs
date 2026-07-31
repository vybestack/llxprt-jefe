//! Authenticated IPv4-loopback JSP/1 embedded publisher boundary.
//!
//! This module owns transport/authentication/bootstrap I/O. It does not own or
//! mutate `AppState`; accepted documents are emitted as typed runtime messages.

use std::collections::HashMap;
use std::fmt;
use std::io::Write;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

mod wire;

use self::wire::{read_request, write_response};

use crate::domain::AgentId;
use crate::domain::observation::OpaqueId;
use crate::jsp::v1::reducer::{ReducerError, ReferenceReducer};
use crate::messages::RuntimeMessage;

const MAX_REQUEST_BYTES: usize = 1_100_000;
const MAX_PUBLISHERS: usize = 256;
const PRODUCER_LEASE: Duration = Duration::from_secs(15);

/// Environment variable authorized local LLxprt launches receive.
pub const BOOTSTRAP_ENV: &str = "LLXPRT_JSP_BOOTSTRAP_FILE";

/// Typed host/bootstrap failures. Diagnostics never contain credentials.
#[derive(Debug)]
pub enum JspHostError {
    Io(std::io::Error),
    InvalidRequest,
    Unauthorized,
    Forbidden,
    Conflict,
    Protocol(String),
    RejectedProtocol(String, RuntimeMessage),
    Capacity,
    Poisoned,
    Random(getrandom::Error),
    WorkerPanicked,
}

impl fmt::Display for JspHostError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "JSP host I/O failed: {error}"),
            Self::InvalidRequest => formatter.write_str("invalid JSP request"),
            Self::Unauthorized => formatter.write_str("unknown JSP publisher credential"),
            Self::Forbidden => formatter.write_str("JSP publisher binding mismatch"),
            Self::Conflict => formatter.write_str("JSP publisher registration conflict"),
            Self::Protocol(detail) | Self::RejectedProtocol(detail, _) => {
                write!(formatter, "JSP protocol error: {detail}")
            }
            Self::Capacity => formatter.write_str("JSP publisher capacity reached"),
            Self::Poisoned => formatter.write_str("JSP host state lock failed"),
            Self::Random(error) => write!(formatter, "JSP credential generation failed: {error}"),
            Self::WorkerPanicked => formatter.write_str("JSP host worker stopped unexpectedly"),
        }
    }
}

impl std::error::Error for JspHostError {}

impl From<std::io::Error> for JspHostError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<ReducerError> for JspHostError {
    fn from(error: ReducerError) -> Self {
        Self::Protocol(error.to_string())
    }
}

/// One pre-authorized publisher binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublisherReservation {
    pub agent_id: AgentId,
    pub generation: u64,
    pub registration_id: String,
    pub publisher_credential: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PublisherPhase {
    Reserved,
    Registered { epoch: String },
}

#[derive(Debug)]
struct Publisher {
    reservation: PublisherReservation,
    phase: PublisherPhase,
    reducer: ReferenceReducer,
    last_seen: Option<Instant>,
    turn_anchor: Option<(u64, Instant)>,
}

/// Shared authenticated publication registry.
#[derive(Debug, Clone, Default)]
pub struct PublisherRegistry {
    publishers: Arc<Mutex<HashMap<String, Publisher>>>,
}

impl PublisherRegistry {
    /// Reserve an opaque credential for one positive lifecycle generation.
    pub fn reserve(&self, reservation: PublisherReservation) -> Result<(), JspHostError> {
        if reservation.generation == 0
            || reservation.registration_id.is_empty()
            || reservation.publisher_credential.len() < 32
        {
            return Err(JspHostError::Forbidden);
        }
        let mut publishers = self.publishers.lock().map_err(|_| JspHostError::Poisoned)?;
        if publishers.contains_key(&reservation.publisher_credential) {
            return Err(JspHostError::Conflict);
        }
        if publishers.len() >= MAX_PUBLISHERS {
            return Err(JspHostError::Capacity);
        }
        publishers.insert(
            reservation.publisher_credential.clone(),
            Publisher {
                reservation,
                phase: PublisherPhase::Reserved,
                reducer: ReferenceReducer::new(),
                last_seen: None,
                turn_anchor: None,
            },
        );
        drop(publishers);
        Ok(())
    }

    /// Revoke all credentials for exactly one agent/generation.
    pub fn revoke(&self, agent_id: &AgentId, generation: u64) -> Result<(), JspHostError> {
        self.publishers
            .lock()
            .map_err(|_| JspHostError::Poisoned)?
            .retain(|_, publisher| {
                publisher.reservation.agent_id != *agent_id
                    || publisher.reservation.generation != generation
            });
        Ok(())
    }

    /// Transition expired registered producers to stale health. Each producer
    /// emits at most one mutation until a heartbeat or snapshot renews its lease.
    pub fn tick(&self, now: Instant) -> Result<Vec<RuntimeMessage>, JspHostError> {
        let mut publishers = self.publishers.lock().map_err(|_| JspHostError::Poisoned)?;
        let mut messages = Vec::new();
        for publisher in publishers.values_mut() {
            let expired = matches!(publisher.phase, PublisherPhase::Registered { .. })
                && publisher
                    .last_seen
                    .is_some_and(|seen| now.saturating_duration_since(seen) >= PRODUCER_LEASE)
                && publisher.reducer.observation().health
                    != crate::domain::observation::ObservationHealth::Stale;
            if expired {
                publisher.reducer.mark_observation_stale();
                messages.push(observation_message(publisher, now));
            }
        }
        drop(publishers);
        Ok(messages)
    }

    fn mutate<R>(
        &self,
        token: &str,
        operation: impl FnOnce(&mut Publisher) -> Result<R, JspHostError>,
    ) -> Result<R, JspHostError> {
        let mut publishers = self.publishers.lock().map_err(|_| JspHostError::Poisoned)?;
        let publisher = publishers
            .get_mut(token)
            .ok_or(JspHostError::Unauthorized)?;
        let result = operation(publisher);
        drop(publishers);
        result
    }
}

/// Bound loopback host. `serve_once` is synchronous and deterministic; callers
/// own scheduling and message delivery.
pub struct JspHost {
    listener: TcpListener,
    registry: PublisherRegistry,
}

impl JspHost {
    /// Bind an OS-assigned IPv4-loopback port.
    pub fn bind(registry: PublisherRegistry) -> Result<Self, JspHostError> {
        let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))?;
        Ok(Self { listener, registry })
    }

    /// The loopback endpoint supplied through bootstrap material.
    pub fn local_addr(&self) -> Result<SocketAddr, JspHostError> {
        let address = self.listener.local_addr()?;
        if !matches!(address, SocketAddr::V4(v4) if v4.ip().is_loopback()) {
            return Err(JspHostError::Forbidden);
        }
        Ok(address)
    }

    fn set_nonblocking(&self) -> Result<(), JspHostError> {
        self.listener.set_nonblocking(true)?;
        Ok(())
    }

    /// Accept and process exactly one bounded HTTP/1.1 request.
    pub fn serve_once(&self) -> Result<Option<RuntimeMessage>, JspHostError> {
        let (mut stream, peer) = self.listener.accept()?;
        if !peer.ip().is_loopback() {
            return Err(JspHostError::Forbidden);
        }
        stream.set_read_timeout(Some(Duration::from_secs(2)))?;
        stream.set_write_timeout(Some(Duration::from_secs(2)))?;
        let result = handle_stream(&mut stream, &self.registry);
        let (status, message) = match result {
            Ok(message) => (200, message),
            Err(JspHostError::Unauthorized) => (401, None),
            Err(JspHostError::Forbidden) => (403, None),
            Err(JspHostError::Conflict) => (409, None),
            Err(JspHostError::RejectedProtocol(_, message)) => (400, Some(message)),
            Err(JspHostError::InvalidRequest | JspHostError::Protocol(_)) => (400, None),
            Err(error) => return Err(error),
        };
        write_response(&mut stream, status)?;
        Ok(message)
    }
}

fn handle_stream(
    stream: &mut TcpStream,
    registry: &PublisherRegistry,
) -> Result<Option<RuntimeMessage>, JspHostError> {
    let request = read_request(stream)?;
    registry.mutate(&request.token, |publisher| {
        if publisher.reservation.registration_id != request.registration_id {
            return Err(JspHostError::Forbidden);
        }
        Ok(())
    })?;
    match request.route.as_str() {
        "/jsp/1/register" => register(registry, &request.token, &request.body),
        "/jsp/1/publish" => publish(registry, &request.token, &request.body),
        "/jsp/1/heartbeat" => heartbeat(registry, &request.token, &request.body),
        _ => Err(JspHostError::InvalidRequest),
    }
}

fn register(
    registry: &PublisherRegistry,
    token: &str,
    body: &[u8],
) -> Result<Option<RuntimeMessage>, JspHostError> {
    let snapshot = crate::jsp::parse_snapshot(body)
        .map_err(|error| JspHostError::Protocol(error.to_string()))?;
    registry.mutate(token, |publisher| {
        if publisher.phase != PublisherPhase::Reserved {
            return Err(JspHostError::Conflict);
        }
        validate_reservation(publisher, &snapshot.identity)?;
        publisher.phase = PublisherPhase::Registered {
            epoch: snapshot.identity.source_epoch.as_str().to_string(),
        };
        publisher.reducer.apply_snapshot(&snapshot);
        let now = Instant::now();
        publisher.last_seen = Some(now);
        Ok(Some(observation_message(publisher, now)))
    })
}

fn publish(
    registry: &PublisherRegistry,
    token: &str,
    body: &[u8],
) -> Result<Option<RuntimeMessage>, JspHostError> {
    registry.mutate(token, |publisher| {
        ensure_registered(publisher)?;
        let now = Instant::now();
        if let Ok(snapshot) = crate::jsp::parse_snapshot(body) {
            validate_binding(publisher, &snapshot.identity)?;
            publisher.reducer.apply_snapshot(&snapshot);
            publisher.last_seen = Some(now);
            return Ok(Some(observation_message(publisher, now)));
        }
        let event = match crate::jsp::v1::parse_event(body) {
            Ok(event) => event,
            Err(error) => {
                publisher.reducer.mark_protocol_error();
                let message = observation_message(publisher, now);
                return Err(JspHostError::RejectedProtocol(error.to_string(), message));
            }
        };
        validate_binding(publisher, &event.identity)?;
        if let Err(error) = publisher.reducer.apply_event(&event) {
            if !matches!(
                error,
                ReducerError::Gap { .. } | ReducerError::FreshSnapshotRequired
            ) {
                publisher.reducer.mark_protocol_error();
            }
            let message = observation_message(publisher, now);
            return Err(JspHostError::RejectedProtocol(error.to_string(), message));
        }
        publisher.last_seen = Some(now);
        Ok(Some(observation_message(publisher, now)))
    })
}

fn heartbeat(
    registry: &PublisherRegistry,
    token: &str,
    body: &[u8],
) -> Result<Option<RuntimeMessage>, JspHostError> {
    registry.mutate(token, |publisher| {
        ensure_registered(publisher)?;
        let now = Instant::now();
        let heartbeat = match crate::jsp::v1::parse_heartbeat(body) {
            Ok(heartbeat) => heartbeat,
            Err(error) => {
                publisher.reducer.mark_protocol_error();
                let message = observation_message(publisher, now);
                return Err(JspHostError::RejectedProtocol(error.to_string(), message));
            }
        };
        validate_binding(publisher, &heartbeat.identity)?;
        if let Err(error) = publisher.reducer.apply_heartbeat(&heartbeat) {
            let message = observation_message(publisher, now);
            return Err(JspHostError::RejectedProtocol(error.to_string(), message));
        }
        publisher.last_seen = Some(now);
        Ok(Some(observation_message(publisher, now)))
    })
}

fn ensure_registered(publisher: &Publisher) -> Result<(), JspHostError> {
    if matches!(publisher.phase, PublisherPhase::Registered { .. }) {
        Ok(())
    } else {
        Err(JspHostError::Conflict)
    }
}

fn validate_reservation(
    publisher: &Publisher,
    identity: &crate::domain::observation::ObservationIdentity,
) -> Result<(), JspHostError> {
    if identity.agent_id.as_str() != publisher.reservation.agent_id.0
        || identity.lifecycle_generation != publisher.reservation.generation
    {
        return Err(JspHostError::Forbidden);
    }
    Ok(())
}

fn validate_binding(
    publisher: &Publisher,
    identity: &crate::domain::observation::ObservationIdentity,
) -> Result<(), JspHostError> {
    validate_reservation(publisher, identity)?;
    match &publisher.phase {
        PublisherPhase::Registered { epoch } if epoch == identity.source_epoch.as_str() => Ok(()),
        PublisherPhase::Registered { .. } | PublisherPhase::Reserved => Err(JspHostError::Conflict),
    }
}

fn observation_message(publisher: &mut Publisher, now: Instant) -> RuntimeMessage {
    let mut observation = publisher.reducer.observation();
    let elapsed = match &observation.turn {
        crate::domain::observation::FieldState::Supported {
            availability: crate::domain::observation::Availability::Known(Some(turn)),
            ..
        } => Some(turn.elapsed_ms),
        _ => None,
    };
    publisher.turn_anchor = match (publisher.turn_anchor, elapsed) {
        (Some((anchored, instant)), Some(current)) if anchored == current => {
            Some((anchored, instant))
        }
        (_, Some(current)) => Some((current, now)),
        (_, None) => None,
    };
    observation.turn_observed_at = publisher.turn_anchor.map(|(_, instant)| instant);
    RuntimeMessage::ObservationUpdated(
        publisher.reservation.agent_id.clone(),
        publisher.reservation.generation,
        Box::new(observation),
    )
}

/// Atomically create owner-only bootstrap material for an authorized local
/// launch. Callers supply a cryptographically random opaque credential.
pub fn create_bootstrap(
    runtime_dir: &Path,
    endpoint: SocketAddr,
    reservation: &PublisherReservation,
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
struct ObservationDelivery {
    pending: Arc<Mutex<HashMap<AgentId, RuntimeMessage>>>,
}

impl ObservationDelivery {
    fn publish(&self, message: RuntimeMessage) -> Result<(), JspHostError> {
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

    fn drain(&self) -> Vec<RuntimeMessage> {
        self.pending.lock().map_or_else(
            |_| Vec::new(),
            |mut pending| pending.drain().map(|(_, value)| value).collect(),
        )
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
        let reservation = PublisherReservation {
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
        let previous = {
            let mut active = self.active.lock().map_err(|_| JspHostError::Poisoned)?;
            active.insert(
                agent_id.clone(),
                ActiveLaunch {
                    generation,
                    bootstrap,
                },
            )
        };
        if let Some(previous) = previous {
            self.registry.revoke(agent_id, previous.generation)?;
            self.cleanup_or_retain(previous.bootstrap)?;
        }
        self.delivery.publish(RuntimeMessage::ObservationCleared(
            agent_id.clone(),
            generation,
        ))?;
        Ok(Some(PreparedJspLaunch {
            coordinator: self.clone(),
            agent_id: agent_id.clone(),
            generation,
            bootstrap_path,
            committed: false,
        }))
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
        for (agent_id, launch) in launches {
            self.registry.revoke(&agent_id, launch.generation)?;
            self.delivery.publish(RuntimeMessage::ObservationCleared(
                agent_id,
                launch.generation,
            ))?;
            self.cleanup_or_retain(launch.bootstrap)?;
        }
        self.retry_pending_cleanup()
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
    #[must_use]
    pub fn drain_messages(&self) -> Vec<RuntimeMessage> {
        self.delivery.drain()
    }
}

impl Drop for JspHostRuntime {
    fn drop(&mut self) {
        let _ = self.coordinator.revoke_all();
        self.shutdown.store(true, Ordering::Release);
        let _ = TcpStream::connect(self.endpoint);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
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

fn random_opaque_id(prefix: &str, byte_count: usize) -> Result<String, JspHostError> {
    let mut bytes = vec![0_u8; byte_count];
    getrandom::fill(&mut bytes).map_err(JspHostError::Random)?;
    let mut value = String::with_capacity(prefix.len() + byte_count.saturating_mul(2));
    value.push_str(prefix);
    for byte in bytes {
        use std::fmt::Write as _;
        write!(value, "{byte:02x}").map_err(|_| JspHostError::InvalidRequest)?;
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

pub(crate) fn authorize_launch_environment_path(
    plan: &mut crate::domain::agent_definition::AgentLaunchPlan,
    bootstrap_path: &Path,
) -> bool {
    if !launch_supports_jsp(plan) {
        return false;
    }
    plan.env.push((
        std::ffi::OsString::from(BOOTSTRAP_ENV),
        bootstrap_path.as_os_str().to_os_string(),
    ));
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

/// Parse a producer epoch only after the authoritative parser has validated it.
#[must_use]
pub fn epoch_label(epoch: &OpaqueId) -> &str {
    epoch.as_str()
}
