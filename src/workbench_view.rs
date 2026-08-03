//! Pure, iocraft-free multi-agent status workbench projection (issue #626).
//!
//! Given agents, observations, filters, terminal geometry, and a page index,
//! [`build_workbench_view`] returns a fully resolved, finite view model: the
//! column count, the per-card todo-window size, the sorted cards for the
//! current page, bucket counts, and any empty-state reason. It performs no
//! I/O, holds no iocraft types, reads no clock internally (any needed instant
//! is passed in), and is exhaustively testable.
//!
//! Cards are FIXED HEIGHT with a WINDOWED todo list. Both the horizontal axis
//! (columns / card width) and the vertical axis (todo-window size / rows /
//! paging) are responsive to terminal geometry.

use std::time::Instant;

use crate::domain::observation::{AgentObservation, Availability, FieldState, TodoItem, TodoList};
use crate::domain::{Agent, AgentId};
use crate::git_info::GitRepoInfo;
use crate::list_viewport::fit_text_to_width;
use crate::status_precedence::{ResolvedStatus, resolve_status};
use unicode_width::UnicodeWidthStr;

// ---------------------------------------------------------------------------
// Layout constants
// ---------------------------------------------------------------------------

/// Minimum interior card width (issue #626 horizontal rule).
pub const MIN_CARD_WIDTH: usize = 40;
/// Maximum interior card width (issue #626 horizontal rule).
pub const MAX_CARD_WIDTH: usize = 52;
/// Horizontal gap between cards in cells.
pub const CARD_GAP: usize = 1;
/// Reserved sidebar width in cells (the left repository/status rail).
pub const SIDEBAR_WIDTH: usize = 22;

/// Minimum todo-window lines (issue #626 vertical rule: W_MIN = 3).
pub const TODO_WINDOW_MIN: usize = 3;
/// Maximum todo-window lines (issue #626 vertical rule: W_MAX = 8).
pub const TODO_WINDOW_MAX: usize = 8;

/// Fixed chrome lines in every card: top border, need line, blank, todo
/// progress header, blank, last-message line, bottom border.
pub const CARD_CHROME_LINES: usize = 7;

/// Fixed chrome lines consumed outside the card grid by the screen itself
/// (status bar, filter lines, footer). The vertical rule subtracts this from
/// the terminal height before dividing among card rows.
pub const SCREEN_CHROME_LINES: usize = 6;

// ---------------------------------------------------------------------------
// Public view-model types
// ---------------------------------------------------------------------------

/// Horizontal + vertical layout resolution for one render frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkbenchLayout {
    /// Number of card columns.
    pub columns: usize,
    /// Interior card width in cells (excludes borders).
    pub card_width: usize,
    /// Todo-window size `W` derived from terminal height.
    pub todo_window: usize,
    /// Card rows visible on the current page.
    pub rows_visible: usize,
    /// Zero-based current page index.
    pub page: usize,
    /// Total page count.
    pub page_count: usize,
}

/// The four status buckets used for sorting and filtering.
///
/// Ordering is significant: [`StatusBucket::NeedsYou`] precedes
/// [`StatusBucket::Working`], which precedes [`StatusBucket::Ready`], which
/// precedes [`StatusBucket::Stale`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusBucket {
    /// An explicit unresolved wait — blocked on the user (highest priority).
    NeedsYou,
    /// Actively working: a live turn or active tool/activity.
    Working,
    /// Idle, nothing pending.
    Ready,
    /// Stale, disconnected, protocol error, dead, or otherwise not live.
    Stale,
}

impl StatusBucket {
    /// The stable index into a four-element bucket-counts array.
    #[must_use]
    pub const fn as_index(self) -> usize {
        match self {
            Self::NeedsYou => 0,
            Self::Working => 1,
            Self::Ready => 2,
            Self::Stale => 3,
        }
    }
}

