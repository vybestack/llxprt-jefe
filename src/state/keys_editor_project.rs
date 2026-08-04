//! The Keys editor's pure projection, capture rule, and typed intents
//! (issue #388, CW-08).
//!
//! One immutable action-registry snapshot and one candidate document become one
//! row per action/context pair. Nothing here validates a chord, decides a
//! conflict, or enforces a limit: the action/key resolver composes the
//! candidate and refuses it with `KEY-E401`, and this only says what the rows
//! are and what the user asked for.

use crate::domain::action_registry::{
    ActionId, ActionRegistrySnapshot, Availability, PROTECTED_ACTION_REASON, Provenance,
};
use crate::domain::input_context::ContextId;
use crate::domain::keymap::{Chord, Key, ModifierSet};
use crate::persistence::settings_document::PublishedSettings;

#[cfg(test)]
#[path = "keys_editor_project_tests.rs"]
mod keys_editor_project_tests;

/// The action that leaves the session, which no capture may take.
pub const EMERGENCY_EXIT_ACTION: &str = "core.emergency-exit";

/// One action/context binding as the editor presents it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyEditorRow {
    /// The context the binding applies in.
    pub context: ContextId,
    /// The action the binding dispatches.
    pub action: ActionId,
    /// The action's label, from the inventory.
    pub label: String,
    /// The chords currently bound, in order; empty means unbound.
    pub chords: Vec<Chord>,
    /// Whether the action can be dispatched at all.
    pub availability: Availability,
    /// Why this row is read-only, when it is.
    pub protected: Option<String>,
    /// Where the effective chords came from.
    pub provenance: Provenance,
}

impl KeyEditorRow {
    /// Whether the resolver reports this action as unavailable.
    #[must_use]
    pub const fn availability_unavailable(&self) -> bool {
        matches!(self.availability, Availability::Unavailable { .. })
    }
}

/// One typed intent the Keys editor emits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyIntent {
    /// Bind exactly this one captured chord.
    CaptureSingleChord {
        /// The context to bind in.
        context: ContextId,
        /// The action to bind.
        action: ActionId,
        /// The single canonical chord that was captured.
        chord: Chord,
    },
    /// Replace this binding's whole chord list.
    SetChords {
        /// The context to bind in.
        context: ContextId,
        /// The action to bind.
        action: ActionId,
        /// The canonical chords, in order.
        chords: Vec<Chord>,
    },
    /// Bind nothing, so the action has no chord at all.
    Unbind {
        /// The context to unbind in.
        context: ContextId,
        /// The action to unbind.
        action: ActionId,
    },
    /// Remove the assignment so the compiled chords are inherited again.
    Reset {
        /// The context to reset.
        context: ContextId,
        /// The action to reset.
        action: ActionId,
    },
}

impl KeyIntent {
    /// The binding this intent names.
    #[must_use]
    pub const fn binding(&self) -> (&ContextId, &ActionId) {
        match self {
            Self::CaptureSingleChord {
                context, action, ..
            }
            | Self::SetChords {
                context, action, ..
            }
            | Self::Unbind { context, action }
            | Self::Reset { context, action } => (context, action),
        }
    }
}

/// What one key press during a capture means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureOutcome {
    /// This is the one chord the capture was waiting for.
    Captured(Chord),
    /// The user withdrew the capture.
    Cancelled,
    /// The chord leaves the session and is never taken by a capture.
    Protected,
}

/// Decide what one chord means while a capture is waiting.
///
/// A capture takes exactly the next chord, so this is a total function of one
/// press rather than a small state machine. A press that carries only modifiers
/// never arrives here at all: a [`Chord`] has no spelling for "a modifier and
/// nothing else", so the boundary that builds one has nothing to pass on.
#[must_use]
pub fn classify_capture(chord: Chord) -> CaptureOutcome {
    if chord == exit_chord() {
        return CaptureOutcome::Protected;
    }
    if chord.modifiers == ModifierSet::empty() && chord.key == Key::Esc {
        return CaptureOutcome::Cancelled;
    }
    CaptureOutcome::Captured(chord)
}

/// The chord that leaves the session.
fn exit_chord() -> Chord {
    Chord::new(
        ModifierSet::from_modifier(crate::domain::keymap::Modifier::Ctrl),
        Key::Char('q'),
    )
}

