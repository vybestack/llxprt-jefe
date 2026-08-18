use super::{
    ActionId, ActionRegistrySnapshot, ContextId, ContextStack, RegistryDiagnostic, Resolution,
};
use super::{
    action_registry_validate::find_resolved, action_registry_validate::validate_declared_bindings,
};
use crate::domain::keymap::Chord;

impl ActionRegistrySnapshot {
    /// Validate the exact action pairs one screen asks to make reachable.
    ///
    /// Declarations are checked after Settings overrides, so publication
    /// cannot retain a request that became unbound or ambiguous with another
    /// requested action or the screen's host fallback stack.
    pub(crate) fn validate_declared_bindings(
        &self,
        declared: &[(ContextId, ActionId)],
        fallback: &ContextStack,
    ) -> Result<(), RegistryDiagnostic> {
        validate_declared_bindings(&self.actions, &self.bindings, declared, fallback)
    }

    /// Resolve one chord only when it belongs to this exact requested pair.
    ///
    /// This keeps the immutable registry authoritative without activating the
    /// rest of the requested action's source context.
    #[must_use]
    pub(crate) fn resolve_declared(
        &self,
        chord: &Chord,
        context: &ContextId,
        action: &ActionId,
    ) -> Resolution {
        find_resolved(&self.resolved, context, chord)
            .filter(|binding| {
                matches!(
                    &binding.2,
                    Resolution::Dispatch { action: resolved, .. }
                        | Resolution::Unavailable { action: resolved, .. }
                        if resolved == action
                )
            })
            .map_or(Resolution::Unbound, |binding| binding.2.clone())
    }
}
