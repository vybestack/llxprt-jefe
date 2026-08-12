//! Provider runtime coordinator (issue #390 CW-10, Slice D).
//!
//! The coordinator is the edge-owned runtime owner of the provider subsystem.
//! It owns the [`PersistentSupervisor`] (when persistent providers started
//! successfully) and the data-only [`PersistentPublication`] snapshot. It never
//! lives inside `AppState` — it is held by `AppContext` so process handles stay
//! at the boundary, never in pure state.
//!
//! The coordinator also owns the immutable provider action catalog: a mapping
//! from [`ActionId`] to the descriptor the background worker needs to execute a
//! one-shot invocation. The catalog is built once at startup composition and
//! never mutated.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use crate::domain::action_registry::ActionId;
use crate::domain::plugin::provider::ProviderMode;
use crate::domain::{CanonicalSemver, Id};
use crate::runtime::provider::dto::{ConfigurePayload, InvokeActionPayload, InvokeContext};
use crate::runtime::provider::environment::ProviderEnvironment;
use crate::runtime::provider::identifiers::RequestId;
use crate::runtime::provider::persistent::PersistentPublication;
use crate::runtime::provider::persistent_session::{
    PersistentInvocation, PersistentInvokeError, PersistentSessionOwner,
};
use crate::state::provider_requests::ActionPolicy;

/// One action's runtime descriptor, keyed by `ActionId`.
///
/// Built once at startup composition from the selected provider binary and
/// the action declaration. The background worker clones this to build a
/// `OneShotRequest` for each invocation.
#[derive(Debug, Clone)]
pub struct ProviderActionDescriptor {
    /// The already-parsed `ActionId` (infallible at invocation time).
    pub action_id: ActionId,
    /// The plugin package id.
    pub plugin_id: Id,
    /// The plugin package version.
    pub plugin_version: CanonicalSemver,
    /// Whether this provider is one-shot or persistent.
    pub mode: ProviderMode,
    /// The resolved host binary path.
    pub binary: PathBuf,
    /// Arguments passed to the binary at spawn.
    pub provider_args: Vec<String>,
    /// Contained working directory.
    pub working_dir: PathBuf,
    /// Contained HOME.
    pub home: PathBuf,
    /// Contained TMPDIR.
    pub tmpdir: PathBuf,
    /// Locale (`LC_ALL`/`LANG`).
    pub locale: String,
    /// Host API identifier sent in `hello`.
    pub host_api: String,
    /// Environment specification (CW10-14).
    pub environment: ProviderEnvironment,
    /// Base `configure` payload; the supervisor merges resolved secrets in.
    pub configure: ConfigurePayload,
    /// Immutable action policy derived from the action declaration.
    pub policy: ActionPolicy,
    /// The declared invocation timeout, in seconds (manifest range 1..=600).
    pub timeout_seconds: u32,
}

/// The immutable provider action catalog.
///
/// Maps each published provider [`ActionId`] to its runtime descriptor. Built
/// once at startup; never mutated during the session.
#[derive(Debug, Clone, Default)]
pub struct ProviderCatalog {
    entries: BTreeMap<ActionId, ProviderActionDescriptor>,
}

impl ProviderCatalog {
    /// Construct an empty catalog.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register one action descriptor.
    pub fn insert(&mut self, action_id: ActionId, descriptor: ProviderActionDescriptor) {
        self.entries.insert(action_id, descriptor);
    }

    /// Look up the descriptor for one action.
    #[must_use]
    pub fn get(&self, action_id: &ActionId) -> Option<&ProviderActionDescriptor> {
        self.entries.get(action_id)
    }

    /// Iterate over action descriptors in stable action-ID order.
    pub fn iter(&self) -> impl Iterator<Item = (&ActionId, &ProviderActionDescriptor)> {
        self.entries.iter()
    }

    /// Withdraw one action from the runnable catalog.
    ///
    /// Used when publication does not happen: an action whose provider never
    /// became ready must not be invocable, even though its metadata stays
    /// visible with an unavailable reason.
    pub fn remove(&mut self, action_id: &ActionId) {
        self.entries.remove(action_id);
    }

