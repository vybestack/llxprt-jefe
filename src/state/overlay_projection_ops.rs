use super::types::{AppState, ConfirmFocus};
use crate::state::provider_view::{ProviderViewInput, project_provider_view_with_confirmation};

impl AppState {
    /// First visible row of this instance's active Help overlay.
    #[must_use]
    pub const fn help_scroll_offset(&self) -> usize {
        self.nav.current().overlays().help_viewport()
    }

    /// Replace this instance's Help viewport when declared Help is active.
    pub fn set_help_scroll_offset(&mut self, viewport: usize) -> bool {
        self.nav
            .current_mut()
            .overlays_mut()
            .set_help_viewport(viewport)
    }

    /// Active declared host overlay kind for this exact screen instance.
    #[must_use]
    pub fn active_overlay_kind(&self) -> Option<crate::workbench::OverlayKind> {
        self.nav
            .current()
            .overlays()
            .active()
            .map(super::screen_overlays::ActiveOverlay::kind)
    }

    /// Whether the exact instance's visible overlay owns mouse input before its panels.
    #[must_use]
    pub fn blocking_overlay_owns_mouse(&self) -> bool {
        matches!(
            self.active_overlay_kind(),
            Some(crate::workbench::OverlayKind::Help | crate::workbench::OverlayKind::Confirmation)
        ) || self.provider_surface_action().is_some()
    }

    pub(crate) fn push_search_char(&mut self, value: char) -> bool {
        let mut query = self.search_query().unwrap_or_default().to_owned();
        query.push(value);
        let Some(query) = crate::overlay_controls::edited_search_query(self, query, 60) else {
            return false;
        };
        let cursor = query.len();
        self.nav
            .current_mut()
            .overlays_mut()
            .replace_search(query, cursor)
    }

    pub(crate) fn pop_search_char(&mut self) -> bool {
        let mut query = self.search_query().unwrap_or_default().to_owned();
        if query.pop().is_none() {
            return false;
        }
        let Some(query) = crate::overlay_controls::edited_search_query(self, query, 60) else {
            return false;
        };
        let cursor = query.len();
        self.nav
            .current_mut()
            .overlays_mut()
            .replace_search(query, cursor)
    }

    /// Whether the active Search Form accepted its typed Submit action.
    #[must_use]
    pub fn search_control_accepts_submit(&self) -> bool {
        crate::overlay_controls::search_submission_accepted(self, 60)
    }

    /// Interpret Help scrolling and bounds through one shared Detail projection.
    #[must_use]
    pub fn help_control_scroll(
        &self,
        action: crate::host_controls::ControlAction,
        render_cols: u16,
        render_rows: u16,
    ) -> Option<(i8, usize)> {
        let layout = crate::overlay_controls::HostOverlayLayout::help(render_cols, render_rows);
        let projection = crate::overlay_controls::project_help(self, layout.content_width);
        match crate::overlay_controls::overlay_intent(&projection, action) {
            crate::host_controls::ControlIntent::Scroll(delta) => Some((
                delta,
                projection.rows.len().saturating_sub(layout.viewport_rows),
            )),
            crate::host_controls::ControlIntent::Event(_)
            | crate::host_controls::ControlIntent::PagePrevious
            | crate::host_controls::ControlIntent::None => None,
        }
    }

    /// Whether the current declared Confirmation overlay owns this exact provider request.
    #[must_use]
    pub fn owns_provider_confirmation(
        &self,
        confirmation: crate::state::provider_requests::PendingConfirmationView<'_>,
    ) -> bool {
        self.active_overlay_kind() == Some(crate::workbench::OverlayKind::Confirmation)
            && self.nav.current().overlays().provider_confirmation()
                == Some(&confirmation.identity())
    }

