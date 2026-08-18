//! Pure host projection for manifest-backed provider screens.
//!
//! Produces a geometry-aware view of every descriptor panel, driven entirely by
//! the frame-owned [`ResolvedLayout`]. Each visible panel carries its resolved
//! chrome/content rectangle, focus state, lifecycle status, and wrapped +
//! scroll-clipped body lines. No geometry is derived outside the layout
//! snapshot: the renderer, mouse router, and projection all read the same
//! rectangles.

pub use crate::host_controls::PanelHitTarget;
use crate::host_controls::project_control;
use crate::runtime::provider::protocol::{Affordance, PanelBody};
use crate::state::provider_panels::{PanelLifecycle, ProviderPanelState};
use crate::workbench::{PanelId, Rect, ResolvedLayout, ScreenDescriptor};

// ---------------------------------------------------------------------------
// View structures consumed by the iocraft component
// ---------------------------------------------------------------------------

/// One lowered screen projected against a resolved geometry snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderScreenView {
    /// Descriptor-owned screen title.
    pub title: String,
    /// Every descriptor panel, in declaration order, projected at resolved geometry.
    pub panels: Vec<PanelProjection>,
    /// Whether the layout fell back to the too-small survivor.
    pub too_small: bool,
}

/// One descriptor panel projected against its resolved rectangle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelProjection {
    /// Which panel this is.
    pub id: PanelId,
    /// Snapshot title, or the panel id before a model is available.
    pub title: String,
    /// Whether the panel occupies cells this frame.
    pub visible: bool,
    /// Whether the panel has keyboard focus.
    pub focused: bool,
    /// The panel's whole rectangle (border + title).
    pub chrome: Rect,
    /// The rectangle inside the border and title.
    pub content: Rect,
    /// Lifecycle-derived rendering status.
    pub status: PanelStatus,
    /// Wrapped and scroll-clipped display lines.
    pub lines: Vec<String>,
    /// Largest valid host-local scroll offset for this projection.
    pub max_scroll_offset: u32,
    /// Semantic target occupying each display line, aligned with `lines`.
    pub hit_targets: Vec<Option<PanelHitTarget>>,
}

/// Distinct lifecycle-derived rendering status for a provider panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelStatus {
    /// A snapshot is loading.
    Loading,
    /// No provider instance exists or the provider is unavailable.
    Unavailable,
    /// The last candidate was invalid; a prior model may be stale.
    Failed,
    /// The panel is suspended.
    Suspended,
    /// The panel is active with an accepted model.
    Active,
    /// The panel is disposed.
    Disposed,
}

// ---------------------------------------------------------------------------
// Public projection entry point
// ---------------------------------------------------------------------------

/// Project one lowered screen against its resolved geometry.
///
/// Every visible panel's `chrome` and `content` come from `layout`; no geometry
/// is re-derived. Body text is Unicode-aware wrapped at the content width and
/// clipped by the host-local scroll offset.
///
/// # Panics
///
/// Panics when `layout` is not the resolver output for `descriptor`. The layout
/// resolver's closed postcondition is one entry for every descriptor panel,
/// including collapsed panels; continuing with partial geometry would create a
/// second layout interpretation and make rendering and hit testing disagree.
#[must_use]
pub fn project_provider_screen(
    descriptor: &ScreenDescriptor,
    screen_instance_id: u64,
    panels: &ProviderPanelState,
    layout: &ResolvedLayout,
    focused_panel: &PanelId,
) -> ProviderScreenView {
    let projected = descriptor
        .panels
        .iter()
        .map(|descriptor_panel| {
            let Some(resolved) = layout.panel(&descriptor_panel.id) else {
                panic!(
                    "resolved layout omitted descriptor panel {}",
                    descriptor_panel.id
                );
            };
            project_one_panel(PanelProjectionInput {
                id: descriptor_panel.id,
                focused: &descriptor_panel.id == focused_panel,
                visible: resolved.visible,
                chrome: resolved.chrome,
                content: resolved.content,
                panels,
                instance: panels.panel_for_screen(screen_instance_id, &descriptor_panel.id),
            })
        })
        .collect();
    ProviderScreenView {
        title: descriptor.title.clone(),
        panels: projected,
        too_small: layout.too_small.is_some(),
    }
}

