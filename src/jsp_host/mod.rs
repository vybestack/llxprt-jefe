//! Authenticated IPv4-loopback JSP/1 embedded publisher boundary.
//!
//! This module owns transport/authentication/bootstrap I/O. It does not own or
//! mutate `AppState`; accepted documents are emitted as typed runtime messages.

use std::collections::HashMap;
use std::fmt;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

mod launch;
mod wire;

use self::wire::{read_request, write_response};

pub use launch::{
    BootstrapMaterial, JspHostRuntime, JspLaunchCoordinator, PreparedJspLaunch,
    authorize_launch_environment, create_bootstrap,
};
pub(crate) use launch::authorize_launch_environment_path;

use crate::domain::AgentId;
use crate::jsp::v1::reducer::{ReducerError, ReferenceReducer};
use crate::messages::RuntimeMessage;

const MAX_REQUEST_BYTES: usize = 1_100_000;
const MAX_PUBLISHERS: usize = 256;
/// Observer lease (specification §19.1).
///
/// Observation health becomes stale once this elapses with no accepted
/// document. Producers must heartbeat at or below a third of this so two
/// consecutive heartbeats can be lost before the lease expires.
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
    registry.mutate(token, |publisher| match &publisher.phase {
        PublisherPhase::Reserved => {
            validate_reservation(publisher, &snapshot.identity)?;
            publisher.phase = PublisherPhase::Registered {
                epoch: snapshot.identity.source_epoch.as_str().to_string(),
            };
            publisher.reducer.apply_snapshot(&snapshot);
            let now = Instant::now();
            publisher.last_seen = Some(now);
            Ok(Some(observation_message(publisher, now)))
        }
        PublisherPhase::Registered { epoch } => {
            // Idempotent replay: if the producer retries registration because
            // the original 200 response was lost, an identical identity triple
            // (agent_id + generation bound by validate_reservation, plus the
            // same source_epoch) is treated as a successful replay. The
            // canonical reducer must NOT be mutated a second time and no
            // duplicate observation message may be delivered — the replay
            // returns 200 with no message so it is observation-neutral.
            validate_reservation(publisher, &snapshot.identity)?;
            if *epoch == snapshot.identity.source_epoch.as_str() {
                publisher.last_seen = Some(Instant::now());
                Ok(None)
            } else {
                // A different epoch for the same agent/generation is a genuine
                // conflict: a new lifecycle tried to register over an active
                // stream.
                Err(JspHostError::Conflict)
            }
        }
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

