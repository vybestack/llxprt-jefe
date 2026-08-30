//! Sole reducer boundary for screen navigation (issue #386).
//!
//! Enter, switch, Back, and composition-root fallback all dispatch through
//! [`reduce_navigation`], keeping stack, generation, and dirty-guard state in
//! one authority.

use crate::domain::effects::{Correlation, EffectError};
use crate::workbench::{ActivationValues, RouteId, ScreenId, ScreenIdentity};

use super::AppState;
use super::navigation::{Activation, NavIntent, NavMessage, NavOutcome, reduce_navigation};
use super::navigation_dirty::{DirtyChoice, DraftAction, DraftToken, SaveIntent};

/// Whether an outcome actually changed which instance is current.
const fn moved(outcome: &NavOutcome) -> bool {
    matches!(
        outcome,
        NavOutcome::Pushed { .. } | NavOutcome::Replaced { .. } | NavOutcome::Restored { .. }
    )
}

impl AppState {
    /// The screen the session is on, as the open descriptor identity.
    ///
    /// Reads route through the navigation authority rather than a field of
    /// their own, so there is exactly one answer to "which screen is this".
    /// The identity is open: a compiled screen, a lowered user screen, or a
    /// lowered package screen. Built-in-only consumers that require a compiled
    /// [`ScreenId`] use [`AppState::compiled_screen`] instead, so a non-compiled
    /// screen is never silently treated as a compiled one.
    #[must_use]
    pub const fn screen(&self) -> ScreenIdentity {
        self.nav.screen()
    }

    /// The compiled screen the session is on, if it is on one.
    ///
    /// Returns `None` when the active screen is a lowered package or custom
    /// screen, so a built-in-only caller fails fast onto its non-built-in path
    /// rather than defaulting to a screen it cannot render or route.
    #[must_use]
    pub const fn compiled_screen(&self) -> Option<ScreenId> {
        self.nav.compiled_screen()
    }

    /// Open `screen`, keeping the current one to come back to.
    pub fn enter_screen(&mut self, screen: ScreenId) -> DraftAction {
        let activation = self.activation_for(screen);
        self.navigate(NavMessage::Navigate(NavIntent::Push(activation)))
    }

    /// Enter a compiled route with validated provider-supplied activation values.
    pub fn enter_provider_route(
        &mut self,
        route: RouteId,
        values: ActivationValues,
    ) -> DraftAction {
        let activation = Activation::from_source(route, values, self.nav.current());
        self.navigate(NavMessage::Navigate(NavIntent::Push(activation)))
    }

    /// Ensure the session is on `screen`, without stacking a second copy of it.
    ///
    /// Some transitions state where the session should end up rather than that
    /// it should move — hiding a shell returns to the terminal manager whether
    /// or not the manager is already the current screen. Pushing in that case
    /// would stack a second instance of the screen the user is already looking
    /// at, and repeating it would fill the stack.
    pub fn show_screen(&mut self, screen: ScreenId) -> DraftAction {
        if self.nav.screen() == screen {
            return DraftAction::None;
        }
        self.enter_screen(screen)
    }

    pub(crate) fn show_dashboard(&mut self) -> DraftAction {
        if self.current_is_composition_root() {
            return DraftAction::None;
        }
        let activation = Activation::from_source(
            self.composition_root_route(),
            ActivationValues::empty(),
            self.nav.current(),
        );
        self.navigate(NavMessage::Navigate(NavIntent::Push(activation)))
    }

    /// Ensure the session is on the Terminal Manager, without stacking a
    /// second copy of it.
    ///
    /// The manager is a descriptor screen (`core.terminals`), so navigation
    /// goes through its route rather than a compiled variant; stating the
    /// destination idempotently keeps shell-return transitions from stacking
    /// a second manager instance the user is already looking at.
    pub(crate) fn show_terminal_manager(&mut self) -> DraftAction {
        if self.nav.screen() == crate::workbench::TERMINALS_IDENTITY {
            return DraftAction::None;
        }
        let route = RouteId::parse("terminals")
            .unwrap_or_else(|error| unreachable!("the core.terminals route must parse: {error}"));
        let activation =
            Activation::from_source(route, ActivationValues::empty(), self.nav.current());
        self.navigate(NavMessage::Navigate(NavIntent::Push(activation)))
    }