// ---------------------------------------------------------------------------
// Per-panel projection
// ---------------------------------------------------------------------------

struct PanelProjectionInput<'a> {
    id: PanelId,
    focused: bool,
    visible: bool,
    chrome: Rect,
    content: Rect,
    panels: &'a ProviderPanelState,
    instance: Option<crate::state::provider_panels::PanelInstanceId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProjectedRow {
    text: String,
    target: Option<PanelHitTarget>,
}

impl ProjectedRow {
    fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            target: None,
        }
    }

    fn targeted(text: impl Into<String>, target: PanelHitTarget) -> Self {
        Self {
            text: text.into(),
            target: Some(target),
        }
    }
}

fn project_one_panel(input: PanelProjectionInput<'_>) -> PanelProjection {
    if !input.visible {
        return hidden_panel(input.id);
    }
    let Some(instance) = input.instance else {
        return unavailable_panel(input.id, input.focused, input.chrome, input.content);
    };
    let lifecycle = input.panels.lifecycle(instance);
    let stale = input.panels.accepted_model_is_stale(instance);
    let snapshot = input.panels.accepted_snapshot(instance);
    let host_local = input.panels.host_local(instance);
    let scroll_offset = host_local.map_or(0, |local| local.scroll_offset);
    let selected_id = host_local.and_then(|local| local.selected_id.as_ref());
    let form_draft = host_local.and_then(|local| local.form_draft.as_ref());

    let status = panel_status(lifecycle, snapshot.is_some(), stale);
    let title = snapshot.map_or_else(
        || input.id.as_str().to_owned(),
        |snapshot| snapshot.title.clone(),
    );
    let mut rows = Vec::new();
    if let Some(snapshot) = snapshot {
        if snapshot.loading {
            rows.push(ProjectedRow::plain("loading…"));
        }
        if stale {
            rows.push(ProjectedRow::plain("stale"));
        }
        let body_width = usize::from(input.content.width.max(1));
        rows.extend(project_description(
            snapshot.description.as_deref(),
            body_width,
        ));
        rows.extend(
            project_control(snapshot, selected_id, form_draft, body_width)
                .into_iter()
                .map(|row| ProjectedRow {
                    text: row.text,
                    target: row.target,
                }),
        );
        project_affordances(snapshot, &mut rows);
    }
    let max_scroll_offset =
        u32::try_from(rows.len().saturating_sub(usize::from(input.content.height)))
            .unwrap_or(u32::MAX);
    let clipped = clip_to_content(rows, scroll_offset, input.content.height);
    let lines = clipped.iter().map(|row| row.text.clone()).collect();
    let hit_targets = clipped.into_iter().map(|row| row.target).collect();
    PanelProjection {
        id: input.id,
        title,
        visible: true,
        focused: input.focused,
        chrome: input.chrome,
        content: input.content,
        status,
        lines,
        max_scroll_offset,
        hit_targets,
    }
}

fn hidden_panel(id: PanelId) -> PanelProjection {
    let title = id.as_str().to_owned();
    PanelProjection {
        id,
        title,
        visible: false,
        focused: false,
        chrome: Rect::default(),
        content: Rect::default(),
        status: PanelStatus::Unavailable,
        lines: Vec::new(),
        max_scroll_offset: 0,
        hit_targets: Vec::new(),
    }
}

fn unavailable_panel(id: PanelId, focused: bool, chrome: Rect, content: Rect) -> PanelProjection {
    let title = id.as_str().to_owned();
    PanelProjection {
        id,
        title,
        visible: true,
        focused,
        chrome,
        content,
        status: PanelStatus::Unavailable,
        lines: vec!["provider unavailable".to_owned()],
        max_scroll_offset: 0,
        hit_targets: vec![None],
    }
}