/// One windowed todo line, already clipped to the card interior.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TodoLine {
    /// The rendered text (checkbox marker + clipped todo text).
    pub text: String,
    /// Whether this line is the derived current/active item (a hint, not
    /// authoritative — the marker is derived from completion state).
    pub is_current: bool,
    /// Whether this line is a real item versus blank padding.
    pub is_blank: bool,
}

/// The windowed slice of a full todo list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TodoWindow {
    /// Exactly `todo_window` entries, blank-padded when the visible slice is
    /// shorter than the window.
    pub visible: Vec<TodoLine>,
    /// Completed count from the FULL list (counter independence).
    pub done: usize,
    /// Total count from the FULL list.
    pub total: usize,
    /// Index of the current item within `visible`, or `None` when all items
    /// are complete or the list is empty.
    pub current: Option<usize>,
}

/// How an agent's todo list should be rendered — field-state honest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TodoRender {
    /// A known list (possibly empty) rendered as a windowed slice.
    Known(TodoWindow),
    /// The producer supports the field but has no current value.
    Unknown,
    /// The producer does not support the todos field at all (no observation,
    /// or observation health is unsupported).
    Unsupported,
}

/// The agent-name portion of a card header, clipped to the budget so the
/// card keeps an exact width at any name length.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentNamePart {
    pub text: String,
}

/// Resolved card header parts (status glyph/word, optional slot, repo/name,
/// elapsed). All parts except the agent name are fixed-width; the name is the
/// only elastic part and is pre-clipped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkbenchHeader {
    /// Status word (e.g. "WAITING", "WORKING", "READY", "STALE").
    pub status_label: String,
    /// Shortcut slot badge, e.g. `"3"`, when assigned.
    pub shortcut_slot: Option<String>,
    /// `repo/name` label, clipped to the name budget.
    pub repo_name: AgentNamePart,
    /// Turn elapsed label (e.g. `"4m 12s"`), or `"—"` when unknown.
    pub elapsed: String,
}

/// One resolved agent card for the current page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkbenchCard {
    pub agent_id: AgentId,
    pub header: WorkbenchHeader,
    /// The wait reason or current activity line, clipped to the card interior.
    pub need: String,
    pub todos: TodoRender,
    /// Last committed assistant message, clipped, or `None`.
    pub last_message: Option<String>,
    pub bucket: StatusBucket,
}

/// The complete, finite workbench view model for one frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkbenchView {
    pub layout: WorkbenchLayout,
    /// Cards for the current page only, already sorted and windowed.
    pub cards: Vec<WorkbenchCard>,
    /// Per-bucket counts computed BEFORE filtering, so counts stay live.
    /// Index order matches [`StatusBucket::as_index`].
    pub bucket_counts: [usize; 4],
    /// When non-empty, why no cards render (all filters off, or no agents).
    pub empty_reason: Option<String>,
}

/// Status-filter mask: four independent booleans.
///
/// `all_off` yields the empty state per requirement S2. Backed by a fixed
/// array indexed by [`StatusBucket::as_index`] so the four flags stay closed
/// and addressable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StatusFilterMask {
    flags: [bool; 4],
}

impl StatusFilterMask {
    /// The default mask: every bucket enabled.
    #[must_use]
    pub const fn all_on() -> Self {
        Self { flags: [true; 4] }
    }

    /// Whether every bucket is disabled (requirement S2 empty-state trigger).
    #[must_use]
    pub const fn all_off(&self) -> bool {
        !self.flags[0] && !self.flags[1] && !self.flags[2] && !self.flags[3]
    }

    /// Whether `bucket` passes the filter.
    #[must_use]
    pub const fn allows(&self, bucket: StatusBucket) -> bool {
        self.flags[bucket.as_index()]
    }

    /// Enable or disable a single bucket.
    #[must_use]
    pub const fn with(self, bucket: StatusBucket, enabled: bool) -> Self {
        let mut flags = self.flags;
        flags[bucket.as_index()] = enabled;
        Self { flags }
    }

