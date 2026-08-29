//! Composition-time validation for the action registry.
//!
//! Extracted from `action_registry.rs` to keep it below the source-size hard
//! limit, which it had reached exactly. These are the checks
//! [`super::RegistryCandidate::compose`] runs before it publishes a snapshot;
//! they are private to the registry and cohesive enough to move as one piece.

use crate::domain::input_context::{ContextId, ContextStack};
use crate::domain::keymap::{Chord, MAX_CHORDS_PER_BINDING, MAX_EFFECTIVE_BINDINGS};

use super::action_registry_chord_cmp::{chords_equivalent, terminal_intercepts};
use super::{
    Action, ActionId, Availability, AvailabilityGeneration, Binding, BindingOverride,
    RegistryDiagnostic, RegistryDiagnosticKind, Resolution, ResolvedBinding,
};

fn diagnostic(kind: RegistryDiagnosticKind) -> RegistryDiagnostic {
    RegistryDiagnostic(kind)
}

pub(super) fn validate_actions_and_bindings(
    actions: &[Action],
    bindings: &[Binding],
) -> Result<(), RegistryDiagnostic> {
    for (index, action) in actions.iter().enumerate() {
        if actions[..index].iter().any(|seen| seen.id == action.id) {
            return Err(diagnostic(RegistryDiagnosticKind::DuplicateAction(
                action.id.clone(),
            )));
        }
    }
    for (index, binding) in bindings.iter().enumerate() {
        validate_action_context(actions, &binding.action, &binding.context)?;
        validate_chord_list(
            binding.context.clone(),
            binding.action.clone(),
            &binding.chords,
        )?;
        if bindings[..index]
            .iter()
            .any(|seen| seen.context == binding.context && seen.action == binding.action)
        {
            return Err(diagnostic(RegistryDiagnosticKind::DuplicateBinding(
                binding.context.clone(),
                binding.action.clone(),
            )));
        }
    }
    Ok(())
}

fn validate_action_context(
    actions: &[Action],
    action_id: &ActionId,
    context: &ContextId,
) -> Result<(), RegistryDiagnostic> {
    let Some(action) = find_action(actions, action_id) else {
        return Err(diagnostic(RegistryDiagnosticKind::UnknownAction(
            action_id.clone(),
        )));
    };
    if !action.contexts.contains(context) {
        return Err(diagnostic(RegistryDiagnosticKind::ActionContextMismatch(
            context.clone(),
            action_id.clone(),
        )));
    }
    Ok(())
}

pub(super) fn validate_context_stacks(
    actions: &[Action],
    stacks: &[ContextStack],
) -> Result<(), RegistryDiagnostic> {
    for stack in stacks {
        for context in stack.iter() {
            if !actions
                .iter()
                .any(|action| action.contexts.contains(context))
            {
                return Err(diagnostic(RegistryDiagnosticKind::UnknownContext(
                    context.clone(),
                )));
            }
        }
    }
    Ok(())
}

pub(super) fn validate_overrides(
    actions: &[Action],
    overrides: &[BindingOverride],
) -> Result<(), RegistryDiagnostic> {
    for (index, candidate) in overrides.iter().enumerate() {
        let known_context = actions
            .iter()
            .any(|action| action.contexts.contains(&candidate.context));
        if !known_context {
            return Err(diagnostic(RegistryDiagnosticKind::UnknownContext(
                candidate.context.clone(),
            )));
        }
        validate_action_context(actions, &candidate.action, &candidate.context)?;
        validate_chord_list(
            candidate.context.clone(),
            candidate.action.clone(),
            &candidate.chords,
        )?;
        if overrides[..index]
            .iter()
            .any(|seen| seen.context == candidate.context && seen.action == candidate.action)
        {
            return Err(diagnostic(RegistryDiagnosticKind::DuplicateOverride(
                candidate.context.clone(),
                candidate.action.clone(),
            )));
        }
    }
    Ok(())
}