    fn switch_to_composition_root(&mut self) -> DraftAction {
        let activation = Activation::from_source(
            self.composition_root_route(),
            ActivationValues::empty(),
            self.nav.current(),
        );
        self.navigate(NavMessage::Navigate(NavIntent::Replace(activation)))
    }

    /// Move to `screen` in place of the current one.
    pub fn switch_screen(&mut self, screen: ScreenId) -> DraftAction {
        let activation = self.activation_for(screen);
        self.navigate(NavMessage::Navigate(NavIntent::Replace(activation)))
    }

    /// Return to the screen underneath this one.
    ///
    /// A restored session opens directly on whatever screen it was last on,
    /// with nothing beneath it, and leaving that screen has always taken the
    /// user home rather than stranding them. So leaving means: go back if
    /// there is somewhere to go back to, otherwise go home — and if this
    /// already is home, stay.
    pub fn leave_screen(&mut self) -> DraftAction {
        if self.nav.depth() == 0 && !self.current_is_composition_root() {
            return self.switch_to_composition_root();
        }
        self.navigate(NavMessage::Navigate(NavIntent::Back))
    }

    pub(super) fn current_is_composition_root(&self) -> bool {
        self.published_workbench()
            .screen_registry()
            .initial_screen()
            .is_some_and(|descriptor| descriptor.id == self.screen())
    }

    fn composition_root_route(&self) -> RouteId {
        let Some(descriptor) = self
            .published_workbench()
            .screen_registry()
            .initial_screen()
        else {
            std::process::abort();
        };
        descriptor.route
    }

    /// Record that this screen now holds unsaved work.
    pub fn mark_screen_dirty(&mut self, draft: DraftToken, save: SaveIntent) -> DraftAction {
        self.navigate(NavMessage::MarkDirty { draft, save })
    }

    /// Record that this screen no longer holds unsaved work.
    pub fn mark_screen_clean(&mut self) -> DraftAction {
        self.navigate(NavMessage::MarkClean)
    }

    /// Answer the dirty guard.
    pub fn resolve_dirty(&mut self, choice: DirtyChoice) -> DraftAction {
        self.navigate(NavMessage::ResolveDirty(choice))
    }

    /// Tell the dirty guard which save attempt the owner actually registered.
    ///
    /// The guard cannot distinguish two attempts at the same operation on the
    /// same screen until it is told the identity of the running one.
    pub fn report_save_started(&mut self, correlation: &Correlation) -> DraftAction {
        self.navigate(NavMessage::SaveStarted {
            correlation: correlation.clone(),
        })
    }

    /// Tell the dirty guard how the save it asked for turned out.
    ///
    /// A success releases the navigation the guard was holding; a failure keeps
    /// the user on the screen with their work and re-offers the choices.
    pub fn report_save_completed(
        &mut self,
        correlation: &Correlation,
        result: Result<(), EffectError>,
    ) -> DraftAction {
        self.navigate(NavMessage::SaveCompleted {
            correlation: correlation.clone(),
            result,
        })
    }

    /// An activation for `screen`, computed from the live current instance.
    ///
    /// Compiled screens declare no activation fields, so this carries no
    /// values; it carries the provenance that lets the reducer refuse a
    /// request computed against a screen that has since been replaced.
    fn activation_for(&self, screen: ScreenId) -> Activation {
        Activation::from_source(
            crate::workbench::route_of(screen),
            ActivationValues::empty(),
            self.nav.current(),
        )
    }