fn panel_status(lifecycle: Option<PanelLifecycle>, has_snapshot: bool, stale: bool) -> PanelStatus {
    match lifecycle {
        None => PanelStatus::Unavailable,
        Some(PanelLifecycle::Declared | PanelLifecycle::Activating) if !has_snapshot => {
            PanelStatus::Loading
        }
        Some(PanelLifecycle::Failed) if stale => PanelStatus::Failed,
        Some(PanelLifecycle::Failed) => PanelStatus::Failed,
        Some(PanelLifecycle::Suspended) => PanelStatus::Suspended,
        Some(PanelLifecycle::Disposed | PanelLifecycle::Disposing) => PanelStatus::Disposed,
        Some(PanelLifecycle::Active | PanelLifecycle::Activating) => PanelStatus::Active,
        Some(PanelLifecycle::Declared) => PanelStatus::Loading,
    }
}

/// Clip body rows to the content height, applying the host scroll offset.
fn clip_to_content(
    rows: Vec<ProjectedRow>,
    scroll_offset: u32,
    content_height: u16,
) -> Vec<ProjectedRow> {
    if content_height == 0 {
        return Vec::new();
    }
    let max = usize::from(content_height);
    let offset = usize::try_from(scroll_offset).unwrap_or(0);
    if offset >= rows.len() {
        return Vec::new();
    }
    rows.into_iter().skip(offset).take(max).collect()
}

fn project_description(description: Option<&str>, width: usize) -> Vec<ProjectedRow> {
    description.map_or_else(Vec::new, |description| {
        crate::text_wrap::wrap_text(description, width)
            .into_iter()
            .map(|row| ProjectedRow::plain(row.text))
            .collect()
    })
}

// ---------------------------------------------------------------------------
// Affordance projection
// ---------------------------------------------------------------------------

fn project_affordances(
    snapshot: &crate::runtime::provider::protocol::PanelSnapshot,
    rows: &mut Vec<ProjectedRow>,
) {
    rows.extend(
        snapshot
            .action_affordances
            .iter()
            .filter(|affordance| !body_projects_affordance(&snapshot.body, affordance))
            .map(|affordance| {
                if affordance.enabled {
                    ProjectedRow::targeted(
                        format!("[{}] {}", affordance.id, affordance.label),
                        affordance_target(snapshot, affordance),
                    )
                } else {
                    ProjectedRow::targeted(
                        format!(
                            "[{}] {} (unavailable: {})",
                            affordance.id,
                            affordance.label,
                            affordance
                                .unavailable_reason
                                .as_deref()
                                .filter(|reason| !reason.trim().is_empty())
                                .unwrap_or("unavailable")
                        ),
                        PanelHitTarget::Unavailable,
                    )
                }
            }),
    );
}

fn body_projects_affordance(body: &PanelBody, affordance: &Affordance) -> bool {
    match body {
        PanelBody::Form(body) => body.submit_action == affordance.action_id,
        PanelBody::Error(body) => body.retry_action.as_ref() == Some(&affordance.id),
        PanelBody::List(_)
        | PanelBody::Tree(_)
        | PanelBody::Detail(_)
        | PanelBody::StructuredDiff(_)
        | PanelBody::Status(_)
        | PanelBody::Progress(_)
        | PanelBody::Empty(_) => false,
    }
}

fn affordance_target(
    snapshot: &crate::runtime::provider::protocol::PanelSnapshot,
    affordance: &Affordance,
) -> PanelHitTarget {
    match &snapshot.body {
        PanelBody::Detail(detail) if detail.actions.contains(&affordance.id) => {
            PanelHitTarget::Link(affordance.id.clone())
        }
        _ => PanelHitTarget::Action(affordance.id.clone()),
    }
}

#[cfg(test)]
#[path = "provider_panel_view_tests.rs"]
mod tests;
