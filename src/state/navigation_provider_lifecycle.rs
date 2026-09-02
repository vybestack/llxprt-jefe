//! Provider-panel lifecycle that navigation commits.
//!
//! Pushing, replacing, and restoring a screen suspends, disposes, resumes, or
//! activates the provider panels the navigation reducer decided for it. The
//! effect-staging helpers those transitions rely on stay in
//! [`super::navigation_ops`] with the rest of the staging surface; this split
//! keeps that reducer file inside the handler source-size gate (issue #706).

use super::AppState;
use super::navigation::NavOutcome;

impl AppState {
    /// Reconcile provider-panel lifecycles with one committed navigation
    /// outcome, rolling the whole transition back when any panel refuses.
    pub(super) fn update_provider_panel_lifecycle(
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
