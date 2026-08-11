impl ProviderPanelState {
    /// Construct empty state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            panels: Vec::new(),
            next_panel_instance_id: 1,
        }
    }

    /// Number of tracked panels.
    #[must_use]
    pub fn len(&self) -> usize {
        self.panels.len()
    }

    /// Whether no panel is tracked.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.panels.is_empty()
    }

    /// The lifecycle of a panel, if known.
    #[must_use]
    pub fn lifecycle(&self, panel: PanelInstanceId) -> Option<PanelLifecycle> {
        self.index(panel).map(|i| self.panels[i].lifecycle)
    }

    /// The activation generation of a panel, if known.
    #[must_use]
    pub fn generation(&self, panel: PanelInstanceId) -> Option<u64> {
        self.index(panel).map(|i| self.panels[i].generation)
    }

    /// The accepted revision of a panel, if a model is retained.
    #[must_use]
    pub fn accepted_revision(&self, panel: PanelInstanceId) -> Option<u64> {
        self.index(panel)
            .and_then(|i| self.panels[i].accepted.as_ref().map(|m| m.revision))
    }

    /// Whether the retained model is stale, if a model is retained.
    #[must_use]
    pub fn is_stale(&self, panel: PanelInstanceId) -> Option<bool> {
        self.index(panel)
            .and_then(|i| self.panels[i].accepted.as_ref().map(|m| m.stale))
    }

    /// The accepted snapshot, if a model is retained.
    #[must_use]
    pub fn accepted_snapshot(&self, panel: PanelInstanceId) -> Option<&PanelSnapshot> {
        self.index(panel)
            .and_then(|i| self.panels[i].accepted.as_ref().map(|m| &m.snapshot))
    }

    /// Whether the retained complete model is visibly stale.
    #[must_use]
    pub fn accepted_model_is_stale(&self, panel: PanelInstanceId) -> bool {
        self.index(panel)
            .and_then(|index| self.panels[index].accepted.as_ref())
            .is_some_and(|model| model.stale)
    }

    /// Find the provider panel instance backing a descriptor panel on a screen instance.
    #[must_use]
    pub fn panel_for_screen(
        &self,
        screen_instance_id: u64,
        panel_id: &PanelId,
    ) -> Option<PanelInstanceId> {
        self.panels
            .iter()
            .find(|panel| {
                panel.screen_instance_id == screen_instance_id && &panel.panel_id == panel_id
            })
            .map(|panel| panel.id)
    }

    /// All panel instances owned by one navigation screen instance.
    #[must_use]
    pub fn panels_for_screen(&self, screen_instance_id: u64) -> Vec<PanelInstanceId> {
        self.panels
            .iter()
            .filter(|panel| panel.screen_instance_id == screen_instance_id)
            .map(|panel| panel.id)
            .collect()
    }

    /// The retained host-local state, if any.
    #[must_use]
    pub fn host_local(&self, panel: PanelInstanceId) -> Option<&HostLocal> {
        self.index(panel)
            .and_then(|i| self.panels[i].host_local.as_ref())
    }

    /// Mark one live panel failed after its provider transport becomes unavailable.
    ///
    /// A complete accepted model remains available and is marked stale. Disposed
    /// and suspended panels reject the transition because neither is subscribed.
    ///
    /// # Errors
    ///
    /// Returns [`PanelError::UnknownPanel`] or [`PanelError::InvalidLifecycle`].
    pub fn fail_runtime(&mut self, panel: PanelInstanceId) -> Result<(), PanelError> {
        let index = self.require(panel)?;
        if !matches!(
            self.panels[index].lifecycle,
            PanelLifecycle::Declared
                | PanelLifecycle::Activating
                | PanelLifecycle::Active
                | PanelLifecycle::Failed
        ) {
            return Err(PanelError::InvalidLifecycle);
        }
        Self::mark_failed(&mut self.panels[index]);
        Ok(())
    }

    /// Mark every subscribed panel for one provider owner failed.
    pub fn fail_runtime_owner(&mut self, owner: &Id) -> usize {
        let mut failed = 0;
        for panel in &mut self.panels {
            if &panel.owner == owner
                && matches!(
                    panel.lifecycle,
                    PanelLifecycle::Declared
                        | PanelLifecycle::Activating
                        | PanelLifecycle::Active
                        | PanelLifecycle::Failed
                )
            {
                Self::mark_failed(panel);
                failed += 1;
            }
        }
        failed
    }

    /// Declare a panel, allocating a fresh monotonic instance identity.
    ///
    /// # Errors
    ///
    /// Returns [`PanelError::InstanceExhausted`] when the u64 counter exhausts.
    pub fn declare(&mut self, command: DeclareInput) -> Result<DeclareOutcome, PanelError> {
        let instance = self.allocate_instance()?;
        self.panels.push(PanelRecord::new(instance, command));
        Ok(DeclareOutcome { instance })
    }

    /// Activate a declared panel, staging an `activate-panel` effect.
    ///
    /// # Errors
    ///
    /// Returns [`PanelError::UnknownPanel`] or [`PanelError::InvalidLifecycle`].
    pub fn activate(&mut self, panel: PanelInstanceId) -> Result<ActivateOutcome, PanelError> {
        let index = self.require(panel)?;
        if self.panels[index].lifecycle != PanelLifecycle::Declared {
            return Err(PanelError::InvalidLifecycle);
        }
        let effect = self.start_activation(index, None)?;
        Ok(ActivateOutcome { effect })
    }

    /// Retry an active or failed panel, staging a fresh `activate-panel`.
    ///
    /// # Errors
    ///
    /// Returns [`PanelError::UnknownPanel`] or [`PanelError::InvalidLifecycle`].
    pub fn retry(&mut self, panel: PanelInstanceId) -> Result<ActivateOutcome, PanelError> {
        let index = self.require(panel)?;
        if !matches!(
            self.panels[index].lifecycle,
            PanelLifecycle::Active | PanelLifecycle::Failed
        ) {
            return Err(PanelError::InvalidLifecycle);
        }
        let effect = self.start_activation(index, None)?;
        Ok(ActivateOutcome { effect })
    }

    /// Resume a suspended panel with a fresh generation and prior host-local.
    ///
    /// # Errors
    ///
    /// Returns [`PanelError::UnknownPanel`] or [`PanelError::InvalidLifecycle`].
    pub fn resume(&mut self, panel: PanelInstanceId) -> Result<ActivateOutcome, PanelError> {
        let index = self.require(panel)?;
        if self.panels[index].lifecycle != PanelLifecycle::Suspended {
            return Err(PanelError::InvalidLifecycle);
        }
        let prior = self.panels[index].host_local.clone();
        let effect = self.start_activation(index, prior)?;
        Ok(ActivateOutcome { effect })
    }

    /// Accept a provider snapshot, atomically replacing the accepted model.
    ///
    /// Correlation and rate failures apply no partial model. A size failure
    /// marks the panel `Failed` and retains the prior model as stale.
    ///
    /// # Errors
    ///
    /// Returns a [`PanelError`] variant for any rejected candidate.
    pub fn accept_snapshot(
        &mut self,
        command: AcceptSnapshot,
    ) -> Result<AcceptOutcome, PanelError> {
        let index = self
            .index(PanelInstanceId(command.snapshot.panel_instance_id))
            .ok_or(PanelError::UnknownPanel)?;
        if !self.panels[index].lifecycle.receives_snapshot() {
            return Err(self.disposed_or_invalid(index));
        }
        self.validate_snapshot_correlation(index, &command)?;
        self.panels[index].bucket.consume(command.elapsed_ms)?;
        let revision = self.apply_snapshot(index, command)?;
        Ok(AcceptOutcome { revision })
    }

    /// Suspend a panel, staging a `deactivate-panel` (reason Suspend).
    ///
    /// Drops the model and retains host-local state.
    ///
    /// # Errors
    ///
    /// Returns [`PanelError::UnknownPanel`] or [`PanelError::InvalidLifecycle`].
    pub fn suspend(&mut self, panel: PanelInstanceId) -> Result<DeactivateEffect, PanelError> {
        let index = self.require(panel)?;
        if !self.panels[index].lifecycle.can_suspend() {
            return Err(PanelError::InvalidLifecycle);
        }
        let record = &mut self.panels[index];
        let generation = record.generation;
        let id = record.id;
        record.lifecycle = PanelLifecycle::Suspended;
        record.accepted = None;
        Ok(DeactivateEffect {
            owner: record.owner.clone(),
            panel_instance: id,
            generation,
            reason: DeactivateReason::Suspend,
        })
    }

    /// Dispose a panel permanently, staging `deactivate-panel` (reason Dispose).
    ///
    /// # Errors
    ///
    /// Returns [`PanelError::UnknownPanel`] or [`PanelError::Disposed`].
    pub fn dispose(&mut self, panel: PanelInstanceId) -> Result<DeactivateOutcome, PanelError> {
        self.dispose_with(panel, DeactivateReason::Dispose)
    }

    /// Replace a panel, staging `deactivate-panel` (reason Replace).
    ///
    /// # Errors
    ///
    /// Returns [`PanelError::UnknownPanel`] or [`PanelError::Disposed`].
    pub fn replace(&mut self, panel: PanelInstanceId) -> Result<DeactivateOutcome, PanelError> {
        self.dispose_with(panel, DeactivateReason::Replace)
    }

    /// Submit a semantic event, returning a typed effect or zero effect.
    ///
    /// Undeclared, invalid, disabled, or stale events emit [`EventOutcome::None`]
    /// with zero mutation. A `Retry` event from `Failed` emits a fresh
    /// [`EventOutcome::Activate`]; an active error model's retry is a normal
    /// provider event validated against that model.
    ///
    /// # Errors
    ///
    /// Returns [`PanelError::UnknownPanel`] when no panel matches.
    fn submit_event(&mut self, command: SubmitEvent) -> Result<EventOutcome, PanelError> {
        let index = self.require(command.panel)?;
        if self.panels[index].lifecycle == PanelLifecycle::Disposed {
            return Ok(EventOutcome::None);
        }
        if matches!(command.event, PanelEvent::Retry)
            && self.panels[index].lifecycle == PanelLifecycle::Failed
        {
            let Some(declaration) = matching_declaration(command.allowed_events, &command.event)
            else {
                return Ok(EventOutcome::None);
            };
            return self.retry_event(index, &command, declaration);
        }
        if self.panels[index].lifecycle != PanelLifecycle::Active {
            return Ok(EventOutcome::None);
        }
        let Some(declaration) = matching_declaration(command.allowed_events, &command.event) else {
            return Ok(EventOutcome::None);
        };
        let Some(revision) = self.validate_event(index, &command, declaration) else {
            return Ok(EventOutcome::None);
        };
        let record = &self.panels[index];
        Ok(EventOutcome::Event(PanelEventEffect {
            owner: record.owner.clone(),
            panel_instance: record.id,
            generation: record.generation,
            revision,
            event: command.event.clone(),
        }))
    }

    /// Validate an event using correlation retained by the live panel record.
    ///
    /// # Errors
    ///
    /// Returns a typed error when the panel instance is unknown.
    pub fn submit_live_event(
        &mut self,
        panel: PanelInstanceId,
        event: PanelEvent,
    ) -> Result<EventOutcome, PanelError> {
        let Some(index) = self.index(panel) else {
            return Err(PanelError::UnknownPanel);
        };
        let record = &self.panels[index];
        let owner = record.owner.clone();
        let process_generation = record.process_generation;
        let panel_generation = record.generation;
        let revision = record.accepted.as_ref().map_or_else(
            || record.expected_revision.saturating_sub(1),
            |model| model.revision,
        );
        let allowed_events = record.allowed_events.clone();
        self.submit_event(SubmitEvent {
            panel,
            owner: &owner,
            received_process_generation: process_generation,
            generation: panel_generation,
            revision,
            event,
            allowed_events: &allowed_events,
        })
    }

    /// Update host-local state on a live or suspended panel (atomic, bounded).
    ///
    /// # Errors
    ///
    /// Returns [`PanelError::InvalidLifecycle`] or [`PanelError::HostLocalTooLarge`].
    pub fn update_host_local(
        &mut self,
        panel: PanelInstanceId,
        host: HostLocal,
    ) -> Result<(), PanelError> {
        let index = self.require(panel)?;
        if !self.panels[index].lifecycle.is_live_or_suspended() {
            return Err(PanelError::InvalidLifecycle);
        }

        if host_local_canonical_bytes(&host) > HOST_LOCAL_MAX_BYTES {
            return Err(PanelError::HostLocalTooLarge);
        }
        self.panels[index].host_local = Some(host);
        Ok(())
    }

    fn index(&self, panel: PanelInstanceId) -> Option<usize> {
        self.panels.iter().position(|record| record.id == panel)
    }

    fn require(&self, panel: PanelInstanceId) -> Result<usize, PanelError> {
        self.index(panel).ok_or(PanelError::UnknownPanel)
    }

    fn allocate_instance(&mut self) -> Result<PanelInstanceId, PanelError> {
        let instance = PanelInstanceId(self.next_panel_instance_id);
        self.next_panel_instance_id = self
            .next_panel_instance_id
            .checked_add(1)
            .ok_or(PanelError::InstanceExhausted)?;
        Ok(instance)
    }

    fn disposed_or_invalid(&self, index: usize) -> PanelError {
        if self.panels[index].lifecycle == PanelLifecycle::Disposed {
            PanelError::Disposed
        } else {
            PanelError::InvalidLifecycle
        }
    }

    fn start_activation(
        &mut self,
        index: usize,
        prior_host_local: Option<HostLocal>,
    ) -> Result<ActivateEffect, PanelError> {
        let record = &mut self.panels[index];
        record.generation = record
            .generation
            .checked_add(1)
            .ok_or(PanelError::GenerationExhausted)?;
        record.expected_revision = 1;
        record.accepted = None;
        record.lifecycle = PanelLifecycle::Activating;
        record.bucket = TokenBucket::fresh();
        Ok(ActivateEffect {
            owner: record.owner.clone(),
            panel_instance: record.id,
            screen_instance: record.screen_instance_id,
            panel_type: record.panel_type.clone(),
            activation: record.activation.clone(),
            prior_host_local,
            generation: record.generation,
        })
    }

    fn dispose_with(
        &mut self,
        panel: PanelInstanceId,
        reason: DeactivateReason,
    ) -> Result<DeactivateOutcome, PanelError> {
        let index = self.require(panel)?;
        let record = &mut self.panels[index];
        if record.lifecycle == PanelLifecycle::Disposed {
            return Err(PanelError::Disposed);
        }
        let sends = record.lifecycle.dispose_sends_effect();
        let owner = record.owner.clone();
        let id = record.id;
        let generation = record.generation;
        record.lifecycle = PanelLifecycle::Disposed;
        record.accepted = None;
        record.host_local = None;
        if sends {
            Ok(DeactivateOutcome::Sent(DeactivateEffect {
                owner,
                panel_instance: id,
                generation,
                reason,
            }))
        } else {
            Ok(DeactivateOutcome::None)
        }
    }

    fn mark_failed(record: &mut PanelRecord) {
        record.lifecycle = PanelLifecycle::Failed;
        if let Some(model) = &mut record.accepted {
            model.stale = true;
        }
    }

    fn retry_event(
        &mut self,
        index: usize,
        command: &SubmitEvent,
        declaration: &EventDeclaration,
    ) -> Result<EventOutcome, PanelError> {
        if self.panels[index].lifecycle != PanelLifecycle::Failed {
            return Ok(EventOutcome::None);
        }
        if !declaration.arguments.is_empty()
            || !base_event_correlation_ok(&self.panels[index], command)
            || self.panels[index]
                .accepted
                .as_ref()
                .is_some_and(|model| model.revision != command.revision)
        {
            return Ok(EventOutcome::None);
        }
        let effect = self.start_activation(index, None)?;
        Ok(EventOutcome::Activate(effect))
    }

    fn validate_snapshot_correlation(
        &self,
        index: usize,
        command: &AcceptSnapshot,
    ) -> Result<(), PanelError> {
        let record = &self.panels[index];
        let snapshot = command.snapshot;
        if record.owner != *command.owner {
            return Err(PanelError::OwnerMismatch);
        }
        if command.received_process_generation != record.process_generation {
            return Err(PanelError::ProcessGenerationMismatch);
        }
        if snapshot.generation != record.generation {
            return Err(PanelError::GenerationMismatch);
        }
        if snapshot.revision != record.expected_revision {
            return Err(PanelError::RevisionMismatch);
        }
        if snapshot.model_schema != MODEL_SCHEMA {
            return Err(PanelError::ModelMismatch);
        }
        Ok(())
    }

    fn apply_snapshot(&mut self, index: usize, command: AcceptSnapshot) -> Result<u64, PanelError> {
        if command.payload_byte_count > SNAPSHOT_MAX_BYTES
            || !self.panels[index]
                .allowed_model_kinds
                .contains(&command.snapshot.kind)
            || !affordances_valid(&self.panels[index].action_authority, command.snapshot)
        {
            self.panels[index].lifecycle = PanelLifecycle::Failed;
            if let Some(model) = &mut self.panels[index].accepted {
                model.stale = true;
            }
            return Err(PanelError::SnapshotInvalid);
        }
        let revision = command.snapshot.revision;
        let Some(expected_revision) = revision.checked_add(1) else {
            let record = &mut self.panels[index];
            record.lifecycle = PanelLifecycle::Failed;
            if let Some(model) = &mut record.accepted {
                model.stale = true;
            }
            return Err(PanelError::SnapshotInvalid);
        };
        let record = &mut self.panels[index];
        record.accepted = Some(AcceptedModel {
            snapshot: command.snapshot.clone(),
            revision,
            stale: false,
        });
        record.expected_revision = expected_revision;
        record.lifecycle = PanelLifecycle::Active;
        Ok(revision)
    }

    fn validate_event(
        &self,
        index: usize,
        command: &SubmitEvent,
        declaration: &EventDeclaration,
    ) -> Option<u64> {
        let record = &self.panels[index];
        if !event_correlation_ok(record, command) {
            return None;
        }
        let model = record.accepted.as_ref()?;
        if !validate_event_against_snapshot(&model.snapshot, &command.event, declaration) {
            return None;
        }
        Some(model.revision)
    }
}

