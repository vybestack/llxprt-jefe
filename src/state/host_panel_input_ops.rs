//! Exact-instance input reduction for host-owned shared-runtime panels.

use crate::domain::{Id, InternalId};
use crate::host_controls::{ControlAction, ControlIntent, control_intent_body};
use crate::host_panel_models::project_host_panel;
use crate::messages::AppMessage;
use crate::runtime::provider::protocol::{PanelBody, PanelEvent};
use crate::workbench::{HostPanelCapability, HostPanelModelSource};

use super::{AppEvent, AppState};

impl AppState {
    /// Whether the active validated declaration owns the sealed Dashboard action context.
    #[must_use]
    pub fn has_dashboard_action_context(&self) -> bool {
        self.published_workbench()
            .screen_registry()
            .get_identity(self.screen())
            .is_some_and(|descriptor| {
                descriptor.has_host_capability(
                    crate::workbench::HostScreenCapability::DashboardActionContext,
                )
            })
    }

    /// Whether the exact focused host-owned control is a reorderable list.
    #[must_use]
    pub fn focused_host_reorder_panel(&self) -> bool {
        let current = self.nav.current();
        self.published_workbench()
            .screen_registry()
            .get_identity(current.screen)
            .and_then(|descriptor| descriptor.panel(&current.panel_focus))
            .and_then(|panel| {
                panel
                    .host_capability
                    .map(crate::workbench::HostPanelCapability::control_kind)
            })
            == Some(crate::host_controls::ControlKind::List)
    }

    /// Whether the active definition owns an agent-list model source.
    #[must_use]
    pub fn has_host_agent_panel(&self) -> bool {
        self.published_workbench()
            .screen_registry()
            .get_identity(self.screen())
            .is_some_and(|descriptor| {
                descriptor.panels.iter().any(|panel| {
                    panel.host_capability.is_some_and(|capability| {
                        capability.model_source() == HostPanelModelSource::AgentList
                    })
                })
            })
    }

    /// Reduce one action through the same closed control factory used by a provider panel.
    ///
    /// Returns `false` when the authenticated host declaration and projected
    /// control disagree or the control has no action for the supplied input.
    pub fn apply_host_panel_action(
        &mut self,
        capability: HostPanelCapability,
        action: ControlAction,
        viewport_cols: usize,
        viewport_rows: usize,
    ) -> bool {
        let model = project_host_panel(self, capability.model_source());
        if crate::host_controls::ControlKind::from(model.body.kind()) != capability.control_kind() {
            return false;
        }
        let intent = control_intent_body(
            &model.body,
            &model.action_affordances,
            model.selected_id.as_ref(),
            None,
            None,
            action,
        );
        match intent {
            ControlIntent::Event(event) => self.apply_host_panel_event(
                capability.model_source(),
                event,
                viewport_cols,
                viewport_rows,
            ),
            ControlIntent::Scroll(delta) => {
                self.scroll_host_panel_kind(capability.model_source(), delta, viewport_rows)
            }
            // PageUp on the grid pages the cards back, the legacy
            // `split.page-up` behavior. Only the card grid pages; every other
            // List control has pages the token protocol owns, so the action is
            // unconsumed there. The reducer bounds the step by the committed
            // frame's display basis and keeps it inert without one (issue
            // #706).
            ControlIntent::PagePrevious => match capability.model_source() {
                HostPanelModelSource::WorkbenchCards => {
                    self.apply_workbench(
                        super::workbench_reducers::WorkbenchNavigation::PreviousPage,
                    );
                    true
                }
                _ => false,
            },
            ControlIntent::None => false,
        }
    }

    /// Move an exact-instance host-owned panel viewport, clamped to its projected model.
    pub fn scroll_host_panel(
        &mut self,
        capability: HostPanelCapability,
        delta: i8,
        viewport_rows: usize,
    ) -> bool {
        self.scroll_host_panel_kind(capability.model_source(), delta, viewport_rows)
    }