/// Project one registry snapshot and one candidate document into editor rows.
///
/// Every action the inventory declares gets a row, bound or not, because an
/// action with no chord is exactly the thing a user opens this editor to fix.
/// Rows are ordered by context then action, so the list does not reshuffle when
/// a binding changes.
#[must_use]
pub fn project_keys(
    snapshot: &ActionRegistrySnapshot,
    published: &PublishedSettings,
) -> Vec<KeyEditorRow> {
    let mut rows: Vec<KeyEditorRow> = snapshot
        .actions()
        .iter()
        .flat_map(|action| {
            action
                .contexts
                .iter()
                .map(move |context| (context.clone(), action))
        })
        .map(|(context, action)| KeyEditorRow {
            chords: effective_chords(snapshot, published, &context, &action.id),
            availability: availability(snapshot, &action.id),
            protected: action.protected.then(|| PROTECTED_ACTION_REASON.to_owned()),
            provenance: provenance(snapshot, published, &context, &action.id),
            label: action.label.clone(),
            action: action.id.clone(),
            context,
        })
        .collect();
    rows.sort_by(|left, right| {
        left.context
            .as_str()
            .cmp(right.context.as_str())
            .then_with(|| left.action.as_str().cmp(right.action.as_str()))
    });
    rows
}

/// The complete `KEY-E401` detail one chord conflict carries.
///
/// The resolver decides that a conflict exists; this only writes it down in the
/// one shape the screen and the diagnostics section both read, so a user can
/// see which two owners want the chord and where the losing one came from.
#[must_use]
pub fn conflict_detail(
    context: &ContextId,
    chord: Chord,
    first: &ActionId,
    second: &ActionId,
    provenance: &Provenance,
) -> String {
    format!(
        "KEY-E401: {} in context {} is claimed by both {} and {} ({})",
        chord.to_canonical_text(),
        context.as_str(),
        first.as_str(),
        second.as_str(),
        source_of(provenance),
    )
}

fn source_of(provenance: &Provenance) -> &str {
    match provenance {
        Provenance::Compiled => "compiled default",
        Provenance::Settings { source } => source,
    }
}

/// The chords this binding would have if the candidate were saved.
///
/// The snapshot is the registry this session started with; the candidate is
/// what a save would make authoritative. A row that showed the snapshot would
/// present the user's own unsaved rebinding as not having happened — and an
/// unbind, which writes an empty list, would look like no change at all.
///
/// A chord the candidate spells but this grammar cannot read is skipped rather
/// than guessed at; the action/key resolver refuses the candidate for the same
/// reason, and that refusal is what the user is shown.
fn effective_chords(
    snapshot: &ActionRegistrySnapshot,
    published: &PublishedSettings,
    context: &ContextId,
    action: &ActionId,
) -> Vec<Chord> {
    if let Some(drafted) = published
        .keymap
        .get(context.as_str())
        .and_then(|actions| actions.get(action.as_str()))
    {
        return drafted
            .iter()
            .filter_map(|text| Chord::parse(text).ok())
            .collect();
    }
    binding(snapshot, context, action).map_or_else(Vec::new, |binding| binding.chords.clone())
}

/// Where this binding's effective chords came from.
///
/// The snapshot answers for the bindings it composed. A row the snapshot has no
/// binding for still reports the document's provenance when the document names
/// it, which is how an unbound-by-the-user action reads as the user's own
/// decision rather than as a compiled default.
fn provenance(
    snapshot: &ActionRegistrySnapshot,
    published: &PublishedSettings,
    context: &ContextId,
    action: &ActionId,
) -> Provenance {
    if let Some(binding) = binding(snapshot, context, action) {
        return binding.provenance.clone();
    }
    if published
        .keymap
        .get(context.as_str())
        .is_some_and(|actions| actions.contains_key(action.as_str()))
    {
        return Provenance::Settings {
            source: "settings".to_owned(),
        };
    }
    Provenance::Compiled
}

fn binding<'snapshot>(
    snapshot: &'snapshot ActionRegistrySnapshot,
    context: &ContextId,
    action: &ActionId,
) -> Option<&'snapshot crate::domain::action_registry::Binding> {
    snapshot
        .effective_bindings()
        .iter()
        .find(|binding| &binding.context == context && &binding.action == action)
}

fn availability(snapshot: &ActionRegistrySnapshot, action: &ActionId) -> Availability {
    snapshot
        .availability_entries()
        .iter()
        .find(|entry| entry.action() == action)
        .map_or(Availability::Available, |entry| {
            entry.availability().clone()
        })
}