    /// Build a mask that enables exactly one bucket.
    #[must_use]
    pub const fn only(bucket: StatusBucket) -> Self {
        let mut flags = [false; 4];
        flags[bucket.as_index()] = true;
        Self { flags }
    }
}

/// One agent's inputs to the projection. Borrowed; the projection clones only
/// what it places into the view model.
#[derive(Debug, Clone)]
pub struct AgentInput<'a> {
    pub agent: &'a Agent,
    pub git_info: Option<&'a GitRepoInfo>,
    pub observation: Option<&'a AgentObservation>,
}

/// Pure projection inputs that carry no references (useful for tests and for
/// callers that already own the data).
#[derive(Debug, Clone)]
pub struct WorkbenchRequest {
    pub agents: Vec<(Agent, Option<GitRepoInfo>, Option<AgentObservation>)>,
    pub status_filter: StatusFilterMask,
    pub repository_filter: Option<String>,
    pub terminal_width: usize,
    pub terminal_height: usize,
    pub page: usize,
}

/// Build the workbench view model from owned inputs (convenience over the
/// borrowed-reference entry point).
#[must_use]
pub fn build_workbench_view(request: &WorkbenchRequest) -> WorkbenchView {
    let refs: Vec<AgentInput<'_>> = request
        .agents
        .iter()
        .map(|(agent, git, obs)| AgentInput {
            agent,
            git_info: git.as_ref(),
            observation: obs.as_ref(),
        })
        .collect();
    build_workbench_view_ref(
        &refs,
        request.status_filter,
        request.repository_filter.as_deref(),
        request.terminal_width,
        request.terminal_height,
        request.page,
    )
}