fn validate_chord_list(
    context: ContextId,
    action: ActionId,
    chords: &[Chord],
) -> Result<(), RegistryDiagnostic> {
    if chords.len() > MAX_CHORDS_PER_BINDING {
        return Err(diagnostic(RegistryDiagnosticKind::TooManyChords(
            context,
            action,
            chords.len(),
            MAX_CHORDS_PER_BINDING,
        )));
    }
    for (index, chord) in chords.iter().enumerate() {
        if chords[..index]
            .iter()
            .any(|seen| chords_equivalent(seen, chord))
        {
            return Err(diagnostic(RegistryDiagnosticKind::DuplicateChord(
                context, action, *chord,
            )));
        }
    }
    Ok(())
}

pub(super) fn apply_overrides(bindings: &mut Vec<Binding>, overrides: &[BindingOverride]) {
    for candidate in overrides {
        bindings.retain(|binding| {
            binding.context != candidate.context || binding.action != candidate.action
        });
        if candidate.chords.is_empty() {
            continue;
        }
        bindings.push(Binding {
            context: candidate.context.clone(),
            action: candidate.action.clone(),
            chords: candidate.chords.clone(),
            provenance: candidate.provenance.clone(),
        });
    }
}

pub(super) fn validate_effective_binding_count(
    bindings: &[Binding],
) -> Result<(), RegistryDiagnostic> {
    let count = bindings.iter().map(|binding| binding.chords.len()).sum();
    if count > MAX_EFFECTIVE_BINDINGS {
        Err(diagnostic(
            RegistryDiagnosticKind::TooManyEffectiveBindings(count, MAX_EFFECTIVE_BINDINGS),
        ))
    } else {
        Ok(())
    }
}

pub(super) fn validate_declared_bindings(
    actions: &[Action],
    bindings: &[Binding],
    declared: &[(ContextId, ActionId)],
    fallback: &ContextStack,
) -> Result<(), RegistryDiagnostic> {
    for (index, (context, action_id)) in declared.iter().enumerate() {
        let binding = declared_binding(actions, bindings, &declared[..index], context, action_id)?;
        validate_prior_declaration_conflicts(bindings, &declared[..index], binding)?;
        validate_fallback_conflicts(actions, bindings, fallback, binding)?;
    }
    Ok(())
}

fn declared_binding<'a>(
    actions: &[Action],
    bindings: &'a [Binding],
    prior: &[(ContextId, ActionId)],
    context: &ContextId,
    action_id: &ActionId,
) -> Result<&'a Binding, RegistryDiagnostic> {
    if prior
        .iter()
        .any(|(prior_context, prior_action)| prior_context == context && prior_action == action_id)
    {
        return Err(diagnostic(RegistryDiagnosticKind::DuplicateBinding(
            context.clone(),
            action_id.clone(),
        )));
    }
    let action = find_action(actions, action_id)
        .ok_or_else(|| diagnostic(RegistryDiagnosticKind::UnknownAction(action_id.clone())))?;
    if !action.contexts.contains(context) {
        return Err(diagnostic(RegistryDiagnosticKind::ActionContextMismatch(
            context.clone(),
            action_id.clone(),
        )));
    }
    if action.protected {
        return Err(diagnostic(RegistryDiagnosticKind::ProtectedDeclared(
            action_id.clone(),
            context.clone(),
        )));
    }
    bindings
        .iter()
        .find(|binding| {
            binding.context == *context
                && binding.action == *action_id
                && !binding.chords.is_empty()
        })
        .ok_or_else(|| {
            diagnostic(RegistryDiagnosticKind::DeclaredUnbound(
                action_id.clone(),
                context.clone(),
            ))
        })
}

fn validate_prior_declaration_conflicts(
    bindings: &[Binding],
    prior: &[(ContextId, ActionId)],
    binding: &Binding,
) -> Result<(), RegistryDiagnostic> {
    for (prior_context, prior_action) in prior {
        if *prior_action == binding.action {
            continue;
        }
        let Some(candidate) = bindings.iter().find(|candidate| {
            candidate.context == *prior_context
                && candidate.action == *prior_action
                && !candidate.chords.is_empty()
        }) else {
            continue;
        };
        if let Some(chord) = overlapping_chord(candidate, binding) {
            return Err(diagnostic(RegistryDiagnosticKind::ImplicitShadow(
                candidate.context.clone(),
                binding.context.clone(),
                chord,
            )));
        }
    }
    Ok(())
}