    fn scroll_host_panel_kind(
        &mut self,
        kind: HostPanelModelSource,
        delta: i8,
        viewport_rows: usize,
    ) -> bool {
        let model = project_host_panel(self, kind);
        let PanelBody::List(body) = model.body else {
            return false;
        };
        let Ok(maximum) = u32::try_from(body.items.len().saturating_sub(viewport_rows)) else {
            return false;
        };
        let next = model
            .scroll_offset
            .saturating_add_signed(i32::from(delta))
            .min(maximum);
        let offset = match kind {
            HostPanelModelSource::RepositoryList => &mut self.repository_scroll_offset,
            HostPanelModelSource::AgentList => &mut self.agent_scroll_offset,
            HostPanelModelSource::SessionList => &mut self.session_scroll_offset,
            // The STATUS block is four fixed rows: it never scrolls.
            // The card grid pages rather than scrolls; the page index is
            // owned by the workbench state and clamped at render time.
            HostPanelModelSource::WorkbenchStatus
            | HostPanelModelSource::WorkbenchCards
            | HostPanelModelSource::SearchInput
            | HostPanelModelSource::AgentTypeAvailability
            | HostPanelModelSource::AgentPreview => return false,
        };
        let changed = *offset != next;
        *offset = next;
        changed
    }

    fn apply_host_panel_event(
        &mut self,
        kind: HostPanelModelSource,
        event: PanelEvent,
        _viewport_cols: usize,
        viewport_rows: usize,
    ) -> bool {
        match event {
            PanelEvent::Selected { id } => {
                if !self.select_host_panel_item(kind, &id) {
                    return false;
                }
                self.reveal_host_panel_selection(kind, viewport_rows);
                true
            }
            PanelEvent::Activated { id } => {
                if !self.select_host_panel_item(kind, &id) {
                    return false;
                }
                self.reveal_host_panel_selection(kind, viewport_rows);
                let event = match kind {
                    HostPanelModelSource::RepositoryList => self
                        .selected_repository()
                        .map(|repository| AppEvent::OpenEditRepository(repository.id.clone())),
                    HostPanelModelSource::AgentList => self
                        .selected_agent()
                        .map(|agent| AppEvent::OpenEditAgent(agent.id.clone())),
                    HostPanelModelSource::SessionList => self.session_activation_event(),
                    // Enter on a bucket row toggles its filter, the legacy
                    // `split.toggle-status-filter` behavior.
                    HostPanelModelSource::WorkbenchStatus => {
                        self.apply_workbench_status_toggle(self.workbench_filter_cursor_bucket());
                        None
                    }
                    // Enter on a card attaches to its agent, the legacy
                    // `split.activate-selection` behavior.
                    HostPanelModelSource::WorkbenchCards => {
                        self.apply_workbench(
                            super::workbench_reducers::WorkbenchNavigation::Attach,
                        );
                        None
                    }
                    // The availability pane is a read-only startup surface:
                    // it declares `focusable: false`, so no host-panel input
                    // ever reaches it (#734).
                    HostPanelModelSource::SearchInput
                    | HostPanelModelSource::AgentTypeAvailability
                    | HostPanelModelSource::AgentPreview => None,
                };
                if let Some(event) = event {
                    self.reduce_message_body(AppMessage::from(event));
                }
                true
            }
            PanelEvent::Submit { .. } if kind == HostPanelModelSource::SearchInput => {
                self.reduce_message_body(AppMessage::from(AppEvent::OpenSearch));
                self.active_overlay_kind() == Some(crate::workbench::OverlayKind::Search)
            }
            // PageDown on the grid pages the cards, the legacy
            // `split.page-down` behavior. The page count comes from the
            // committed frame's display basis (the same basis the render
            // loop uses), not the caller's viewport rectangle, so the
            // retained page counter never advances past the last real
            // page (issue #706).
            PanelEvent::PageRequested { .. } if kind == HostPanelModelSource::WorkbenchCards => {
                let page_count = self.display_page_count();
                self.apply_workbench_page_next_within(page_count);
                true
            }
            PanelEvent::Action { .. }
            | PanelEvent::FieldChanged { .. }
            | PanelEvent::Submit { .. }
            | PanelEvent::PageRequested { .. }
            | PanelEvent::Retry
            | PanelEvent::Cancel
            | PanelEvent::LinkSelected { .. }
            | PanelEvent::ExpansionChanged { .. } => false,
        }
    }

    /// Resolve Enter on the focused shell row: a close-only row warns, a
    /// running row requests the generation-guarded shell focus that the
    /// attach scheduler completes after its owner attaches.
    fn session_activation_event(&mut self) -> Option<AppEvent> {
        let row = self.terminal_manager.selected_index.and_then(|index| {
            crate::state::project_managed_shell_rows(self)
                .into_iter()
                .nth(index)
        })?;
        if row.close_only {
            self.warning_message =
                Some("Cannot focus a non-running agent's shell (close-only).".to_string());
            return None;
        }
        Some(AppEvent::RequestShellFocus {
            agent_id: row.agent_id.clone(),
            origin: crate::state::ShellFocusOrigin::ManagerEnter,
        })
    }

