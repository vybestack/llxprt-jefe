//! Pure host projection for manifest-backed provider screens.
//!
//! Produces a geometry-aware view of every descriptor panel, driven entirely by
//! the frame-owned [`ResolvedLayout`]. Each visible panel carries its resolved
//! chrome/content rectangle, focus state, lifecycle status, and wrapped +
//! scroll-clipped body lines. No geometry is derived outside the layout
//! snapshot: the renderer, mouse router, and projection all read the same
//! rectangles.

use crate::git_info::GitRepoInfo;
pub use crate::host_controls::PanelHitTarget;
use crate::host_controls::project_control_body;
use crate::runtime::provider::protocol::{Affordance, PanelBody};
use crate::state::AppState;
use crate::state::provider_panels::{PanelLifecycle, ProviderPanelState};
use crate::workbench::descriptor::HostPanelModelSource;
use crate::workbench::{
    FILTER_BAND_PANEL_TYPE, PTY_PANEL_TYPE, PanelId, Rect, ResolvedLayout, ScreenDescriptor,
    ScreenInstanceId,
};
use crate::workbench_view::{WorkbenchRequest, WorkbenchView, build_workbench_view};

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
    /// Which shared renderer consumes this panel's projected content.
    pub render: PanelRender,
    pub hit_targets: Vec<Option<PanelHitTarget>>,
    /// Rectangle-keyed targets for content a row index cannot address: the
    /// card grid packs several cards onto one row, so each visible card
    /// carries its own rectangle (issue #706).
    pub rect_hit_targets: Vec<(Rect, PanelHitTarget)>,
}
/// Shared content renderer selected by the descriptor panel type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelRender {
    /// The closed public host-control projection.
    Control,
    /// The private host PTY rendered through `TerminalView`.
    EmbeddedTerminal,
    /// The retained workbench card grid, rendered by the bespoke grid
    /// renderer from the [`WorkbenchView`] (#706 maintainer decision).
    WorkbenchCards,
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

/// Projection refusal when resolved geometry and the active screen diverge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelProjectionError {
    /// Geometry belongs to another exact screen instance.
    StaleLayout {
        expected: ScreenInstanceId,
        actual: ScreenInstanceId,
    },
    /// Geometry omitted one descriptor-owned panel.
    MissingPanel(PanelId),
}

impl std::fmt::Display for PanelProjectionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleLayout { expected, actual } => write!(
                formatter,
                "resolved layout belongs to screen instance {actual:?}, expected {expected:?}"
            ),
            Self::MissingPanel(panel) => {
                write!(
                    formatter,
                    "resolved layout omitted descriptor panel {panel}"
                )
            }
        }
    }
}

impl std::error::Error for PanelProjectionError {}

// ---------------------------------------------------------------------------
// Public projection entry point
// ---------------------------------------------------------------------------

/// Project one lowered screen against its resolved geometry.
///
/// Every visible panel's `chrome` and `content` come from `layout`; no geometry
/// is re-derived. Body text is Unicode-aware wrapped at the content width and
/// clipped by the host-local scroll offset.
///
/// Returns a typed refusal when `layout` is not the resolver output for
/// `descriptor`. Continuing with partial geometry would create a second layout
/// interpretation and make rendering and hit testing disagree.
pub fn project_provider_screen(
    descriptor: &ScreenDescriptor,
    screen_instance_id: u64,
    panels: &ProviderPanelState,
    layout: &ResolvedLayout,
    focused_panel: &PanelId,
) -> Result<ProviderScreenView, PanelProjectionError> {
    let projected = descriptor
        .panels
        .iter()
        .map(|descriptor_panel| {
            let resolved = layout
                .panel(&descriptor_panel.id)
                .ok_or(PanelProjectionError::MissingPanel(descriptor_panel.id))?;
            Ok(project_one_panel(PanelProjectionInput {
                id: descriptor_panel.id,
                focused: &descriptor_panel.id == focused_panel,
                visible: resolved.visible,
                chrome: resolved.chrome,
                content: resolved.content,
                panels,
                instance: panels.panel_for_screen(screen_instance_id, &descriptor_panel.id),
            }))
        })
        .collect::<Result<Vec<_>, PanelProjectionError>>()?;
    Ok(ProviderScreenView {
        title: descriptor.title.clone(),
        panels: projected,
        too_small: layout.too_small.is_some(),
    })
}