/// Build the workbench view model from borrowed agent inputs.
///
/// Pure: performs no I/O and reads no clock. Sorting is stable within a
/// bucket so only a bucket change may reorder an agent.
#[must_use]
pub fn build_workbench_view_ref(
    agents: &[AgentInput<'_>],
    status_filter: StatusFilterMask,
    repository_filter: Option<&str>,
    terminal_width: usize,
    terminal_height: usize,
    page: usize,
) -> WorkbenchView {
    let (bucket_counts, bucketed) = bucket_agents(agents);

    if let Some(empty) = check_empty_state(
        status_filter,
        repository_filter,
        terminal_width,
        terminal_height,
        bucket_counts,
        &bucketed,
    ) {
        return empty;
    }

    let visible = filter_agents(&bucketed, status_filter, repository_filter);
    let horizontal = resolve_horizontal(terminal_width);
    let columns = horizontal.columns;
    let visible_count = visible.len();
    let vertical = resolve_vertical(
        terminal_height,
        visible_count,
        columns,
        longest_visible_todo_list(&visible),
    );

    let sorted = stable_sort(visible);
    let (layout, start, end) = resolve_paging(vertical, horizontal, columns, visible_count, page);
    let cards = build_page_cards(
        &sorted[start..end],
        horizontal.card_width,
        vertical.todo_window,
    );

    WorkbenchView {
        layout,
        cards,
        bucket_counts,
        empty_reason: None,
    }
}

/// Return an empty view when no agents should render, or `None` to continue.
fn check_empty_state(
    status_filter: StatusFilterMask,
    repository_filter: Option<&str>,
    terminal_width: usize,
    terminal_height: usize,
    bucket_counts: [usize; 4],
    bucketed: &[(usize, StatusBucket, &AgentInput<'_>)],
) -> Option<WorkbenchView> {
    if status_filter.all_off() {
        return Some(empty_view(
            "All status filters are off. Enable one to see agents.",
            terminal_width,
            terminal_height,
            bucket_counts,
        ));
    }
    let visible_count = bucketed
        .iter()
        .filter(|(_, bucket, input)| {
            status_filter.allows(*bucket) && repository_matches(input.agent, repository_filter)
        })
        .count();
    if visible_count == 0 {
        return Some(empty_view(
            &empty_reason(repository_filter, bucket_counts.iter().sum::<usize>()),
            terminal_width,
            terminal_height,
            bucket_counts,
        ));
    }
    None
}

/// Whether an agent's repository matches the filter (None or empty = all).
fn repository_matches(agent: &Agent, repository_filter: Option<&str>) -> bool {
    match repository_filter {
        Some(repo) if !repo.is_empty() => agent.repository_id.0 == repo,
        _ => true,
    }
}

/// Build the card view models for one page of agents.
fn build_page_cards(
    page_slice: &[(usize, StatusBucket, &AgentInput<'_>)],
    card_width: usize,
    todo_window: usize,
) -> Vec<WorkbenchCard> {
    page_slice
        .iter()
        .map(|&(_, bucket, input)| build_card(input, bucket, card_width, todo_window))
        .collect()
}

/// Bucket every agent and accumulate unfiltered counts. Preserves incoming
/// order for stable intra-bucket sort.
fn bucket_agents<'a>(
    agents: &'a [AgentInput<'a>],
) -> ([usize; 4], Vec<(usize, StatusBucket, &'a AgentInput<'a>)>) {
    let mut bucket_counts = [0_usize; 4];
    let mut bucketed = Vec::with_capacity(agents.len());
    for (index, input) in agents.iter().enumerate() {
        let resolved = resolve_status(input.agent.status, input.observation);
        let bucket = bucket_for(resolved);
        bucket_counts[bucket.as_index()] += 1;
        bucketed.push((index, bucket, input));
    }
    (bucket_counts, bucketed)
}

/// Apply repository + status filters, preserving order.
fn filter_agents<'a>(
    bucketed: &'a [(usize, StatusBucket, &AgentInput<'a>)],
    status_filter: StatusFilterMask,
    repository_filter: Option<&str>,
) -> Vec<(usize, StatusBucket, &'a AgentInput<'a>)> {
    bucketed
        .iter()
        .copied()
        .filter(|(_, bucket, input)| {
            status_filter.allows(*bucket) && repository_matches(input.agent, repository_filter)
        })
        .collect()
}

/// Reason string when no agents pass the filters.
fn empty_reason(repository_filter: Option<&str>, total_agents: usize) -> String {
    match (repository_filter, total_agents) {
        (Some(_), 0) => "No agents match this repository.".to_string(),
        (_, 0) => "No agents are running.".to_string(),
        _ => "No agents match the current filters.".to_string(),
    }
}

/// Stable sort by bucket, then by incoming index.
fn stable_sort<'a>(
    visible: Vec<(usize, StatusBucket, &'a AgentInput<'a>)>,
) -> Vec<(usize, StatusBucket, &'a AgentInput<'a>)> {
    let mut sorted = visible;
    sorted.sort_by_key(|(index, bucket, _)| (bucket_sort_key(*bucket), *index));
    sorted
}

/// Clamp the page index and resolve the final layout plus the inclusive
/// start/exclusive end indices of the current page slice.
fn resolve_paging(
    vertical: VerticalLayout,
    horizontal: HorizontalLayout,
    columns: usize,
    visible_count: usize,
    page: usize,
) -> (WorkbenchLayout, usize, usize) {
    let rows_visible = vertical.rows_visible;
    let cards_per_page = rows_visible.saturating_mul(columns).max(1);
    let page_count = div_ceil(visible_count, cards_per_page).max(1);
    let clamped_page = page.min(page_count.saturating_sub(1));
    let start = clamped_page.saturating_mul(cards_per_page);
    let end = (start.saturating_add(cards_per_page)).min(visible_count);
    let layout = WorkbenchLayout {
        columns,
        card_width: horizontal.card_width,
        todo_window: vertical.todo_window,
        rows_visible,
        page: clamped_page,
        page_count,
    };
    (layout, start, end)
}