    /// Whether the catalog is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The number of registered actions.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

/// Build the `invoke-action` payload for one invocation from the descriptor
/// and the host-side invocation data.
///
/// The `ActionId` is taken from the descriptor (pre-parsed at startup), so no
/// runtime parsing or fallback is needed.
pub fn build_invocation_payload(
    descriptor: &ProviderActionDescriptor,
    invocation: &crate::domain::effects::ProviderInvocation,
) -> Result<InvokeActionPayload, InvocationIdError> {
    let invocation_id = build_invocation_id(&descriptor.plugin_id, invocation.key.generation)?;
    Ok(InvokeActionPayload {
        invocation_id,
        action_id: descriptor.action_id.clone(),
        arguments: invocation.arguments.clone(),
        context: InvokeContext {
            screen_id: invocation.context_screen.clone(),
            screen_instance: invocation.context_instance.clone(),
            resource_refs: invocation.context_refs.clone(),
        },
        continuation: invocation.continuation.clone().map(|continuation| {
            crate::runtime::provider::dto::Continuation {
                confirmation_id: continuation.confirmation_id,
                approved: continuation.approved,
                values: continuation.values,
            }
        }),
    })
}

/// Build a typed invocation id from the plugin id and generation.
///
/// The dot separator satisfies the `Id` grammar, but `Id` also has a byte
/// limit, and a long plugin id plus a generation can exceed it. Falling back to
/// the bare plugin id would hand every invocation of that package the *same*
/// invocation id, silently destroying the correlation the id exists to carry —
/// so an id that cannot be built is an error the caller must see.
///
/// # Errors
///
/// Returns [`InvocationIdError`] when the composed id exceeds the `Id` bound.
fn build_invocation_id(plugin_id: &Id, generation: u64) -> Result<Id, InvocationIdError> {
    let raw = format!("{plugin_id}.{generation}");
    Id::parse(&raw).map_err(|_| InvocationIdError { raw })
}

/// A `{plugin_id}.{generation}` pair that does not fit the `Id` grammar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvocationIdError {
    /// The composed value that was refused.
    pub raw: String,
}

impl std::fmt::Display for InvocationIdError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invocation id {:?} is not a valid Id", self.raw)
    }
}

impl std::error::Error for InvocationIdError {}

/// Why one invocation could not be turned into a supervisor request.
///
/// Both variants are host-side defects, not package defects: the caller must
/// report the request as unavailable rather than spawn a provider that would
/// be sent an id it cannot be correlated by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestBuildError {
    /// The monotonic request-id counter exceeded the 20-digit bound.
    RequestId(crate::runtime::provider::identifiers::RequestIdError),
    /// The `{plugin_id}.{generation}` invocation id exceeded the `Id` bound.
    InvocationId(InvocationIdError),
}

impl std::fmt::Display for RequestBuildError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RequestId(error) => write!(formatter, "invalid request id: {error:?}"),
            Self::InvocationId(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for RequestBuildError {}

impl From<crate::runtime::provider::identifiers::RequestIdError> for RequestBuildError {
    fn from(error: crate::runtime::provider::identifiers::RequestIdError) -> Self {
        Self::RequestId(error)
    }
}

impl From<InvocationIdError> for RequestBuildError {
    fn from(error: InvocationIdError) -> Self {
        Self::InvocationId(error)
    }
}

/// Build a `OneShotRequest` from a descriptor and invocation.
///
/// The caller supplies the monotonic request-id counter value; this guarantees
/// uniqueness per in-flight request without deriving from the generation
/// (which can collide under modulo).
///
/// # Errors
///
/// Returns [`RequestBuildError`] when the request id or the invocation id
/// cannot be built. Neither is expected in practice; both are reported rather
/// than papered over because a duplicate id destroys correlation silently.
pub fn build_one_shot_request(
    descriptor: &ProviderActionDescriptor,
    invocation: &crate::domain::effects::ProviderInvocation,
    request_counter: u64,
) -> Result<crate::runtime::provider::supervisor::OneShotRequest, RequestBuildError> {
    let request_id = RequestId::new_host(request_counter)?;
    Ok(crate::runtime::provider::supervisor::OneShotRequest {
        binary: descriptor.binary.clone(),
        arguments: descriptor.provider_args.clone(),
        working_dir: descriptor.working_dir.clone(),
        environment: descriptor.environment.clone(),
        home: descriptor.home.clone(),
        tmpdir: descriptor.tmpdir.clone(),
        locale: descriptor.locale.clone(),
        host_api: descriptor.host_api.clone(),
        plugin_id: descriptor.plugin_id.clone(),
        plugin_version: descriptor.plugin_version.clone(),
        generation: invocation.key.generation,
        request_id,
        configure: descriptor.configure.clone(),
        invocation: build_invocation_payload(descriptor, invocation)?,
    })
}

/// The provider runtime coordinator.
///
/// Owns the persistent session owner (when persistent providers started
/// successfully) plus the data-only publication snapshot. Held by `AppContext`,
/// never by `AppState`. Shuts down before host exit. Owns a monotonic
/// request-id counter so each in-flight request gets a unique `h-` id.
pub struct ProviderCoordinator {
    sessions: PersistentSessionOwner,
    publication: Option<PersistentPublication>,
    catalog: ProviderCatalog,
    request_counter: AtomicU64,
}

/// A persistent dispatch failure: the request id or invocation payload could
/// not be built, or no live session owns the plugin id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PersistentDispatchError {
    /// The request id or invocation id exceeded a grammar bound.
    Build(RequestBuildError),
    /// No session owns the plugin id or the owner thread has exited.
    Session(PersistentInvokeError),
}