/// Project the active definition through provider-backed and host-owned controls.
pub fn project_current_screen(
    state: &AppState,
    descriptor: &ScreenDescriptor,
    layout: &ResolvedLayout,
) -> Result<ProviderScreenView, PanelProjectionError> {
    let current_instance = state.nav.current().id;
    if layout.screen_instance != current_instance {
        return Err(PanelProjectionError::StaleLayout {
            expected: current_instance,
            actual: layout.screen_instance,
        });
    }
    let instance_id = current_instance.get();
    let registry = state.published_workbench().screen_registry();
    let mut view = project_provider_screen(
        descriptor,
        instance_id,
        state.provider_panels(),
        layout,
        &state.nav.current().panel_focus,
    )?;
    for (panel_descriptor, projection) in descriptor.panels.iter().zip(&mut view.panels) {
        if !projection.visible
            || registry
                .panel_binding(descriptor.id, &panel_descriptor.id)
                .is_some()
            || state
                .provider_panels()
                .panel_for_screen(instance_id, &panel_descriptor.id)
                .is_some()
        {
            continue;
        }
        project_declared_content(&descriptor.id, panel_descriptor, projection, state);
    }
    Ok(view)
}

/// Fill one unbound panel from the content its declaration implies: the
/// private host PTY, the Repositories filter band, or a host control.
fn project_declared_content(
    screen: &crate::workbench::ScreenIdentity,
    panel_descriptor: &crate::workbench::PanelDescriptor,
    projection: &mut PanelProjection,
    state: &AppState,
) {
    if panel_descriptor.panel_type.as_str() == PTY_PANEL_TYPE {
        // The Terminal Manager's preview pane carries the single live
        // viewer only while the shell overlay runs; the rest of the time
        // it is the throttled read-only preview captured from the
        // manager channel — never a second live viewer.
        if screen == &crate::workbench::TERMINALS_IDENTITY && !state.shell_overlay_active() {
            project_shell_preview(projection, state);
            return;
        }
        "Terminal".clone_into(&mut projection.title);
        projection.status = PanelStatus::Active;
        projection.lines.clear();
        projection.hit_targets.clear();
        projection.max_scroll_offset = 0;
        projection.render = PanelRender::EmbeddedTerminal;
        return;
    }
    if panel_descriptor.panel_type.as_str() == FILTER_BAND_PANEL_TYPE {
        project_filter_band(projection, state);
        return;
    }
    let Some(capability) = panel_descriptor.host_capability else {
        return;
    };
    if capability.model_source() == HostPanelModelSource::WorkbenchCards {
        // The grid survives the Repositories cutover with its own
        // renderer; the host capability still owns the input
        // contract (selection, attach, paging) declared in #706.
        let model = crate::host_panel_models::project_host_panel(state, capability.model_source());
        projection.title = model.title;
        projection.status = PanelStatus::Active;
        projection.lines.clear();
        projection.hit_targets.clear();
        projection.max_scroll_offset = 0;
        projection.render = PanelRender::WorkbenchCards;
        projection.rect_hit_targets = workbench_card_hit_targets(state, projection.content);
        return;
    }
    let model = crate::host_panel_models::project_host_panel(state, capability.model_source());
    if crate::host_controls::ControlKind::from(model.body.kind()) == capability.control_kind() {
        project_host_model(projection, model);
    }
}

/// Build the retained workbench card-grid view from app state.
///
/// Keeps the legacy wiring the grid depends on: configured origins feed the
/// card headers, the split filter scopes the grid, and the retained page
/// counter selects the window. The bespoke renderer owns geometry, so the
/// viewport here is the panel's content rectangle, not the terminal.
#[must_use]
pub fn workbench_view_from_state(state: &AppState, cols: u16, rows: u16) -> WorkbenchView {
    let agents: Vec<_> = state
        .agents
        .iter()
        .map(|agent| {
            let git_info = state
                .repository_by_id(&agent.repository_id)
                .map(|repository| GitRepoInfo::from_configured_origin(&repository.github_repo));
            let observation = state.observations.get(&agent.id).cloned();
            (agent.clone(), git_info, observation)
        })
        .collect();
    let repository_filter = state.split_filter.as_ref().map(|id| id.0.clone());
    build_workbench_view(&WorkbenchRequest {
        agents,
        status_filter: state.workbench.status_filter.mask(),
        repository_filter,
        terminal_width: usize::from(cols),
        terminal_height: usize::from(rows),
        page: state.workbench.page,
    })
}