/// Horizontal resolution result.
#[derive(Clone, Copy)]
struct HorizontalLayout {
    columns: usize,
    card_width: usize,
}

/// Resolve columns and card width from terminal width (issue #626 horizontal
/// rule). Backs off columns while any card would fall below the minimum.
fn resolve_horizontal(total_width: usize) -> HorizontalLayout {
    let usable = total_width
        .saturating_sub(SIDEBAR_WIDTH)
        .saturating_sub(CARD_GAP);
    let mut columns = (usable.saturating_add(CARD_GAP)) / (MIN_CARD_WIDTH.saturating_add(CARD_GAP));
    if columns == 0 {
        columns = 1;
    }
    let card_width = recompute_card_width(columns, usable);
    let (columns, card_width) = back_off(columns, card_width, usable);
    HorizontalLayout {
        columns,
        card_width,
    }
}

/// Recompute card width for a given column count and usable width.
fn recompute_card_width(columns: usize, usable: usize) -> usize {
    if columns == 0 {
        return MIN_CARD_WIDTH;
    }
    let gaps = columns.saturating_sub(1).saturating_mul(CARD_GAP);
    let width = usable.saturating_sub(gaps) / columns;
    width.min(MAX_CARD_WIDTH)
}

/// Back off one column at a time while `card_width < MIN_CARD_WIDTH` and more
/// than one column remains.
fn back_off(mut columns: usize, mut card_width: usize, usable: usize) -> (usize, usize) {
    while columns > 1 && card_width < MIN_CARD_WIDTH {
        columns -= 1;
        card_width = recompute_card_width(columns, usable);
    }
    // Guarantee the floor: a single column is always at least MIN_CARD_WIDTH
    // so a card is never narrower than the minimum even on a degenerate width.
    if card_width < MIN_CARD_WIDTH {
        card_width = MIN_CARD_WIDTH;
    }
    (columns, card_width)
}

/// Vertical resolution result.
#[derive(Clone, Copy)]
struct VerticalLayout {
    todo_window: usize,
    rows_visible: usize,
}

/// Resolve todo-window size, visible rows, and page count from terminal
/// height (issue #626 vertical rule). Agents come first; leftover height
/// becomes detail. `W` never exceeds the longest visible list.
fn resolve_vertical(
    terminal_height: usize,
    visible_agents: usize,
    columns: usize,
    longest_visible_list: usize,
) -> VerticalLayout {
    let avail = terminal_height.saturating_sub(SCREEN_CHROME_LINES);
    let rows_needed = div_ceil(visible_agents, columns.max(1));
    let rows_at_min = (avail / (CARD_CHROME_LINES + TODO_WINDOW_MIN + 1)).max(1);

    if rows_needed <= rows_at_min {
        // Everything fits: grow the window with leftover height.
        let grown = avail.checked_div(rows_needed).map_or(0, |row_budget| {
            row_budget
                .saturating_sub(1)
                .saturating_sub(CARD_CHROME_LINES)
        });
        let capped_by_list = grown.min(longest_visible_list);
        let w = clamp(capped_by_list, TODO_WINDOW_MIN, TODO_WINDOW_MAX);
        VerticalLayout {
            todo_window: w,
            rows_visible: rows_needed.max(1),
        }
    } else {
        // Too many agents: page them at the minimum window.
        VerticalLayout {
            todo_window: TODO_WINDOW_MIN,
            rows_visible: rows_at_min,
        }
    }
}