    /// Commit one navigation message, surfacing any refusal and reporting what
    /// the draft's owner must now do.
    fn navigate(&mut self, message: NavMessage) -> DraftAction {
        let prior = self.clone();
        let workbench = std::sync::Arc::clone(self.published_workbench());
        let registry = workbench.screen_registry();
        let placeholder = self.nav.clone();
        let transition = reduce_navigation(
            std::mem::replace(&mut self.nav, placeholder),
            registry,
            message,
        );
        self.nav = transition.state;
        if let NavOutcome::Refused(refusal) = &transition.outcome {
            self.error_message = Some(refusal.to_string());
        }
        if moved(&transition.outcome) {
            // The pending ledger has to learn what navigation just decided,
            // otherwise work started on the screen the session left would still
            // match its own record and be applied to whatever replaced it.
            let (screen_generation, activation_generation) = self.nav.live_generations();
            let dropped = self
                .pending_effects
                .adopt_live_generations(screen_generation, activation_generation);
            if dropped > 0 {
                tracing::debug!(
                    dropped,
                    screen_generation,
                    activation_generation,
                    "dropped pending work belonging to a screen the session left"
                );
            }
            if let Err(error) =
                self.update_provider_panel_lifecycle(&transition.outcome, registry, &prior)
            {
                *self = prior;
                self.error_message = Some(error);
                return DraftAction::None;
            }
            if matches!(
                &transition.outcome,
                NavOutcome::Pushed { .. } | NavOutcome::Replaced { .. }
            ) {
                self.sync_current_selected_resource();
            }
        }
        transition.draft
    }