/// One rectangle per visible card, in the grid's row-major paint order.
///
/// The bespoke grid renderer paints card `i` at row `i / columns`, column
/// `i % columns`, on the same column/row strides the layout resolves
/// (#706). Sharing the strides through [`WorkbenchView`]'s own layout keeps
/// the renderer and the hit targets in lockstep.
#[must_use]
pub fn workbench_card_hit_targets(state: &AppState, content: Rect) -> Vec<(Rect, PanelHitTarget)> {
    let view = workbench_view_from_state(state, content.width, content.height);
    let columns = view.layout.columns.max(1);
    // The painted card is an interior `card_width` wrapped in a bordered Box:
    // left + right borders add 2 columns, so the footprint width is
    // `card_width + 2`. The flex-row places cards side by side with no
    // explicit gap, so the column stride equals the footprint width (issue
    // #706). Using `card_width + CARD_GAP` here (as before) under-counted
    // by 1 because `CARD_GAP` is 1 but the borders add 2.
    let column_stride = view
        .layout
        .card_width
        .saturating_add(crate::workbench_view::CARD_BORDER_COLS);
    // The painted card is its interior lines plus the bordered box: the
    // same per-row budget `resolve_vertical` divides by.
    let card_height = view
        .layout
        .todo_window
        .saturating_add(crate::workbench_view::CARD_CHROME_LINES)
        .saturating_add(1);
    // Model ids index the full ordered agent list; `view.cards` is the
    // current page's slice of it.
    let cards_per_page = view.layout.rows_visible.saturating_mul(columns).max(1);
    let page_start = view.layout.page.saturating_mul(cards_per_page);
    let content_col = usize::from(content.col);
    let content_row = usize::from(content.row);
    let content_width = usize::from(content.width);
    view.cards
        .iter()
        .enumerate()
        .filter_map(|(index, _card)| {
            let row = index / columns;
            let column = index % columns;
            let col_offset = column.saturating_mul(column_stride);
            let width = column_stride.min(content_width.saturating_sub(col_offset));
            let (Ok(col), Ok(row), Ok(width), Ok(height)) = (
                u16::try_from(content_col.saturating_add(col_offset)),
                u16::try_from(content_row.saturating_add(row.saturating_mul(card_height))),
                u16::try_from(width),
                u16::try_from(card_height),
            ) else {
                return None;
            };
            Some((
                Rect {
                    col,
                    row,
                    width,
                    height,
                },
                PanelHitTarget::ListItem(crate::domain::Id::internal_indexed(
                    crate::domain::InternalId::WorkbenchCardItem,
                    page_start + index,
                )),
            ))
        })
        .collect()
}

/// Project the Repositories screen's filter band.
///
/// The legacy rail showed the search row plus a terminal-style cursor; the
/// band carries the same line at full width.
fn project_filter_band(projection: &mut PanelProjection, state: &AppState) {
    let line =
        crate::overlay_controls::project_search(state, usize::from(crate::layout::LEFT_COL_WIDTH))
            .rows
            .into_iter()
            .next()
            .map_or_else(|| "Filter: ".to_owned(), |row| row.text);
    projection.title.clear();
    projection.status = PanelStatus::Active;
    projection.lines = vec![format!("{line}_")];
    projection.hit_targets = vec![None];
    projection.max_scroll_offset = 0;
    projection.render = PanelRender::Control;
}

/// Project the Terminal Manager's throttled shell preview into a PTY panel.
///
/// Mirrors the retired compiled renderer: a header identifying the selected
/// shell's owner, then the last captured preview lines (or the same
/// placeholders the compiled screen showed) so an unfocused manager never
/// renders a second live viewer.
fn project_shell_preview(projection: &mut PanelProjection, state: &AppState) {
    let rows = crate::state::project_managed_shell_rows(state);
    let selected = state
        .terminal_manager
        .selected_index
        .and_then(|index| rows.get(index));
    let Some(row) = selected else {
        projection.status = PanelStatus::Active;
        projection.lines.clear();
        projection.hit_targets.clear();
        projection.max_scroll_offset = 0;
        return;
    };
    let manager = &state.terminal_manager;
    let mut lines = vec![
        format!("Agent: {}", row.agent_name),
        format!(
            "Repo: {} · Workdir: {} · Status: {}{}",
            row.repository_name,
            row.work_dir,
            row.status_label,
            if row.close_only { " (close-only)" } else { "" }
        ),
        crate::ui::components::SEPARATOR_LINE.to_string(),
    ];
    if let Some(pending) = &manager.pending_focus {
        lines.push(format!("Focusing {}\u{2026}", pending.agent_id.0));
    }
    if manager.preview.failed {
        lines.push("(preview unavailable)".to_owned());
    } else if manager.preview.lines.is_empty() {
        if row.close_only {
            lines.push("(owner not running \u{2014} close-only)".to_owned());
        } else {
            lines.push("(capturing preview\u{2026})".to_owned());
        }
    } else {
        lines.extend(manager.preview.lines.iter().cloned());
    }
    let maximum = lines.len().saturating_sub(1);
    projection.status = PanelStatus::Active;
    projection.lines = lines;
    projection.hit_targets.clear();
    projection.max_scroll_offset = u32::try_from(maximum).unwrap_or(u32::MAX);
}