/// Build one card from a single agent input.
fn build_card(
    input: &AgentInput<'_>,
    bucket: StatusBucket,
    card_width: usize,
    todo_window: usize,
) -> WorkbenchCard {
    let interior = card_width;
    let resolved = resolve_status(input.agent.status, input.observation);
    let status_label = bucket_label(bucket, resolved);
    let shortcut_slot = input.agent.shortcut_slot.map(|slot| slot.to_string());
    let repo = input
        .git_info
        .and_then(|g| g.origin_shortform.as_deref())
        .unwrap_or("?");
    let name_budget = name_budget(interior, shortcut_slot.as_deref(), &status_label);
    let repo_name = AgentNamePart {
        text: clip_repo_name(repo, &input.agent.name, name_budget),
    };
    let elapsed = elapsed_label(input.observation, Instant::now());
    let header = WorkbenchHeader {
        status_label: fit_text_to_width(&status_label, interior),
        shortcut_slot,
        repo_name,
        elapsed: fit_text_to_width(&elapsed, interior),
    };
    let need = need_line(resolved, input.observation, interior);
    let todos = render_todos(input.observation, todo_window, interior);
    let last_message = last_message(input.observation, interior);
    WorkbenchCard {
        agent_id: input.agent.id.clone(),
        header,
        need,
        todos,
        last_message,
        bucket,
    }
}

/// The bucket display label (status word shown in the header).
fn bucket_label(bucket: StatusBucket, resolved: ResolvedStatus) -> String {
    match bucket {
        StatusBucket::NeedsYou => match resolved {
            ResolvedStatus::Waiting(reason) => {
                format!(
                    "WAITING — {}",
                    crate::status_precedence::wait_reason_label(reason)
                )
            }
            _ => "NEEDS YOU".to_string(),
        },
        StatusBucket::Working => "WORKING".to_string(),
        StatusBucket::Ready => "READY".to_string(),
        StatusBucket::Stale => match resolved {
            ResolvedStatus::Disconnected => "DISCONNECTED".to_string(),
            ResolvedStatus::Dead => "DEAD".to_string(),
            ResolvedStatus::Connecting => "CONNECTING".to_string(),
            ResolvedStatus::ProtocolError => "PROTOCOL ERROR".to_string(),
            _ => "STALE".to_string(),
        },
    }
}

/// The "need" line: the wait reason when blocked, else the current activity.
fn need_line(
    resolved: ResolvedStatus,
    observation: Option<&AgentObservation>,
    interior: usize,
) -> String {
    let raw = match resolved {
        ResolvedStatus::Waiting(reason) => {
            format!(
                "needs you: {}",
                crate::status_precedence::wait_reason_label(reason)
            )
        }
        _ => activity_line(observation),
    };
    fit_text_to_width(&raw, interior)
}

/// Describe the current activity from the observation (tool/turn/idle).
fn activity_line(observation: Option<&AgentObservation>) -> String {
    let Some(observation) = observation else {
        return "no telemetry".to_string();
    };
    if let FieldState::Supported {
        availability: Availability::Known(tool),
        ..
    } = &observation.tool
    {
        return format!("tool: {}", tool.label.as_str());
    }
    if let FieldState::Supported {
        availability: Availability::Known(activity),
        ..
    } = &observation.activity
    {
        return match activity.state {
            crate::domain::observation::NativeActivityState::Thinking => "thinking".to_string(),
            crate::domain::observation::NativeActivityState::Acting => "acting".to_string(),
            crate::domain::observation::NativeActivityState::Idle => "idle".to_string(),
        };
    }
    "working".to_string()
}

/// Map a resolved status to a status bucket.
fn bucket_for(resolved: ResolvedStatus) -> StatusBucket {
    match resolved {
        ResolvedStatus::Waiting(_) => StatusBucket::NeedsYou,
        ResolvedStatus::Working => StatusBucket::Working,
        ResolvedStatus::Ready => StatusBucket::Ready,
        // Everything else — the terminal states Failed and Ended, plus Stale,
        // Disconnected, Dead, Connecting, Starting, ProcessWaiting,
        // ProcessPaused, TelemetryUnsupported, ProtocolError and Unknown.
        //
        // Failed and Ended in particular must NOT sit in Working: a finished or
        // broken agent is not doing anything, and counting it as Working both
        // inflates that bucket and buries it among agents that really are busy.
        // They land here so they never fold into Ready either.
        _ => StatusBucket::Stale,
    }
}