fn validate_fallback_conflicts(
    actions: &[Action],
    bindings: &[Binding],
    fallback: &ContextStack,
    binding: &Binding,
) -> Result<(), RegistryDiagnostic> {
    let mut implicit_shadow = None;
    for fallback_context in fallback.iter() {
        for host in bindings.iter().filter(|candidate| {
            candidate.context == *fallback_context && candidate.action != binding.action
        }) {
            let Some(chord) = overlapping_chord(binding, host) else {
                continue;
            };
            if binding_is_protected(actions, host) {
                return Err(diagnostic(RegistryDiagnosticKind::ProtectedShadowed(
                    host.action.clone(),
                    host.context.clone(),
                    chord,
                )));
            }
            implicit_shadow.get_or_insert_with(|| {
                RegistryDiagnosticKind::ImplicitShadow(
                    binding.context.clone(),
                    host.context.clone(),
                    chord,
                )
            });
        }
    }
    implicit_shadow.map_or(Ok(()), |kind| Err(diagnostic(kind)))
}

pub(super) fn validate_context_conflicts(bindings: &[Binding]) -> Result<(), RegistryDiagnostic> {
    for (index, first) in bindings.iter().enumerate() {
        for second in &bindings[index + 1..] {
            if first.context != second.context {
                continue;
            }
            if let Some(chord) = overlapping_chord(first, second) {
                return Err(diagnostic(RegistryDiagnosticKind::ContextConflict(
                    first.context.clone(),
                    chord,
                    first.action.clone(),
                    second.action.clone(),
                )));
            }
        }
    }
    Ok(())
}

pub(super) fn validate_availability(
    actions: &[Action],
    generation: AvailabilityGeneration,
) -> Result<AvailabilityGeneration, RegistryDiagnostic> {
    for (index, entry) in generation.1.iter().enumerate() {
        if find_action(actions, &entry.0).is_none() {
            return Err(diagnostic(RegistryDiagnosticKind::UnknownAvailability(
                entry.0.clone(),
            )));
        }
        if generation.1[..index].iter().any(|seen| seen.0 == entry.0) {
            return Err(diagnostic(RegistryDiagnosticKind::DuplicateAvailability(
                entry.0.clone(),
            )));
        }
    }
    for action in actions {
        if !generation.1.iter().any(|entry| entry.0 == action.id) {
            return Err(diagnostic(RegistryDiagnosticKind::MissingAvailability(
                action.id.clone(),
            )));
        }
    }
    Ok(generation)
}

pub(super) fn validate_cross_contexts(
    actions: &[Action],
    bindings: &[Binding],
    overrides: &[BindingOverride],
    stacks: &[ContextStack],
) -> Result<(), RegistryDiagnostic> {
    for stack in stacks {
        let contexts: Vec<_> = stack.iter().collect();
        for child_index in 0..contexts.len() {
            for parent in &contexts[child_index + 1..] {
                validate_context_pair(actions, bindings, overrides, contexts[child_index], parent)?;
            }
        }
    }
    Ok(())
}

fn validate_context_pair(
    actions: &[Action],
    bindings: &[Binding],
    overrides: &[BindingOverride],
    child: &ContextId,
    parent: &ContextId,
) -> Result<(), RegistryDiagnostic> {
    for child_binding in bindings.iter().filter(|binding| binding.context == *child) {
        for parent_binding in bindings.iter().filter(|binding| binding.context == *parent) {
            let Some(chord) = overlapping_chord(child_binding, parent_binding) else {
                continue;
            };
            let protected_child = binding_is_protected(actions, child_binding);
            let protected_parent = binding_is_protected(actions, parent_binding);
            if !protected_child && protected_parent {
                return Err(diagnostic(RegistryDiagnosticKind::ProtectedShadowed(
                    parent_binding.action.clone(),
                    parent_binding.context.clone(),
                    chord,
                )));
            }
            let parent_changed = override_exists(overrides, parent_binding);
            let child_changed = override_exists(overrides, child_binding);
            if parent_changed && !child_changed {
                return Err(diagnostic(RegistryDiagnosticKind::ImplicitShadow(
                    child.clone(),
                    parent.clone(),
                    chord,
                )));
            }
        }
    }
    Ok(())
}