/// Fill one host-owned panel from its projected model, clamping the retained
/// scroll offset to the visible row window after any source shrink.
fn project_host_model(
    projection: &mut PanelProjection,
    model: crate::host_panel_models::HostPanelModel,
) {
    let body_width = usize::from(projection.content.width.max(1));
    let rows = project_model_rows(ModelProjectionInput {
        body: &model.body,
        affordances: &model.action_affordances,
        description: None,
        loading: false,
        stale: false,
        selected_id: model.selected_id.as_ref(),
        form_draft: None,
        body_width,
    });
    projection.title = model.title;
    projection.status = PanelStatus::Active;
    let maximum = rows
        .len()
        .saturating_sub(usize::from(projection.content.height));
    projection.max_scroll_offset = u32::try_from(maximum).unwrap_or(u32::MAX);
    let clipped = clip_to_content(rows, model.scroll_offset, projection.content.height);
    projection.lines = clipped.iter().map(|row| row.text.clone()).collect();
    projection.hit_targets = clipped.into_iter().map(|row| row.target).collect();
    projection.render = PanelRender::Control;
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

struct ModelProjectionInput<'a> {
    body: &'a PanelBody,
    affordances: &'a [Affordance],
    description: Option<&'a str>,
    loading: bool,
    stale: bool,
    selected_id: Option<&'a crate::domain::Id>,
    form_draft: Option<&'a crate::domain::TypedMap>,
    body_width: usize,
}

fn project_model_rows(input: ModelProjectionInput<'_>) -> Vec<ProjectedRow> {
    let mut rows = Vec::new();
    if input.loading {
        rows.push(ProjectedRow::plain("loading…"));
    }
    if input.stale {
        rows.push(ProjectedRow::plain("stale"));
    }
    rows.extend(project_description(input.description, input.body_width));
    rows.extend(
        project_control_body(
            input.body,
            input.affordances,
            input.selected_id,
            input.form_draft,
            input.body_width,
        )
        .into_iter()
        .map(|row| ProjectedRow {
            text: row.text,
            target: row.target,
        }),
    );
    project_affordances(input.body, input.affordances, &mut rows);
    rows
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
    let rows = snapshot.map_or_else(Vec::new, |snapshot| {
        project_model_rows(ModelProjectionInput {
            body: &snapshot.body,
            affordances: &snapshot.action_affordances,
            description: snapshot.description.as_deref(),
            loading: snapshot.loading,
            stale,
            selected_id,
            form_draft,
            body_width: usize::from(input.content.width.max(1)),
        })
    });
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
        render: PanelRender::Control,
        rect_hit_targets: Vec::new(),
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
        render: PanelRender::Control,
        rect_hit_targets: Vec::new(),
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
        render: PanelRender::Control,
        rect_hit_targets: Vec::new(),
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

fn project_affordances(body: &PanelBody, affordances: &[Affordance], rows: &mut Vec<ProjectedRow>) {
    rows.extend(
        affordances
            .iter()
            .filter(|affordance| !body_projects_affordance(body, affordance))
            .map(|affordance| {
                if affordance.enabled {
                    ProjectedRow::targeted(
                        format!("[{}] {}", affordance.id, affordance.label),
                        affordance_target(body, affordance),
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

fn affordance_target(body: &PanelBody, affordance: &Affordance) -> PanelHitTarget {
    match body {
        PanelBody::Detail(detail) if detail.actions.contains(&affordance.id) => {
            PanelHitTarget::Link(affordance.id.clone())
        }
        _ => PanelHitTarget::Action(affordance.id.clone()),
    }
}

#[cfg(test)]
#[path = "provider_panel_view_tests.rs"]
mod tests;