/// Sort priority for a bucket (lower sorts first).
fn bucket_sort_key(bucket: StatusBucket) -> u8 {
    match bucket {
        StatusBucket::NeedsYou => 0,
        StatusBucket::Working => 1,
        StatusBucket::Ready => 2,
        StatusBucket::Stale => 3,
    }
}

/// Resolve todo rendering with field-state honesty.
fn render_todos(
    observation: Option<&AgentObservation>,
    window: usize,
    interior: usize,
) -> TodoRender {
    let Some(observation) = observation else {
        return TodoRender::Unsupported;
    };
    match &observation.todos {
        FieldState::Unsupported => TodoRender::Unsupported,
        FieldState::Supported {
            availability: Availability::Unknown,
            ..
        } => TodoRender::Unknown,
        FieldState::Supported {
            availability: Availability::Known(list),
            ..
        }
        | FieldState::Supported {
            availability:
                Availability::Degraded {
                    last_value: list, ..
                },
            ..
        } => TodoRender::Known(window_todos(list, window, interior)),
    }
}

/// Build the windowed todo slice with counter independence and blank padding.
fn window_todos(list: &TodoList, window: usize, interior: usize) -> TodoWindow {
    let items = &list.items;
    let total = items.len();
    let done = items.iter().filter(|t| t.completed).count();
    let (start, current_global) = todo_window_start(items, window);
    let current_visible = current_global.and_then(|g| {
        if g >= start && g < start.saturating_add(window) {
            Some(g.saturating_sub(start))
        } else {
            None
        }
    });
    let mut visible = Vec::with_capacity(window);
    for slot in 0..window {
        let global = start.saturating_add(slot);
        if global < total {
            let item = &items[global];
            let is_current = current_visible == Some(slot);
            let prefix = if is_current { "▸" } else { " " };
            let marker = if item.completed { "x" } else { " " };
            let line = format!("{prefix}[{marker}] {}", item.text.as_str());
            visible.push(TodoLine {
                text: fit_text_to_width(&line, interior),
                is_current,
                is_blank: false,
            });
        } else {
            visible.push(TodoLine {
                text: String::new(),
                is_current: false,
                is_blank: true,
            });
        }
    }
    TodoWindow {
        visible,
        done,
        total,
        current: current_visible,
    }
}

/// Compute the window start index and the current (first incomplete) item
/// global index, per the issue's TODO WINDOW RULE.
fn todo_window_start(items: &[TodoItem], window: usize) -> (usize, Option<usize>) {
    let len = items.len();
    let first_open = items.iter().position(|t| !t.completed);
    let current = first_open;
    let start = match first_open {
        None => len.saturating_sub(window),
        Some(open) => open.saturating_sub(1).min(len.saturating_sub(window)),
    };
    (start, current)
}

/// Extract the last committed assistant message, clipped, or `None`.
fn last_message(observation: Option<&AgentObservation>, interior: usize) -> Option<String> {
    let observation = observation?;
    let message = match &observation.last_message {
        FieldState::Supported {
            availability: Availability::Known(message),
            ..
        }
        | FieldState::Supported {
            availability:
                Availability::Degraded {
                    last_value: message,
                    ..
                },
            ..
        } => Some(message),
        _ => None,
    }?;
    Some(fit_text_to_width(message.content.as_str(), interior))
}