impl std::fmt::Display for PersistentDispatchError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Build(error) => write!(formatter, "{error}"),
            Self::Session(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for PersistentDispatchError {}

impl From<RequestBuildError> for PersistentDispatchError {
    fn from(error: RequestBuildError) -> Self {
        Self::Build(error)
    }
}

impl From<PersistentInvokeError> for PersistentDispatchError {
    fn from(error: PersistentInvokeError) -> Self {
        Self::Session(error)
    }
}

impl ProviderCoordinator {
    /// Construct an empty coordinator (no persistent providers, empty catalog).
    #[must_use]
    pub fn empty() -> Self {
        Self::from_catalog(ProviderCatalog::new())
    }

    /// Construct a coordinator that owns a catalog but no persistent process.
    ///
    /// This is the one-shot case: actions are invocable, and every process
    /// exists only for the duration of one invocation.
    #[must_use]
    pub fn from_catalog(catalog: ProviderCatalog) -> Self {
        Self {
            sessions: PersistentSessionOwner::empty(),
            publication: None,
            catalog,
            request_counter: AtomicU64::new(0),
        }
    }

    /// Construct from a persistent startup result and catalog. On success,
    /// moves each ready candidate into a command-owner thread and stores the
    /// publication snapshot. On failure, no sessions are held and no processes
    /// leak.
    #[must_use]
    pub fn from_startup(
        result: crate::runtime::provider::persistent::PersistentStartupResult,
        catalog: ProviderCatalog,
    ) -> Self {
        use crate::runtime::provider::persistent::PersistentStartupResult;
        match result {
            PersistentStartupResult::Started {
                supervisor,
                publication,
            } => Self {
                sessions: supervisor.into_sessions(),
                publication: Some(publication),
                catalog,
                request_counter: AtomicU64::new(0),
            },
            PersistentStartupResult::Failed(_) => Self {
                sessions: PersistentSessionOwner::empty(),
                publication: None,
                catalog,
                request_counter: AtomicU64::new(0),
            },
        }
    }

    /// The data-only publication snapshot, when persistent providers are ready.
    #[must_use]
    pub fn publication(&self) -> Option<&PersistentPublication> {
        self.publication.as_ref()
    }

    /// The immutable provider action catalog.
    #[must_use]
    pub fn catalog(&self) -> &ProviderCatalog {
        &self.catalog
    }

    /// Whether any persistent provider session is owned.
    #[must_use]
    pub fn has_persistent(&self) -> bool {
        !self.sessions.is_empty()
    }

    /// Probe every persistent candidate's health.
    #[must_use]
    pub fn health(&self) -> Vec<crate::runtime::provider::persistent::CandidateHealthSnapshot> {
        self.sessions.health()
    }

    /// Allocate the next monotonic request-id counter value.
    fn next_request_counter(&self) -> u64 {
        self.request_counter.fetch_add(1, Ordering::SeqCst)
    }

    /// Build a `OneShotRequest` for one invocation using the coordinator's
    /// monotonic request-id counter.
    ///
    /// # Errors
    ///
    /// Returns [`RequestBuildError`] if the request id or invocation id cannot
    /// be built (practically never).
    pub fn build_one_shot(
        &self,
        descriptor: &ProviderActionDescriptor,
        invocation: &crate::domain::effects::ProviderInvocation,
    ) -> Result<crate::runtime::provider::supervisor::OneShotRequest, RequestBuildError> {
        let counter = self.next_request_counter();
        build_one_shot_request(descriptor, invocation, counter)
    }

    /// Invoke one action on the already-Ready persistent candidate for the
    /// descriptor's plugin id. The invocation runs in the candidate's
    /// command-owner thread; the returned handle lets the caller poll live
    /// progress, set a cancel flag, and consume the terminal result. The
    /// candidate stays alive for the next invocation.
    ///
    /// # Errors
    ///
    /// Returns [`PersistentDispatchError`] when the request id or invocation
    /// payload cannot be built, or when no session owns the plugin id or the
    /// owner thread has exited.
    pub fn invoke_persistent(
        &self,
        descriptor: &ProviderActionDescriptor,
        invocation: &crate::domain::effects::ProviderInvocation,
    ) -> Result<PersistentInvocation, PersistentDispatchError> {
        let counter = self.next_request_counter();
        let request_id = RequestId::new_host(counter).map_err(RequestBuildError::from)?;
        let payload =
            build_invocation_payload(descriptor, invocation).map_err(RequestBuildError::from)?;
        let timeout = Duration::from_secs(u64::from(descriptor.timeout_seconds.max(1)));
        Ok(self
            .sessions
            .invoke(&descriptor.plugin_id, request_id, payload, timeout)?)
    }

    /// Queue a panel activation on the exact owning persistent provider.
    pub fn activate_panel(
        &self,
        owner: &Id,
        payload: crate::runtime::provider::panel_model::ActivatePanelPayload,
    ) -> Result<(), PersistentDispatchError> {
        let request_id =
            RequestId::new_host(self.next_request_counter()).map_err(RequestBuildError::from)?;
        self.sessions.activate_panel(owner, request_id, payload)?;
        Ok(())
    }

    /// Queue a panel deactivation on the exact owning persistent provider.
    pub fn deactivate_panel(
        &self,
        owner: &Id,
        payload: crate::runtime::provider::panel_model::DeactivatePanelPayload,
    ) -> Result<(), PersistentDispatchError> {
        let request_id =
            RequestId::new_host(self.next_request_counter()).map_err(RequestBuildError::from)?;
        self.sessions.deactivate_panel(owner, request_id, payload)?;
        Ok(())
    }

    /// Queue one validated semantic panel event on the exact persistent owner.
    pub fn panel_event(
        &self,
        owner: &Id,
        payload: crate::runtime::provider::panel_model::PanelEventPayload,
    ) -> Result<(), PersistentDispatchError> {
        let request_id =
            RequestId::new_host(self.next_request_counter()).map_err(RequestBuildError::from)?;
        self.sessions.panel_event(owner, request_id, payload)?;
        Ok(())
    }

    /// Drain all asynchronous panel snapshots currently delivered by providers.
    #[must_use]
    pub fn drain_panel_deliveries(
        &self,
    ) -> Vec<crate::runtime::provider::persistent_session::PanelDelivery> {
        self.sessions.drain_panel_deliveries()
    }

    /// Shut down every persistent candidate and reap the process trees.
    /// Idempotent. Must be called before host exit.
    pub fn shutdown(&mut self) {
        self.sessions.shutdown();
    }
}

impl std::fmt::Debug for ProviderCoordinator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderCoordinator")
            .field("has_persistent", &self.has_persistent())
            .field(
                "candidate_count",
                &self
                    .publication
                    .as_ref()
                    .map_or(0, |publication| publication.ready().len()),
            )
            .field("catalog_len", &self.catalog.len())
            .finish_non_exhaustive()
    }
}