fn event_correlation_ok(record: &PanelRecord, command: &SubmitEvent) -> bool {
    base_event_correlation_ok(record, command)
        && record
            .accepted
            .as_ref()
            .is_some_and(|model| model.revision == command.revision)
}

fn base_event_correlation_ok(record: &PanelRecord, command: &SubmitEvent) -> bool {
    let owner_matches = record.owner == *command.owner;
    let process_generation_matches =
        command.received_process_generation == record.process_generation;
    let panel_generation_matches = command.generation == record.generation;
    owner_matches && process_generation_matches && panel_generation_matches
}

// ---------------------------------------------------------------------------
// Canonical host-local byte size is provided by the private `canonical`
// child module; event validation lives in the private `event_validation`
// child module.
// ---------------------------------------------------------------------------

/// Whether every affordance in a snapshot is authorized by the owner's declared
/// action ids.
///
/// An affordance whose `action_id` is not declared by the owner is rejected. A
/// disabled affordance must carry a nonempty `unavailable_reason`. The check is
/// atomic: if any affordance fails, the whole snapshot is invalid.
fn affordances_valid(authority: &[ActionId], snapshot: &PanelSnapshot) -> bool {
    snapshot.action_affordances.iter().all(|affordance| {
        authority.contains(&affordance.action_id)
            && (affordance.enabled
                || affordance
                    .unavailable_reason
                    .as_ref()
                    .is_some_and(|reason| !reason.trim().is_empty()))
    })
}

#[cfg(test)]
#[path = "provider_panels_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "provider_panels_event_tests.rs"]
mod event_tests;
