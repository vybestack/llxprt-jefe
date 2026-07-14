use crate::list_viewport::{ListMove, move_selection};

use super::{AppState, PaneFocus};

impl AppState {
    pub(super) fn handle_navigate_page(&mut self, movement: ListMove) {
        match self.pane_focus {
            PaneFocus::Repositories => self.navigate_repository_page(movement),
            PaneFocus::Agents => self.navigate_agent_page(movement),
            PaneFocus::Terminal => {}
        }
    }

    fn navigate_repository_page(&mut self, movement: ListMove) {
        let visible_indices = self.visible_repository_indices();
        let selected = self.selected_repository_visible_index();
        let next = move_selection(selected, visible_indices.len(), movement);
        if next == selected {
            return;
        }
        self.remember_selected_agent_for_current_repo();
        self.selected_repository_index = next.and_then(|index| visible_indices.get(index).copied());
        self.restore_selected_agent_for_current_repo();
        self.reset_terminal_scrollback();
    }

    fn navigate_agent_page(&mut self, movement: ListMove) {
        let Some(repository_id) = self.selected_repository_id().cloned() else {
            self.selected_agent_index = None;
            return;
        };
        let visible_indices = self.agent_indices_for_repository(&repository_id);
        let selected = self.selected_agent_index.and_then(|selected_index| {
            visible_indices
                .iter()
                .position(|global_index| *global_index == selected_index)
        });
        let next = move_selection(selected, visible_indices.len(), movement);
        if next == selected {
            return;
        }
        self.selected_agent_index = next.and_then(|index| visible_indices.get(index).copied());
        self.remember_selected_agent_for_current_repo();
        self.reset_terminal_scrollback();
    }
}
