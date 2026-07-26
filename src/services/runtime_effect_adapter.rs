//! Runtime-family effect adapter (issue #381 S9).
//!
//! Executes committed [`RuntimeEffect`] operations against the shell-owned
//! [`RuntimeManager`] and reports typed completions. The adapter borrows only
//! the runtime handle: callers must have released every state/context guard
//! before running effects, so no adapter I/O can happen while `AppState` is
//! borrowed (CW01-10).
//!
//! Families this composition slice has not wired report a typed
//! [`EffectErrorKind::Unavailable`] completion instead of being silently
//! dropped, keeping the closed-effect contract observable end-to-end.

use crate::domain::effects::{
    Effect, EffectError, EffectErrorKind, EffectResponse, IssuedEffect, RuntimeEffect,
    RuntimeResponse,
};
use crate::runtime::RuntimeManager;

use super::effect_executor::{AdapterExecution, EffectAdapter};

/// Adapter executing runtime-family effects against a [`RuntimeManager`].
pub struct RuntimeEffectAdapter<'a, R: RuntimeManager + ?Sized> {
    /// Shell-owned runtime handle; never stored in state.
    pub runtime: &'a mut R,
}

impl<R: RuntimeManager + ?Sized> EffectAdapter for RuntimeEffectAdapter<'_, R> {
    fn execute(&mut self, issued: &IssuedEffect) -> AdapterExecution {
        let result = match &issued.effect {
            Effect::Runtime(RuntimeEffect::KillSession { agent_id }) => self
                .runtime
                .kill(agent_id)
                .map(|()| EffectResponse::Runtime(RuntimeResponse::Killed))
                .map_err(|error| {
                    EffectError::new(EffectErrorKind::Unavailable, false, &error.to_string())
                }),
            other => Err(EffectError::new(
                EffectErrorKind::Unavailable,
                false,
                &format!(
                    "{:?} effects are not wired in this composition",
                    other.family()
                ),
            )),
        };
        AdapterExecution::Completed(result)
    }
}
