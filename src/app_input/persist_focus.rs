//! Persistence helpers: state serialization and pane-focus conversion.
//!
//! `pane_focus_to_persisted` / `pane_focus_from_persisted` bridge the state-layer
//! `PaneFocus` enum and the string form stored in the persisted `State` DTO. They
//! live in the app-shell layer (not in `persistence/`) because the persistence
//! module is restricted to `domain/` dependencies and cannot reference
//! `state::PaneFocus`. See issue #160.

use tracing::warn;

use jefe::persistence::{PersistenceManager, State as PersistedState};
use jefe::state::PaneFocus;

use super::SharedContext;

/// Serialize a `PaneFocus` to its persisted string form.
#[must_use]
pub fn pane_focus_to_persisted(focus: PaneFocus) -> String {
    match focus {
        PaneFocus::Repositories => "repositories",
        PaneFocus::Agents => "agents",
        PaneFocus::Terminal => "terminal",
    }
    .to_owned()
}

/// Parse a persisted pane-focus string back into `PaneFocus`.
///
/// Unknown or empty strings (e.g. older state files written before this field
/// existed) fall back to `Repositories`, matching the pre-existing default.
#[must_use]
pub fn pane_focus_from_persisted(value: &str) -> PaneFocus {
    match value {
        "agents" => PaneFocus::Agents,
        "terminal" => PaneFocus::Terminal,
        _ => PaneFocus::Repositories,
    }
}

/// Persist the current state to disk via the shared context's persistence
/// manager. Failures are logged but never crash the app.
pub fn persist_state(ctx: &SharedContext, persisted: &PersistedState) {
    if let Some(ctx_arc) = ctx
        && let Ok(ctx_guard) = ctx_arc.lock()
        && let Err(e) = ctx_guard.persistence.save_state(persisted)
    {
        warn!(error = %e, "could not save state");
    }
}