    /// Exact pending token presented by the current screen instance's Confirmation overlay.
    #[must_use]
    pub fn current_provider_confirmation(
        &self,
    ) -> Option<crate::state::provider_requests::PendingConfirmationView<'_>> {
        let identity = self.nav.current().overlays().provider_confirmation()?;
        self.provider_requests.pending_confirmation_view(identity)
    }

    /// Interpret provider-confirmation focus and activation through the shared Form control.
    #[must_use]
    pub fn provider_confirmation_focus_for(
        &self,
        action: crate::host_controls::ControlAction,
        width: usize,
    ) -> Option<ConfirmFocus> {
        crate::overlay_controls::provider_confirmation_focus(self, action, width)
    }

    fn unavailable_provider_surface_projection(
        &self,
        viewport_rows: usize,
    ) -> Option<crate::state::provider_view::ProviderViewProjection> {
        let current = self.nav.current();
        let surface_action = current.provider_surface_action()?;
        let action = self
            .action_registry()
            .provider_actions()
            .find(|action| action.id == *surface_action)?;
        let context_instance = current.id.to_string();
        Some(
            self.attach_provider_confirmation_state(project_provider_view_with_confirmation(
                &ProviderViewInput {
                    requests: &self.provider_requests,
                    context_screen: current.screen.as_str(),
                    context_instance: &context_instance,
                    availability: self.action_availability(&action.id),
                    focused: false,
                    confirm: None,
                    viewport_rows,
                    focused_index: None,
                    action_label: Some(&action.label),
                },
                None,
            )),
        )
    }

    /// Project the provider surface from this exact committed request and action identity.
    #[must_use]
    pub fn provider_surface_projection(
        &self,
        viewport_rows: usize,
    ) -> Option<crate::state::provider_view::ProviderViewProjection> {
        let registry = self.action_registry();
        let current = self.nav.current();
        if current.overlays().generic_confirmation().is_some() {
            return None;
        }
        if current.provider_surface_action().is_some() {
            return self.unavailable_provider_surface_projection(viewport_rows);
        }
        let context_screen = current.screen.as_str();
        let context_instance = current.id.to_string();
        let requests = self.provider_requests.requests();
        let request = requests.iter().rev().find(|request| {
            request.context_screen().as_str() == context_screen
                && request.context_instance().as_str() == context_instance
        })?;
        let action = registry
            .provider_actions()
            .find(|action| action.id.as_str() == request.key().action_id.as_str())?;
        let confirmation = self.current_provider_confirmation();
        let confirm = confirmation.and_then(|_| current.overlays().confirmation_focus());
        let focused_index = requests
            .iter()
            .filter(|request| {
                request.context_screen().as_str() == context_screen
                    && request.context_instance().as_str() == context_instance
            })
            .count()
            .checked_sub(1);
        Some(
            self.attach_provider_confirmation_state(project_provider_view_with_confirmation(
                &ProviderViewInput {
                    requests: &self.provider_requests,
                    context_screen,
                    context_instance: &context_instance,
                    availability: self.action_availability(&action.id),
                    focused: !request.is_terminal(),
                    confirm,
                    viewport_rows,
                    focused_index,
                    action_label: Some(&action.label),
                },
                confirmation,
            )),
        )
    }

    fn attach_provider_confirmation_state(
        &self,
        mut projection: crate::state::provider_view::ProviderViewProjection,
    ) -> crate::state::provider_view::ProviderViewProjection {
        if let crate::state::provider_view::ProviderViewMode::Confirmation {
            continuation_values,
            focused_field,
            ..
        } = &mut projection.mode
        {
            *continuation_values = self
                .nav
                .current()
                .overlays()
                .confirmation_values()
                .cloned()
                .unwrap_or_default();
            *focused_field = self
                .nav
                .current()
                .overlays()
                .confirmation_focused_field()
                .cloned();
        }
        projection
    }

    /// Interpret one continuation field edit through the shared Form control.
    #[must_use]
    pub fn provider_confirmation_field_edit(
        &self,
        field_id: crate::domain::Id,
        value: crate::domain::TypedValue,
    ) -> Option<(crate::domain::Id, crate::domain::TypedValue)> {
        let pending = self.current_provider_confirmation()?;
        let overlays = self.nav.current().overlays();
        let focus = overlays.confirmation_focus()?;
        let empty_values = crate::domain::TypedMap::new();
        let projection = crate::overlay_controls::project_provider_confirmation(
            crate::overlay_controls::ProviderConfirmationContent {
                title: pending.title(),
                body: pending.body(),
                confirm_label: pending.confirm_label(),
                focus,
                continuation_schema: pending.continuation_schema(),
                continuation_values: overlays.confirmation_values().unwrap_or(&empty_values),
                focused_field: overlays.confirmation_focused_field(),
            },
            60,
        );
        match crate::overlay_controls::overlay_intent(
            &projection,
            crate::host_controls::ControlAction::EditField { field_id, value },
        ) {
            crate::host_controls::ControlIntent::Event(
                crate::runtime::provider::protocol::PanelEvent::FieldChanged { field_id, value },
            ) => Some((field_id, value)),
            crate::host_controls::ControlIntent::Event(_)
            | crate::host_controls::ControlIntent::Scroll(_)
            | crate::host_controls::ControlIntent::PagePrevious
            | crate::host_controls::ControlIntent::None => None,
        }
    }

    /// Retain one syntactically valid but constraint-incomplete continuation draft locally.
    pub fn set_provider_confirmation_draft(
        &mut self,
        field_id: crate::domain::Id,
        value: crate::domain::TypedValue,
    ) -> bool {
        let Some(field) = self
            .current_provider_confirmation()
            .and_then(|confirmation| {
                confirmation
                    .continuation_schema()
                    .iter()
                    .find(|field| field.id() == &field_id)
            })
        else {
            return false;
        };
        if !crate::form_value_edit::form_value_has_editable_syntax(field, &value)
            || crate::form_value_edit::form_value_is_complete(field, &value)
        {
            return false;
        }
        let Some((field_id, value)) = self.provider_confirmation_field_edit(field_id, value) else {
            return false;
        };
        self.nav
            .current_mut()
            .overlays_mut()
            .set_confirmation_value(&field_id, value)
    }

    /// Whether the shared provider control accepts Retry for the current surface.
    #[must_use]
    pub fn provider_retry_control_accepts(&self) -> bool {
        self.provider_surface_projection(24)
            .is_some_and(|projection| {
                crate::overlay_controls::provider_retry_accepted(&projection, 60)
            })
    }

    /// Whether the shared provider control accepts cancellation for the current surface.
    #[must_use]
    pub fn provider_cancel_control_accepts(&self) -> bool {
        self.provider_surface_projection(24)
            .is_some_and(|projection| {
                crate::overlay_controls::provider_cancel_accepted(&projection, 60)
            })
    }
}