    fn reveal_host_panel_selection(&mut self, kind: HostPanelModelSource, viewport_rows: usize) {
        let selected = match kind {
            HostPanelModelSource::RepositoryList => self.selected_repository_visible_index(),
            HostPanelModelSource::AgentList => self.selected_agent_local_index(),
            HostPanelModelSource::SessionList => self.terminal_manager.selected_index,
            // All four bucket rows are always on screen.
            // Card selection never scrolls the grid: paging is explicit.
            HostPanelModelSource::WorkbenchStatus
            | HostPanelModelSource::WorkbenchCards
            | HostPanelModelSource::SearchInput
            | HostPanelModelSource::AgentTypeAvailability
            | HostPanelModelSource::AgentPreview => None,
        };
        let Some(selected) = selected.and_then(|index| u32::try_from(index).ok()) else {
            return;
        };
        let viewport_rows = u32::try_from(viewport_rows.max(1)).unwrap_or(u32::MAX);
        let offset = match kind {
            HostPanelModelSource::RepositoryList => &mut self.repository_scroll_offset,
            HostPanelModelSource::AgentList => &mut self.agent_scroll_offset,
            HostPanelModelSource::SessionList => &mut self.session_scroll_offset,
            HostPanelModelSource::WorkbenchStatus
            | HostPanelModelSource::WorkbenchCards
            | HostPanelModelSource::SearchInput
            | HostPanelModelSource::AgentTypeAvailability
            | HostPanelModelSource::AgentPreview => return,
        };
        if selected < *offset {
            *offset = selected;
        } else if selected >= offset.saturating_add(viewport_rows) {
            *offset = selected.saturating_add(1).saturating_sub(viewport_rows);
        }
    }

    fn select_host_panel_item(&mut self, kind: HostPanelModelSource, id: &Id) -> bool {
        match kind {
            HostPanelModelSource::RepositoryList => {
                let Some((_, repository_index)) = self
                    .visible_repository_indices()
                    .into_iter()
                    .enumerate()
                    .find(|(visible_index, _)| {
                        Id::internal_indexed(InternalId::RepositoryItem, *visible_index) == *id
                    })
                else {
                    return false;
                };
                self.select_repository_by_index(repository_index);
                true
            }
            HostPanelModelSource::AgentList => {
                let Some(repository) = self.selected_repository() else {
                    return false;
                };
                let indices = self.agent_indices_for_repository(&repository.id);
                let Some(local_index) = indices.iter().enumerate().find_map(|(local_index, _)| {
                    (Id::internal_indexed(InternalId::AgentItem, local_index) == *id)
                        .then_some(local_index)
                }) else {
                    return false;
                };
                self.select_agent_by_local_index(local_index);
                true
            }
            HostPanelModelSource::SessionList => {
                let count = crate::state::project_managed_shell_rows(self).len();
                let Some(index) = (0..count)
                    .find(|index| Id::internal_indexed(InternalId::SessionItem, *index) == *id)
                else {
                    return false;
                };
                self.terminal_manager.selected_index = Some(index);
                true
            }
            HostPanelModelSource::WorkbenchStatus => {
                let count = crate::workbench_view::STATUS_BLOCK_ORDER.len();
                let Some(index) = (0..count).find(|index| {
                    Id::internal_indexed(InternalId::StatusBucketItem, *index) == *id
                }) else {
                    return false;
                };
                // Cursor moves never reset the page; only toggles do.
                self.workbench.filter_cursor = index;
                true
            }
            HostPanelModelSource::WorkbenchCards => self.select_workbench_card(id),
            HostPanelModelSource::SearchInput
            | HostPanelModelSource::AgentTypeAvailability
            | HostPanelModelSource::AgentPreview => false,
        }
    }
    fn select_workbench_card(&mut self, id: &Id) -> bool {
        let inputs = crate::host_panel_models::workbench_agent_inputs(self);
        let repository_filter = self
            .split_filter
            .as_ref()
            .map(|repository| repository.0.as_str());
        let order = crate::workbench_view::ordered_agent_ids(
            &inputs,
            self.workbench.status_filter.mask(),
            repository_filter,
        );
        let Some(target) = (0..order.len())
            .find(|index| Id::internal_indexed(InternalId::WorkbenchCardItem, *index) == *id)
            .map(|index| order[index].clone())
        else {
            return false;
        };
        self.select_workbench_agent(&target);
        true
    }
}
