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

use crate::domain::action_registry::ActionId;
use crate::domain::plugin::provider::ProviderMode;
use crate::domain::{CanonicalSemver, Id};
use crate::runtime::provider::dto::{ConfigurePayload, InvokeActionPayload, InvokeContext};
use crate::runtime::provider::environment::ProviderEnvironment;
use crate::runtime::provider::identifiers::RequestId;
use crate::runtime::provider::persistent::{
    PersistentPublication, PersistentStartupResult, PersistentSupervisor,
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
#[must_use]
pub fn build_invocation_payload(
    descriptor: &ProviderActionDescriptor,
    invocation: &crate::domain::effects::ProviderInvocation,
) -> InvokeActionPayload {
    let invocation_id = build_invocation_id(&descriptor.plugin_id, invocation.key.generation);
    InvokeActionPayload {
        invocation_id,
        action_id: descriptor.action_id.clone(),
        arguments: invocation.arguments.clone(),
        context: InvokeContext {
            screen_id: invocation.context_screen.clone(),
            screen_instance: invocation.context_instance.clone(),
            resource_refs: invocation.context_refs.clone(),
        },
        continuation: invocation.continuation.clone().map(|c| {
            crate::runtime::provider::dto::Continuation {
                confirmation_id: c.confirmation_id,
                approved: c.approved,
                values: c.values,
            }
        }),
    }
}

/// Build a typed invocation id from the plugin id and generation.
///
/// The format `{plugin_id}.{generation}` is valid because plugin ids are valid
/// `Id` values and the generation is a `u64` rendered as decimal digits. The
/// dot separator between them satisfies the `Id` grammar.
fn build_invocation_id(plugin_id: &Id, generation: u64) -> Id {
    let raw = format!("{plugin_id}.{generation}");
    Id::parse(&raw).unwrap_or_else(|_| plugin_id.clone())
}

/// Build a `OneShotRequest` from a descriptor and invocation.
///
/// The caller supplies the monotonic request-id counter value; this guarantees
/// uniqueness per in-flight request without deriving from the generation
/// (which can collide under modulo).
///
/// # Errors
///
/// Returns [`RequestIdError`](crate::runtime::provider::identifiers::RequestIdError)
/// when the counter exceeds 20 digits. In practice this never happens because
/// the counter is a `u64` that starts at zero and increments by one.
pub fn build_one_shot_request(
    descriptor: &ProviderActionDescriptor,
    invocation: &crate::domain::effects::ProviderInvocation,
    request_counter: u64,
) -> Result<
    crate::runtime::provider::supervisor::OneShotRequest,
    crate::runtime::provider::identifiers::RequestIdError,
> {
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
        invocation: build_invocation_payload(descriptor, invocation),
    })
}

/// The provider runtime coordinator.
///
/// Owns the [`PersistentSupervisor`] when persistent providers started
/// successfully, plus the data-only publication snapshot. Held by `AppContext`,
/// never by `AppState`. Shuts down before host exit. Owns a monotonic
/// request-id counter so each in-flight request gets a unique `h-` id.
pub struct ProviderCoordinator {
    persistent: Option<PersistentSupervisor>,
    publication: Option<PersistentPublication>,
    catalog: ProviderCatalog,
    request_counter: AtomicU64,
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
            persistent: None,
            publication: None,
            catalog,
            request_counter: AtomicU64::new(0),
        }
    }

    /// Construct from a persistent startup result and catalog. On success,
    /// takes ownership of the supervisor and stores the publication snapshot.
    /// On failure, no supervisor is held and no processes leak.
    #[must_use]
    pub fn from_startup(result: PersistentStartupResult, catalog: ProviderCatalog) -> Self {
        match result {
            PersistentStartupResult::Started {
                supervisor,
                publication,
            } => Self {
                persistent: Some(supervisor),
                publication: Some(publication),
                catalog,
                request_counter: AtomicU64::new(0),
            },
            PersistentStartupResult::Failed(_) => Self {
                persistent: None,
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

    /// Whether any persistent provider is owned and ready.
    #[must_use]
    pub fn has_persistent(&self) -> bool {
        self.persistent.is_some()
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
    /// Returns [`RequestIdError`](crate::runtime::provider::identifiers::RequestIdError)
    /// if the counter overflows the request-id digit bound (practically never).
    pub fn build_one_shot(
        &self,
        descriptor: &ProviderActionDescriptor,
        invocation: &crate::domain::effects::ProviderInvocation,
    ) -> Result<
        crate::runtime::provider::supervisor::OneShotRequest,
        crate::runtime::provider::identifiers::RequestIdError,
    > {
        let counter = self.next_request_counter();
        build_one_shot_request(descriptor, invocation, counter)
    }

    /// Shut down every persistent candidate and reap the process trees.
    /// Idempotent. Must be called before host exit.
    pub fn shutdown(&mut self) {
        if let Some(supervisor) = self.persistent.as_mut() {
            supervisor.shutdown();
        }
    }
}

impl std::fmt::Debug for ProviderCoordinator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderCoordinator")
            .field("has_persistent", &self.persistent.is_some())
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