/// The turn-elapsed label, derived from the observation's turn anchor. Passes
/// `now` from the caller; the projection itself does not read a clock.
fn elapsed_label(observation: Option<&AgentObservation>, now: Instant) -> String {
    let Some(observation) = observation else {
        return "—".to_string();
    };
    let anchor = match &observation.turn {
        FieldState::Supported {
            availability: Availability::Known(Some(turn)),
            ..
        } => Some(turn.elapsed_ms),
        _ => None,
    };
    let Some(anchor) = anchor else {
        return "—".to_string();
    };
    let local_elapsed = observation.turn_observed_at.map_or(0, |observed| {
        u64::try_from(now.saturating_duration_since(observed).as_millis()).unwrap_or(u64::MAX)
    });
    format_elapsed(anchor.saturating_add(local_elapsed))
}

/// Format elapsed milliseconds as `Xm Ys` or `Ys`.
fn format_elapsed(elapsed_ms: u64) -> String {
    let seconds = elapsed_ms / 1_000;
    let minutes = seconds / 60;
    let remainder = seconds % 60;
    if minutes == 0 {
        format!("{seconds}s")
    } else {
        format!("{minutes}m {remainder}s")
    }
}

/// Budget remaining for the `repo/name` part after the fixed header parts.
fn name_budget(interior: usize, slot: Option<&str>, status_label: &str) -> usize {
    // Header layout: "<STATUS> [slot] repo/name  elapsed"
    // Fixed overhead: status word, a space, optional "[slot] " (4 + slot len),
    // two spaces before elapsed.
    // Measure in terminal cells, not scalar values: `fit_text_to_width` clips
    // by display width, so counting chars here would over-credit the budget for
    // wide glyphs (CJK, emoji) and let the header overflow its card.
    let mut used = UnicodeWidthStr::width(status_label) + 1; // status + space
    if let Some(slot) = slot {
        used += 2 + UnicodeWidthStr::width(slot) + 1; // "[N] "
    }
    used += 2; // separator before elapsed (we reserve room for "  …")
    interior.saturating_sub(used)
}

/// Clip `repo/name` to fit the name budget, truncating the name with an
/// ellipsis when it overflows.
fn clip_repo_name(repo: &str, name: &str, budget: usize) -> String {
    if budget == 0 {
        return String::new();
    }
    let full = format!("{repo}/{name}");
    fit_text_to_width(&full, budget)
}

/// The longest todo list among the visible agents (for the window cap).
fn longest_visible_todo_list(visible: &[(usize, StatusBucket, &AgentInput<'_>)]) -> usize {
    visible
        .iter()
        .filter_map(|(_, _, input)| input.observation)
        .filter_map(|obs| match &obs.todos {
            FieldState::Supported {
                availability: Availability::Known(list),
                ..
            }
            | FieldState::Supported {
                availability:
                    Availability::Degraded {
                        last_value: list, ..
                    },
                ..
            } => Some(list.items.len()),
            _ => None,
        })
        .max()
        .unwrap_or(0)
}

/// Construct an empty view (no cards) with a reason and the live bucket counts.
fn empty_view(
    reason: &str,
    terminal_width: usize,
    terminal_height: usize,
    bucket_counts: [usize; 4],
) -> WorkbenchView {
    let horizontal = resolve_horizontal(terminal_width);
    let vertical = resolve_vertical(terminal_height, 0, horizontal.columns, 0);
    WorkbenchView {
        layout: WorkbenchLayout {
            columns: horizontal.columns,
            card_width: horizontal.card_width,
            todo_window: vertical.todo_window,
            rows_visible: 0,
            page: 0,
            page_count: 1,
        },
        cards: Vec::new(),
        bucket_counts,
        empty_reason: Some(reason.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Small numeric helpers
// ---------------------------------------------------------------------------

/// Ceiling division: `ceil(a / b)`.
fn div_ceil(a: usize, b: usize) -> usize {
    if b == 0 {
        return 0;
    }
    a.div_ceil(b)
}

/// Clamp `value` into `[lo, hi]`.
fn clamp(value: usize, lo: usize, hi: usize) -> usize {
    value.clamp(lo, hi)
}
