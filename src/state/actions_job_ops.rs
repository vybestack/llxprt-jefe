//! Deterministic Actions job-inspection transitions and display-row bounds.
//!
//! Reducers in `actions_ops` delegate here so focus normalization, wrapping,
//! expansion, and scroll-follow all consume one pure detail projection.

use super::{ActionsFocus, AppState};
use crate::actions_detail_view::{normalize_job_focus, project_actions_detail};
use crate::layout::ActionsDetailGeometry;
use crate::messages::{NavDir, ScrollDir};

impl AppState {
    /// Clear all inspection state immediately when a new run/list load begins.
    pub(super) fn reset_actions_inspection(&mut self) {
        self.actions_state.expanded_jobs.clear();
        self.actions_state.focused_job_index = None;
        self.actions_state.detail_scroll_offset = 0;
    }

    fn detail_geometry(&self) -> ActionsDetailGeometry {
        ActionsDetailGeometry {
            viewport_rows: self.actions_state.detail_viewport_rows,
            content_width: self.actions_state.detail_content_width,
        }
    }

    fn detail_projection(&self) -> Option<crate::actions_detail_view::ActionsDetailProjection> {
        let detail = self.actions_state.run_detail.as_ref()?;
        Some(project_actions_detail(
            detail,
            &self.actions_state.expanded_jobs,
            self.actions_state.focused_job_index,
            self.detail_geometry(),
        ))
    }

    fn normalize_actions_job_focus(&mut self) {
        self.actions_state.focused_job_index =
            self.actions_state.run_detail.as_ref().and_then(|detail| {
                normalize_job_focus(detail, self.actions_state.focused_job_index)
            });
    }

    fn max_actions_detail_scroll_offset(&self) -> usize {
        self.detail_projection().map_or(0, |projection| {
            projection.max_scroll_offset(self.actions_state.detail_viewport_rows)
        })
    }

    /// Public Actions detail bound used by mouse-wheel routing.
    #[must_use]
    pub fn actions_max_detail_scroll_offset(&self) -> usize {
        self.max_actions_detail_scroll_offset()
    }

    fn clamp_actions_detail_scroll(&mut self) {
        self.actions_state.detail_scroll_offset = self
            .actions_state
            .detail_scroll_offset
            .min(self.max_actions_detail_scroll_offset());
    }

    fn follow_focused_job(&mut self) {
        self.normalize_actions_job_focus();
        let Some(projection) = self.detail_projection() else {
            self.actions_state.detail_scroll_offset = 0;
            return;
        };
        self.actions_state.detail_scroll_offset = projection.reveal_focused_job(
            self.actions_state.detail_scroll_offset,
            self.actions_state.detail_viewport_rows,
        );
    }

    pub(super) fn set_actions_detail_geometry(
        &mut self,
        viewport_rows: usize,
        content_width: usize,
    ) -> bool {
        self.actions_state.detail_viewport_rows = viewport_rows;
        self.actions_state.detail_content_width = content_width;
        self.follow_focused_job();
        true
    }

    pub(super) fn handle_actions_detail_scroll(&mut self, dir: ScrollDir) -> bool {
        let max = self.max_actions_detail_scroll_offset();
        let current = self.actions_state.detail_scroll_offset.min(max);
        self.actions_state.detail_scroll_offset = match dir {
            ScrollDir::Up => current.saturating_sub(1),
            ScrollDir::Down => current.saturating_add(1).min(max),
            ScrollDir::PageUp => current.saturating_sub(super::VIEWPORT_PAGE_JUMP),
            ScrollDir::PageDown => current.saturating_add(super::VIEWPORT_PAGE_JUMP).min(max),
        };
        true
    }

    fn focused_job_id(&mut self) -> Option<u64> {
        self.normalize_actions_job_focus();
        let index = self.actions_state.focused_job_index?;
        self.actions_state
            .run_detail
            .as_ref()?
            .jobs
            .get(index)
            .map(|job| job.id)
    }

    pub(super) fn expand_actions_job(&mut self) -> bool {
        if let Some(job_id) = self.focused_job_id() {
            self.actions_state.expanded_jobs.insert(job_id);
            self.follow_focused_job();
        }
        true
    }

    pub(super) fn collapse_actions_job(&mut self) -> bool {
        if let Some(job_id) = self.focused_job_id() {
            self.actions_state.expanded_jobs.remove(&job_id);
        }
        self.follow_focused_job();
        true
    }

    pub(super) fn handle_actions_detail_escape(&mut self) -> bool {
        let collapsed = self
            .focused_job_id()
            .is_some_and(|job_id| self.actions_state.expanded_jobs.remove(&job_id));
        if collapsed {
            self.follow_focused_job();
        } else {
            self.actions_state.focus = ActionsFocus::RunList;
            self.clamp_actions_detail_scroll();
        }
        true
    }

    pub(super) fn navigate_actions_job(&mut self, dir: NavDir) -> bool {
        self.normalize_actions_job_focus();
        let Some(detail) = self.actions_state.run_detail.as_ref() else {
            return true;
        };
        let Some(current) = self.actions_state.focused_job_index else {
            return true;
        };
        let last = detail.jobs.len().saturating_sub(1);
        self.actions_state.focused_job_index = Some(match dir {
            NavDir::Up => current.saturating_sub(1),
            NavDir::Down => current.saturating_add(1).min(last),
            _ => current,
        });
        self.follow_focused_job();
        true
    }
}