    fn update_provider_panel_lifecycle(
        &mut self,
        outcome: &NavOutcome,
        registry: &crate::workbench::ScreenRegistry,
        prior: &Self,
    ) -> Result<(), String> {
        match outcome {
            NavOutcome::Pushed { suspended, .. } => {
                self.suspend_provider_panels(*suspended)?;
                self.activate_current_provider_panels(registry)?;
            }
            NavOutcome::Replaced { disposed, .. } => {
                self.dispose_provider_panels(prior, *disposed, true)?;
                self.activate_current_provider_panels(registry)?;
            }
            NavOutcome::Restored { disposed, restored } => {
                self.dispose_provider_panels(prior, *disposed, false)?;
                self.resume_provider_panels(*restored)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn suspend_provider_panels(
        &mut self,
        owner: crate::workbench::ScreenInstanceId,
    ) -> Result<(), String> {
        let effects = {
            let instance = self
                .nav
                .instance_mut(owner)
                .ok_or_else(|| "suspended screen instance is not live".to_owned())?;
            let panels = instance.provider_panels_mut();
            panels
                .panels_for_screen(owner.get())
                .into_iter()
                .map(|panel| panels.suspend(panel).map_err(|error| error.to_string()))
                .collect::<Result<Vec<_>, _>>()?
        };
        for effect in effects {
            self.stage_panel_deactivate(effect)?;
        }
        Ok(())
    }

    fn dispose_provider_panels(
        &mut self,
        prior: &Self,
        owner: crate::workbench::ScreenInstanceId,
        replaced: bool,
    ) -> Result<(), String> {
        let instance = prior
            .nav
            .instance(owner)
            .ok_or_else(|| "disposed screen instance was not live".to_owned())?;
        let mut panels = instance.provider_panels().clone();
        let panel_ids = panels.panels_for_screen(owner.get());
        for panel in panel_ids {
            let outcome = if replaced {
                panels.replace(panel)
            } else {
                panels.dispose(panel)
            }
            .map_err(|error| error.to_string())?;
            if let crate::state::provider_panels::DeactivateOutcome::Sent(effect) = outcome {
                self.stage_panel_deactivate(effect)?;
            }
        }
        Ok(())
    }

    fn resume_provider_panels(
        &mut self,
        owner: crate::workbench::ScreenInstanceId,
    ) -> Result<(), String> {
        let effects = {
            let instance = self
                .nav
                .instance_mut(owner)
                .ok_or_else(|| "restored screen instance is not live".to_owned())?;
            let panels = instance.provider_panels_mut();
            panels
                .panels_for_screen(owner.get())
                .into_iter()
                .map(|panel| {
                    panels
                        .resume(panel)
                        .map(|activated| activated.effect)
                        .map_err(|error| error.to_string())
                })
                .collect::<Result<Vec<_>, _>>()?
        };
        for effect in effects {
            self.stage_panel_activate(effect)?;
        }
        Ok(())
    }

    fn activate_current_provider_panels(
        &mut self,
        registry: &crate::workbench::ScreenRegistry,
    ) -> Result<(), String> {
        let screen = self.nav.current().screen;
        let descriptor = registry
            .get_identity(screen)
            .ok_or_else(|| "published screen has no registry descriptor".to_owned())?;
        let current = self.nav.current().clone();
        let activation = activation_typed_map(&current.activation.values)?;
        for panel in &descriptor.panels {
            let Some(binding) = registry.panel_binding(current.screen, &panel.id) else {
                continue;
            };
            self.activate_provider_panel(&current, panel, binding, &activation)?;
        }
        Ok(())
    }

    fn activate_provider_panel(
        &mut self,
        current: &crate::state::navigation::ScreenInstance,
        panel: &crate::workbench::PanelDescriptor,
        binding: &crate::workbench::PackagePanelBinding,
        activation: &crate::domain::TypedMap,
    ) -> Result<(), String> {
        let allowed_model_kinds = binding
            .model_kinds
            .iter()
            .copied()
            .map(protocol_body_kind)
            .collect::<Vec<_>>();
        let allowed_events = binding
            .event_schema
            .iter()
            .map(|entry| crate::state::provider_panels::EventDeclaration {
                kind: panel_event_kind(entry.kind()),
                arguments: entry.arguments().to_vec(),
            })
            .collect::<Vec<_>>();
        let Some(panel_instance) = current
            .relationships()
            .and_then(|relationships| relationships.panel_instance_id(&panel.id))
        else {
            return Err("published panel has no runtime identity".to_owned());
        };
        let declared = self
            .provider_panels_mut()
            .declare_instance(
                panel_instance,
                crate::state::provider_panels::DeclareInput {
                    owner: &binding.owner,
                    panel_id: &panel.id,
                    screen_instance_id: current.id.get(),
                    panel_type: &binding.panel_type,
                    activation,
                    allowed_model_kinds: &allowed_model_kinds,
                    allowed_events: &allowed_events,
                    action_authority: &binding.action_authority,
                    process_generation:
                        crate::runtime::provider::protocol::INITIAL_PROCESS_GENERATION,
                },
            )
            .map_err(|error| error.to_string())?;
        let activated = self
            .provider_panels_mut()
            .activate(declared.instance)
            .map_err(|error| error.to_string())?;
        self.stage_panel_activate(activated.effect)
    }

    /// Validate and stage one host-owned semantic event for a live provider panel.
    pub fn submit_provider_panel_event(
        &mut self,
        panel: crate::state::provider_panels::PanelInstanceId,
        event: crate::runtime::provider::protocol::PanelEvent,
    ) -> bool {
        use crate::runtime::provider::protocol::PanelEvent;
        use crate::state::provider_panels::EventOutcome;

        let selected = match &event {
            PanelEvent::Selected { id } => Some(id.clone()),
            _ => None,
        };
        let field_change = match &event {
            PanelEvent::FieldChanged { field_id, value } => Some((field_id.clone(), value.clone())),
            _ => None,
        };
        match self.provider_panels_mut().submit_live_event(panel, event) {
            Ok(EventOutcome::Event(effect)) => {
                let local_result = if let Some(id) = selected {
                    self.update_panel_host_selection(panel, id)
                } else if let Some((field_id, value)) = field_change {
                    self.update_panel_host_field(panel, field_id, value)
                } else {
                    Ok(())
                };
                if local_result.is_err() {
                    return false;
                }
                self.stage_panel_event(effect);
                true
            }
            Ok(EventOutcome::Activate(effect)) => match self.stage_panel_activate(effect) {
                Ok(()) => true,
                Err(error) => {
                    self.error_message = Some(error);
                    false
                }
            },
            Ok(EventOutcome::None) | Err(_) => false,
        }
    }

    /// Atomically commit one provider semantic event and its declared typed relationships.
    ///
    /// Selection-capable controls project their stable semantic key through each
    /// participating output-port schema. Any invalid projection or authenticated
    /// relationship refusal leaves both the provider event and relationship state unchanged.
    pub fn submit_provider_panel_semantic_event(
        &mut self,
        panel_instance_id: crate::state::provider_panels::PanelInstanceId,
        panel_id: &crate::workbench::PanelId,
        event: crate::runtime::provider::protocol::PanelEvent,
    ) -> bool {
        let commands =
            match self.selection_relationship_commands(panel_instance_id, panel_id, &event) {
                Ok(commands) => commands,
                Err(error) => {
                    self.error_message = Some(error);
                    return false;
                }
            };
        if commands.is_empty() {
            return self.submit_provider_panel_event(panel_instance_id, event);
        }

        let mut candidate = self.clone();
        if !candidate.submit_provider_panel_event(panel_instance_id, event) {
            return false;
        }
        for command in commands {
            if let Err(error) = candidate.apply_relationship_command(command) {
                self.error_message = Some(error.to_string());
                return false;
            }
        }
        *self = candidate;
        true
    }

    pub(super) fn selection_relationship_commands(
        &self,
        panel_instance_id: crate::state::provider_panels::PanelInstanceId,
        panel_id: &crate::workbench::PanelId,
        event: &crate::runtime::provider::protocol::PanelEvent,
    ) -> Result<Vec<super::RelationshipCommand>, String> {
        let crate::runtime::provider::protocol::PanelEvent::Selected { id } = event else {
            return Ok(Vec::new());
        };
        let current = self.nav.current();
        let Some(descriptor) = self.active_relationship_descriptor(panel_id)? else {
            return Ok(Vec::new());
        };
        let semantic_key = self.provider_selection_semantic_key(panel_instance_id, id)?;
        let binding = self
            .published_workbench()
            .screen_registry()
            .panel_binding(current.screen, panel_id)
            .ok_or_else(|| "active panel has no authenticated provider binding".to_owned())?;
        if !self.provider_panels().matches_active_binding(
            panel_instance_id,
            current.id.get(),
            panel_id,
            &binding.owner,
        ) {
            return Err("selected provider control names a stale panel instance".to_owned());
        }
        let relationship_panel_instance = current
            .relationships()
            .and_then(|runtime| runtime.panel_instance_id(panel_id))
            .ok_or_else(|| {
                "active provider panel has no relationship runtime identity".to_owned()
            })?;
        let panel = descriptor
            .panels
            .iter()
            .find(|panel| panel.id == *panel_id)
            .ok_or_else(|| {
                format!("active provider panel declaration {panel_id} is unavailable")
            })?;
        let schemas = self.published_workbench().resource_schemas();
        panel
            .ports
            .iter()
            .filter(|port| {
                port.direction == crate::workbench::PortDirection::Output
                    && descriptor.relationships.iter().any(|relationship| {
                        relationship.source.panel == *panel_id
                            && relationship.source.port == port.id
                    })
            })
            .map(|port| {
                semantic_relationship_command(
                    schemas,
                    current,
                    relationship_panel_instance,
                    panel_id,
                    port,
                    &semantic_key,
                )
            })
            .collect()
    }
    fn active_relationship_descriptor(
        &self,
        panel_id: &crate::workbench::PanelId,
    ) -> Result<Option<&crate::workbench::ScreenDescriptor>, String> {
        let descriptor = self
            .published_workbench()
            .screen_registry()
            .get_identity(self.nav.current().screen)
            .ok_or_else(|| "active provider screen declaration is unavailable".to_owned())?;
        Ok(descriptor
            .relationships
            .iter()
            .any(|relationship| relationship.source.panel == *panel_id)
            .then_some(descriptor))
    }

    fn provider_selection_semantic_key(
        &self,
        panel_instance_id: crate::state::provider_panels::PanelInstanceId,
        selected_id: &crate::domain::Id,
    ) -> Result<String, String> {
        let snapshot = self
            .provider_panels()
            .accepted_snapshot(panel_instance_id)
            .ok_or_else(|| "selected provider control has no accepted snapshot".to_owned())?;
        selected_provider_semantic_key(&snapshot.body, selected_id)
            .ok_or_else(|| format!("selected provider control does not contain item {selected_id}"))
    }

    fn update_panel_host_selection(
        &mut self,
        panel: crate::state::provider_panels::PanelInstanceId,
        selected_id: crate::domain::Id,
    ) -> Result<(), crate::state::provider_panels::PanelError> {
        let prior = self.provider_panels().host_local(panel).cloned();
        let host = crate::runtime::provider::protocol::HostLocal {
            focus_target: prior.as_ref().and_then(|local| local.focus_target.clone()),
            scroll_offset: prior.as_ref().map_or(0, |local| local.scroll_offset),
            selected_id: Some(selected_id),
            form_draft: prior.and_then(|local| local.form_draft),
        };
        self.provider_panels_mut().update_host_local(panel, host)
    }

    fn update_panel_host_field(
        &mut self,
        panel: crate::state::provider_panels::PanelInstanceId,
        field_id: crate::domain::Id,
        value: crate::domain::TypedValue,
    ) -> Result<(), crate::state::provider_panels::PanelError> {
        let prior = self
            .provider_panels()
            .host_local(panel)
            .cloned()
            .unwrap_or_default();
        let mut form_draft = prior.form_draft.clone().unwrap_or_default();
        form_draft.insert(field_id.clone(), value);
        let host = crate::runtime::provider::protocol::HostLocal {
            focus_target: Some(field_id),
            form_draft: Some(form_draft),
            ..prior
        };
        self.provider_panels_mut().update_host_local(panel, host)
    }

    fn stage_panel_event(&mut self, panel_event: crate::state::provider_panels::PanelEventEffect) {
        use crate::domain::effects::{
            Effect, EffectFamily, ProviderEffect, RetryPolicy, SemanticKey,
        };

        let subject = format!("panel-event-{}", panel_event.panel_instance.as_u64());
        let owner = panel_event.owner.clone();
        let effect = Effect::Provider(ProviderEffect::PanelEvent {
            owner: panel_event.owner,
            panel_instance_id: panel_event.panel_instance.as_u64(),
            panel_generation: panel_event.generation,
            revision: panel_event.revision,
            event: domain_panel_event(panel_event.event),
        });
        let semantic_key = SemanticKey::new(EffectFamily::Provider, &subject);
        if let Err(error) =
            self.register_pending_effect(owner, semantic_key, effect, RetryPolicy::Never)
        {
            self.error_message = Some(error.to_string());
        }
    }
    fn stage_panel_activate(
        &mut self,
        activate: crate::state::provider_panels::ActivateEffect,
    ) -> Result<(), String> {
        use crate::domain::effects::{
            Effect, EffectFamily, ProviderEffect, RetryPolicy, SemanticKey,
        };
        let subject = format!("activate-panel-{}", activate.panel_instance.as_u64());
        let owner = activate.owner.clone();
        let effect = Effect::Provider(ProviderEffect::ActivatePanel {
            owner: activate.owner,
            panel_instance_id: activate.panel_instance.as_u64(),
            screen_instance_id: activate.screen_instance,
            panel_type: activate.panel_type,
            activation: activate.activation,
            prior_host_local: activate.prior_host_local.map(|local| {
                crate::domain::effects::ProviderPanelHostLocal {
                    focus_target: local.focus_target,
                    scroll_offset: local.scroll_offset,
                    selected_id: local.selected_id,
                    form_draft: local.form_draft,
                }
            }),
            panel_generation: activate.generation,
        });
        let semantic_key = SemanticKey::new(EffectFamily::Provider, &subject);
        self.register_pending_effect(owner, semantic_key, effect, RetryPolicy::Never)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    fn stage_panel_deactivate(
        &mut self,
        deactivate: crate::state::provider_panels::DeactivateEffect,
    ) -> Result<(), String> {
        use crate::domain::effects::{
            Effect, EffectFamily, ProviderEffect, ProviderPanelDeactivateReason, RetryPolicy,
            SemanticKey,
        };
        use crate::runtime::provider::protocol::DeactivateReason;

        let subject = format!("deactivate-panel-{}", deactivate.panel_instance.as_u64());
        let owner = deactivate.owner.clone();
        let reason = match deactivate.reason {
            DeactivateReason::Suspend => ProviderPanelDeactivateReason::Suspend,
            DeactivateReason::Dispose => ProviderPanelDeactivateReason::Dispose,
            DeactivateReason::Replace => ProviderPanelDeactivateReason::Replace,
        };
        let effect = Effect::Provider(ProviderEffect::DeactivatePanel {
            owner: deactivate.owner,
            panel_instance_id: deactivate.panel_instance.as_u64(),
            panel_generation: deactivate.generation,
            reason,
        });
        let semantic_key = SemanticKey::new(EffectFamily::Provider, &subject);
        self.register_pending_effect(owner, semantic_key, effect, RetryPolicy::Never)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

fn semantic_relationship_command(
    schemas: &crate::workbench::ResourceSchemaRegistry,
    current: &super::navigation::ScreenInstance,
    panel_instance_id: crate::state::provider_panels::PanelInstanceId,
    panel_id: &crate::workbench::PanelId,
    port: &crate::workbench::PortDescriptor,
    semantic_key: &str,
) -> Result<super::RelationshipCommand, String> {
    let type_id =
        crate::domain::Id::parse(port.type_id.name()).map_err(|error| error.to_string())?;
    let value = schemas
        .project_semantic_key(
            &port.owner_id,
            &type_id,
            port.type_id.version(),
            semantic_key,
        )
        .map_err(|error| error.to_string())?;
    Ok(super::RelationshipCommand {
        open_screen_id: current.id,
        panel_instance_id,
        generation: current.generation,
        owner_id: port.owner_id.clone(),
        intent: crate::workbench::SourceIntent::Publish {
            port: crate::workbench::PortRef {
                panel: *panel_id,
                port: port.id,
            },
            value: crate::workbench::PortValue::Typed(value),
        },
    })
}

fn protocol_body_kind(
    kind: crate::domain::plugin::ModelKind,
) -> crate::runtime::provider::protocol::BodyKind {
    use crate::domain::plugin::ModelKind;
    use crate::runtime::provider::protocol::BodyKind;

    match kind {
        ModelKind::List => BodyKind::List,
        ModelKind::Tree => BodyKind::Tree,
        ModelKind::Detail => BodyKind::Detail,
        ModelKind::StructuredDiff => BodyKind::StructuredDiff,
        ModelKind::Form => BodyKind::Form,
        ModelKind::Status => BodyKind::Status,
        ModelKind::Progress => BodyKind::Progress,
        ModelKind::Empty => BodyKind::Empty,
        ModelKind::Error => BodyKind::Error,
    }
}

const fn panel_event_kind(
    kind: crate::domain::plugin::EventKind,
) -> crate::state::provider_panels::EventKind {
    use crate::domain::plugin::EventKind as Manifest;
    use crate::state::provider_panels::EventKind as Panel;

    match kind {
        Manifest::Selected => Panel::Selected,
        Manifest::Activated => Panel::Activated,
        Manifest::Action => Panel::Action,
        Manifest::FieldChanged => Panel::FieldChanged,
        Manifest::Submit => Panel::Submit,
        Manifest::PageRequested => Panel::PageRequested,
        Manifest::Retry => Panel::Retry,
        Manifest::Cancel => Panel::Cancel,
        Manifest::LinkSelected => Panel::LinkSelected,
        Manifest::ExpansionChanged => Panel::ExpansionChanged,
    }
}
fn selected_provider_semantic_key(
    body: &crate::runtime::provider::protocol::PanelBody,
    selected: &crate::domain::Id,
) -> Option<String> {
    use crate::runtime::provider::protocol::PanelBody;

    match body {
        PanelBody::List(list) => list
            .items
            .iter()
            .find(|item| item.id == *selected)
            .map(|item| item.id.as_str().to_owned()),
        PanelBody::Tree(tree) => tree
            .nodes
            .iter()
            .find(|node| node.id == *selected)
            .map(|node| node.semantic_key.as_str().to_owned()),
        PanelBody::StructuredDiff(diff) => diff
            .files
            .iter()
            .find(|file| file.id == *selected)
            .map(|file| file.id.as_str().to_owned()),
        PanelBody::Detail(_)
        | PanelBody::Form(_)
        | PanelBody::Status(_)
        | PanelBody::Progress(_)
        | PanelBody::Empty(_)
        | PanelBody::Error(_) => None,
    }
}

fn domain_panel_event(
    event: crate::runtime::provider::protocol::PanelEvent,
) -> crate::domain::effects::ProviderPanelEvent {
    use crate::domain::effects::ProviderPanelEvent as Domain;
    use crate::runtime::provider::protocol::PanelEvent as Wire;

    match event {
        Wire::Selected { id } => Domain::Selected { id },
        Wire::Activated { id } => Domain::Activated { id },
        Wire::Action { id, arguments } => Domain::Action { id, arguments },
        Wire::FieldChanged { field_id, value } => Domain::FieldChanged { field_id, value },
        Wire::Submit { values } => Domain::Submit { values },
        Wire::PageRequested { token } => Domain::PageRequested { token },
        Wire::Retry => Domain::Retry,
        Wire::Cancel => Domain::Cancel,
        Wire::LinkSelected { link_id } => Domain::LinkSelected { link_id },
        Wire::ExpansionChanged { id, expanded } => Domain::ExpansionChanged { id, expanded },
    }
}

fn activation_typed_map(
    values: &crate::workbench::ActivationValues,
) -> Result<crate::domain::TypedMap, String> {
    use crate::domain::TypedValue;
    use crate::workbench::ActivationValue;

    let mut typed = crate::domain::TypedMap::new();
    for (id, value) in values.iter() {
        let value = match value {
            ActivationValue::Boolean(value) | ActivationValue::OptionalBoolean(Some(value)) => {
                TypedValue::Bool(*value)
            }
            ActivationValue::OptionalBoolean(None) => continue,
            ActivationValue::Text(value) | ActivationValue::Enumerated(value) => {
                TypedValue::String(value.clone())
            }
            ActivationValue::Integer(value) => TypedValue::Integer(*value),
            ActivationValue::Path(value) => {
                let Some(value) = value.to_str() else {
                    return Err("NAV-E001: activation path is not UTF-8".to_owned());
                };
                TypedValue::String(value.to_owned())
            }
            ActivationValue::TextList(values) => {
                TypedValue::List(values.iter().cloned().map(TypedValue::String).collect())
            }
        };
        typed.insert(id.clone(), value);
    }
    Ok(typed)
}