pub(super) fn validate_protected(
    actions: &[Action],
    bindings: &[Binding],
    availability: &AvailabilityGeneration,
) -> Result<(), RegistryDiagnostic> {
    for action in actions.iter().filter(|action| protected_action(action)) {
        for context in &action.contexts {
            if !bindings
                .iter()
                .any(|binding| binding.context == *context && binding.action == action.id)
            {
                return Err(diagnostic(RegistryDiagnosticKind::ProtectedUnbound(
                    action.id.clone(),
                    context.clone(),
                )));
            }
        }
        if availability.1.iter().any(|entry| {
            entry.0 == action.id && matches!(entry.1, Availability::Unavailable { .. })
        }) {
            return Err(diagnostic(RegistryDiagnosticKind::ProtectedUnavailable(
                action.id.clone(),
            )));
        }
    }
    Ok(())
}

fn find_action<'a>(actions: &'a [Action], id: &ActionId) -> Option<&'a Action> {
    actions.iter().find(|action| action.id == *id)
}

pub(super) fn build_resolved(
    actions: &[Action],
    bindings: &[Binding],
    availability: &AvailabilityGeneration,
) -> Result<Vec<ResolvedBinding>, RegistryDiagnostic> {
    let mut resolved = Vec::new();
    for binding in bindings {
        let action = find_action(actions, &binding.action).ok_or_else(|| {
            diagnostic(RegistryDiagnosticKind::UnknownAction(
                binding.action.clone(),
            ))
        })?;
        let entry = availability
            .1
            .iter()
            .find(|entry| entry.0 == binding.action)
            .ok_or_else(|| {
                diagnostic(RegistryDiagnosticKind::MissingAvailability(
                    binding.action.clone(),
                ))
            })?;
        let outcome = action_resolution(action, &entry.1);
        for chord in &binding.chords {
            resolved.push(ResolvedBinding(
                binding.context.clone(),
                *chord,
                outcome.clone(),
                terminal_intercepts(action, chord),
            ));
        }
    }
    Ok(resolved)
}

fn action_resolution(action: &Action, availability: &Availability) -> Resolution {
    match availability {
        Availability::Available => Resolution::Dispatch {
            action: action.id.clone(),
            handler: action.handler,
        },
        Availability::Unavailable { reason } => Resolution::Unavailable {
            action: action.id.clone(),
            reason: reason.clone(),
        },
    }
}

pub(super) fn find_resolved<'a>(
    bindings: &'a [ResolvedBinding],
    context: &ContextId,
    chord: &Chord,
) -> Option<&'a ResolvedBinding> {
    bindings
        .iter()
        .find(|binding| binding.0 == *context && chords_equivalent(&binding.1, chord))
}

fn overlapping_chord(first: &Binding, second: &Binding) -> Option<Chord> {
    first.chords.iter().find_map(|first_chord| {
        second
            .chords
            .iter()
            .any(|second_chord| chords_equivalent(first_chord, second_chord))
            .then_some(*first_chord)
    })
}

fn override_exists(overrides: &[BindingOverride], binding: &Binding) -> bool {
    overrides
        .iter()
        .any(|candidate| candidate.context == binding.context && candidate.action == binding.action)
}

fn binding_is_protected(actions: &[Action], binding: &Binding) -> bool {
    find_action(actions, &binding.action).is_some_and(protected_action)
}

/// The `protected` flag is the single authority. Re-listing action IDs here
/// would let an inventory row that forgot the flag look protected anyway, which
/// hides the real defect; the inventory tests assert the flag instead.
fn protected_action(action: &Action) -> bool {
    action.protected
}
