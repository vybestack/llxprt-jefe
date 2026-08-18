//! Closed host-control vocabulary and identity-free factory dispatch.
//!
//! Public package declarations select one of the nine [`ControlKind`] values.
//! The host additionally owns a private terminal control that cannot be named by
//! package manifests or provider snapshots.

use crate::domain::plugin::ModelKind;
use crate::domain::{Id, TypedMap, TypedValue};
use crate::list_viewport::fit_text_to_width;
use crate::runtime::provider::protocol::{
    BodyKind, DiffLineOrigin, EmptyBody, ErrorBody, FormBody, PanelBody, PanelEvent, PanelSnapshot,
    ProgressBody, StatusBody, StructuredDiffBody, StructuredDiffFile, TreeBody, TreeNode,
};
use crate::text_wrap::wrap_text;

mod intent;

use intent::{display_value, public_control_intent, push_wrapped};

/// The complete public control vocabulary shared by every screen origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ControlKind {
    List,
    Tree,
    Detail,
    StructuredDiff,
    Form,
    Status,
    Progress,
    Empty,
    Error,
}

impl ControlKind {
    /// Every public control in canonical wire order.
    pub const ALL: [Self; 9] = [
        Self::List,
        Self::Tree,
        Self::Detail,
        Self::StructuredDiff,
        Self::Form,
        Self::Status,
        Self::Progress,
        Self::Empty,
        Self::Error,
    ];

    /// Exact lower-kebab-case public spelling.
    #[must_use]
    pub const fn as_wire(self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Tree => "tree",
            Self::Detail => "detail",
            Self::StructuredDiff => "structured-diff",
            Self::Form => "form",
            Self::Status => "status",
            Self::Progress => "progress",
            Self::Empty => "empty",
            Self::Error => "error",
        }
    }

    /// Parse one exact public spelling.
    #[must_use]
    pub fn from_wire(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_wire() == value)
    }
}

impl From<ModelKind> for ControlKind {
    fn from(value: ModelKind) -> Self {
        match value {
            ModelKind::List => Self::List,
            ModelKind::Tree => Self::Tree,
            ModelKind::Detail => Self::Detail,
            ModelKind::StructuredDiff => Self::StructuredDiff,
            ModelKind::Form => Self::Form,
            ModelKind::Status => Self::Status,
            ModelKind::Progress => Self::Progress,
            ModelKind::Empty => Self::Empty,
            ModelKind::Error => Self::Error,
        }
    }
}

impl From<BodyKind> for ControlKind {
    fn from(value: BodyKind) -> Self {
        match value {
            BodyKind::List => Self::List,
            BodyKind::Tree => Self::Tree,
            BodyKind::Detail => Self::Detail,
            BodyKind::StructuredDiff => Self::StructuredDiff,
            BodyKind::Form => Self::Form,
            BodyKind::Status => Self::Status,
            BodyKind::Progress => Self::Progress,
            BodyKind::Empty => Self::Empty,
            BodyKind::Error => Self::Error,
        }
    }
}

/// One semantic target rendered on a host-control row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelHitTarget {
    /// One selectable List item.
    ListItem(Id),
    /// One visible Tree node.
    TreeNode(Id),
    /// One StructuredDiff file.
    DiffFile(Id),
    /// One editable Form field.
    Field(Id),
    /// One declared action affordance.
    Action(Id),
    /// The enabled Form submit affordance.
    Submit,
    /// The enabled List pagination affordance.
    PageRequested,
    /// The enabled Error retry affordance.
    Retry,
    /// The enabled Progress cancel affordance.
    Cancel,
    /// One enabled Detail link.
    Link(Id),
    /// A rendered row with no available semantic action.
    Unavailable,
}

/// Internal dispatch key. `Terminal` cannot cross the public control boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HostControlKind {
    Public(ControlKind),
    Terminal,
}

/// Unforgeable outside this crate and required for host-only terminal dispatch.
pub(crate) struct HostTerminalCapability {
    _private: (),
}

#[cfg(test)]
pub(crate) const fn host_terminal_capability() -> HostTerminalCapability {
    HostTerminalCapability { _private: () }
}

mod sealed {
    pub trait Sealed {}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HostControlRow {
    pub(crate) text: String,
    pub(crate) target: Option<PanelHitTarget>,
}

impl HostControlRow {
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

/// Closed actions interpreted by one host control factory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlAction {
    /// Move selection to the previous visible target.
    Previous,
    /// Move selection to the next visible target.
    Next,
    /// Activate the currently selected target.
    Activate,
    /// Select the exact semantic target.
    Select(Id),
    /// Invoke the exact action affordance.
    Action(Id),
    /// Invoke the focused action affordance.
    FocusedAction,
    /// Submit the current Form values.
    Submit,
    /// Request the next List page.
    PageNext,
    /// Retry a retryable Error model.
    Retry,
    /// Cancel a cancellable Progress model.
    Cancel,
    /// Open the exact Detail link.
    Link(Id),
    /// Open the focused Detail link.
    FocusedLink,
}

/// Pure result of interpreting an action against an accepted provider model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlIntent {
    /// Emit one typed provider-panel event.
    Event(PanelEvent),
    /// Move host-local viewport state by a signed row delta.
    Scroll(i8),
    /// Apply no state transition or provider event.
    None,
}

#[derive(Clone, Copy)]
pub(crate) struct ProjectionInput<'a> {
    snapshot: &'a PanelSnapshot,
    selected_id: Option<&'a Id>,
    form_draft: Option<&'a TypedMap>,
    width: usize,
}

#[derive(Clone)]
pub(crate) struct IntentInput<'a> {
    snapshot: &'a PanelSnapshot,
    selected_id: Option<&'a Id>,
    focus_target: Option<&'a Id>,
    form_draft: Option<&'a TypedMap>,
    action: ControlAction,
}

/// Sealed host factory selected solely by [`HostControlKind`].
pub(crate) trait HostControlFactory: sealed::Sealed + Sync {
    fn kind(&self) -> HostControlKind;
    fn project(&self, input: ProjectionInput<'_>) -> Vec<HostControlRow>;
    fn intent(&self, input: IntentInput<'_>) -> ControlIntent;
}

struct PublicFactory {
    kind: ControlKind,
    project: fn(ProjectionInput<'_>) -> Vec<HostControlRow>,
}

impl sealed::Sealed for PublicFactory {}

impl HostControlFactory for PublicFactory {
    fn kind(&self) -> HostControlKind {
        HostControlKind::Public(self.kind)
    }

    fn project(&self, input: ProjectionInput<'_>) -> Vec<HostControlRow> {
        (self.project)(input)
    }

    fn intent(&self, input: IntentInput<'_>) -> ControlIntent {
        public_control_intent(self.kind, input)
    }
}

struct TerminalFactory;

impl sealed::Sealed for TerminalFactory {}

impl HostControlFactory for TerminalFactory {
    fn kind(&self) -> HostControlKind {
        HostControlKind::Terminal
    }

    fn project(&self, _input: ProjectionInput<'_>) -> Vec<HostControlRow> {
        unreachable!("provider snapshots cannot select the host-only terminal control")
    }

    fn intent(&self, _input: IntentInput<'_>) -> ControlIntent {
        ControlIntent::None
    }
}

static LIST: PublicFactory = PublicFactory {
    kind: ControlKind::List,
    project: project_list,
};
static TREE: PublicFactory = PublicFactory {
    kind: ControlKind::Tree,
    project: project_tree,
};
static DETAIL: PublicFactory = PublicFactory {
    kind: ControlKind::Detail,
    project: project_detail,
};
static STRUCTURED_DIFF: PublicFactory = PublicFactory {
    kind: ControlKind::StructuredDiff,
    project: project_structured_diff,
};
static FORM: PublicFactory = PublicFactory {
    kind: ControlKind::Form,
    project: project_form,
};
static STATUS: PublicFactory = PublicFactory {
    kind: ControlKind::Status,
    project: project_status,
};
static PROGRESS: PublicFactory = PublicFactory {
    kind: ControlKind::Progress,
    project: project_progress,
};
static EMPTY: PublicFactory = PublicFactory {
    kind: ControlKind::Empty,
    project: project_empty,
};
static ERROR: PublicFactory = PublicFactory {
    kind: ControlKind::Error,
    project: project_error,
};
static TERMINAL: TerminalFactory = TerminalFactory;

/// Sole host-control dispatch. The exhaustive match intentionally has no fallback.
pub(crate) fn host_control_factory(
    kind: HostControlKind,
    terminal_capability: Option<&HostTerminalCapability>,
) -> Option<&'static dyn HostControlFactory> {
    match kind {
        HostControlKind::Public(ControlKind::List) => Some(&LIST),
        HostControlKind::Public(ControlKind::Tree) => Some(&TREE),
        HostControlKind::Public(ControlKind::Detail) => Some(&DETAIL),
        HostControlKind::Public(ControlKind::StructuredDiff) => Some(&STRUCTURED_DIFF),
        HostControlKind::Public(ControlKind::Form) => Some(&FORM),
        HostControlKind::Public(ControlKind::Status) => Some(&STATUS),
        HostControlKind::Public(ControlKind::Progress) => Some(&PROGRESS),
        HostControlKind::Public(ControlKind::Empty) => Some(&EMPTY),
        HostControlKind::Public(ControlKind::Error) => Some(&ERROR),
        HostControlKind::Terminal => {
            terminal_capability.map(|_| &TERMINAL as &dyn HostControlFactory)
        }
    }
}

/// Project one validated provider body through the sole control factory boundary.
pub(crate) fn project_control(
    snapshot: &PanelSnapshot,
    selected_id: Option<&Id>,
    form_draft: Option<&TypedMap>,
    width: usize,
) -> Vec<HostControlRow> {
    let kind = HostControlKind::Public(ControlKind::from(snapshot.kind));
    let Some(factory) = host_control_factory(kind, None) else {
        unreachable!("public control dispatch does not require terminal capability")
    };
    debug_assert_eq!(factory.kind(), kind);
    factory.project(ProjectionInput {
        snapshot,
        selected_id,
        form_draft,
        width,
    })
}

/// Interpret one navigation action through the same sole control factory boundary.
#[must_use]
pub fn control_intent(
    snapshot: &PanelSnapshot,
    selected_id: Option<&Id>,
    focus_target: Option<&Id>,
    form_draft: Option<&TypedMap>,
    action: ControlAction,
) -> ControlIntent {
    let kind = HostControlKind::Public(ControlKind::from(snapshot.kind));
    let Some(factory) = host_control_factory(kind, None) else {
        unreachable!("public control dispatch does not require terminal capability")
    };
    debug_assert_eq!(factory.kind(), kind);
    factory.intent(IntentInput {
        snapshot,
        selected_id,
        focus_target,
        form_draft,
        action,
    })
}
/// Resolve the effective selectable identity for a validated control model.
#[must_use]
pub fn selected_control_id<'a>(
    snapshot: &'a PanelSnapshot,
    local: Option<&'a Id>,
) -> Option<&'a Id> {
    match &snapshot.body {
        PanelBody::List(body) => local
            .filter(|selected| body.items.iter().any(|item| &item.id == *selected))
            .or(body.selected_id.as_ref())
            .or_else(|| body.items.first().map(|item| &item.id)),
        PanelBody::Tree(body) => selected_tree_id(body, local),
        PanelBody::StructuredDiff(body) => local
            .filter(|selected| body.files.iter().any(|file| &file.id == *selected))
            .or(body.selected_file_id.as_ref())
            .or_else(|| body.files.first().map(|file| &file.id)),
        PanelBody::Detail(_)
        | PanelBody::Form(_)
        | PanelBody::Status(_)
        | PanelBody::Progress(_)
        | PanelBody::Empty(_)
        | PanelBody::Error(_) => None,
    }
}

fn project_list(input: ProjectionInput<'_>) -> Vec<HostControlRow> {
    let PanelBody::List(body) = &input.snapshot.body else {
        unreachable!("list factory received another body kind")
    };
    let mut rows = Vec::new();
    let selected = input
        .selected_id
        .filter(|selected| body.items.iter().any(|item| &item.id == *selected))
        .or_else(|| {
            body.selected_id
                .as_ref()
                .filter(|selected| body.items.iter().any(|item| &item.id == *selected))
        })
        .or_else(|| body.items.first().map(|item| &item.id));
    for item in &body.items {
        let item_target = PanelHitTarget::ListItem(item.id.clone());
        let marker = if selected == Some(&item.id) {
            ">> "
        } else {
            "   "
        };
        let status_suffix = item
            .status
            .as_deref()
            .map_or(String::new(), |value| format!(" [{value}]"));
        push_wrapped(
            &mut rows,
            &format!("{marker}{}{status_suffix}", item.label),
            input.width,
            Some(item_target.clone()),
        );
        if let Some(description) = &item.description {
            push_wrapped(
                &mut rows,
                &format!("   {description}"),
                input.width,
                Some(item_target.clone()),
            );
        }
        for action in &item.actions {
            let target = action_target(
                input.snapshot,
                action,
                PanelHitTarget::Action(action.clone()),
            );
            push_wrapped(
                &mut rows,
                &format!("   actions: {action}"),
                input.width,
                target,
            );
        }
    }
    if body.next_page_token.is_some() {
        rows.push(HostControlRow::targeted(
            "more results available",
            PanelHitTarget::PageRequested,
        ));
    }
    rows
}

fn project_tree(input: ProjectionInput<'_>) -> Vec<HostControlRow> {
    let PanelBody::Tree(body) = &input.snapshot.body else {
        unreachable!("tree factory received another body kind")
    };
    let selected = selected_tree_id(body, input.selected_id);
    let mut rows = Vec::new();
    for node in visible_tree_nodes(body) {
        let marker = if selected == Some(&node.id) {
            "> "
        } else {
            "  "
        };
        let expansion = if node.expandable {
            if node.expanded { "▾ " } else { "▸ " }
        } else {
            "  "
        };
        let depth = usize::try_from(node.depth).unwrap_or(usize::MAX);
        let indent = "  ".repeat(depth.min(input.width));
        rows.push(HostControlRow::targeted(
            fit_text_to_width(
                &format!("{marker}{indent}{expansion}{}", node.label),
                input.width,
            ),
            PanelHitTarget::TreeNode(node.id.clone()),
        ));
    }
    rows
}

fn visible_tree_nodes(body: &TreeBody) -> Vec<&TreeNode> {
    let mut hidden_below = None;
    body.nodes
        .iter()
        .filter(|node| {
            if hidden_below.is_some_and(|depth| node.depth > depth) {
                return false;
            }
            hidden_below = None;
            if node.expandable && !node.expanded {
                hidden_below = Some(node.depth);
            }
            true
        })
        .collect()
}

fn selected_tree_id<'a>(body: &'a TreeBody, local: Option<&'a Id>) -> Option<&'a Id> {
    let visible = visible_tree_nodes(body);
    local
        .filter(|selected| visible.iter().any(|node| &node.id == *selected))
        .or_else(|| {
            body.selected_id
                .as_ref()
                .filter(|selected| visible.iter().any(|node| &node.id == *selected))
        })
        .or_else(|| visible.first().map(|node| &node.id))
}

fn project_detail(input: ProjectionInput<'_>) -> Vec<HostControlRow> {
    let PanelBody::Detail(body) = &input.snapshot.body else {
        unreachable!("detail factory received another body kind")
    };
    let mut rows = Vec::new();
    push_wrapped(&mut rows, &body.document, input.width, None);
    for row in &body.metadata {
        push_wrapped(
            &mut rows,
            &format!("{}: {}", row.label, row.value),
            input.width,
            None,
        );
    }
    for action in &body.actions {
        let target = action_target(input.snapshot, action, PanelHitTarget::Link(action.clone()));
        push_wrapped(
            &mut rows,
            &format!("actions: {action}"),
            input.width,
            target,
        );
    }
    rows
}

fn project_structured_diff(input: ProjectionInput<'_>) -> Vec<HostControlRow> {
    let PanelBody::StructuredDiff(body) = &input.snapshot.body else {
        unreachable!("structured-diff factory received another body kind")
    };
    let selected = selected_diff_file_id(body, input.selected_id);
    let mut rows = Vec::new();
    for file in &body.files {
        let marker = if selected == Some(&file.id) {
            ">> "
        } else {
            "   "
        };
        let binary = if file.binary { " [binary]" } else { "" };
        rows.push(HostControlRow::targeted(
            fit_text_to_width(
                &format!("{marker}{}{binary}", diff_file_name(file)),
                input.width,
            ),
            PanelHitTarget::DiffFile(file.id.clone()),
        ));
        for hunk in &file.hunks {
            rows.push(HostControlRow::plain(fit_text_to_width(
                &hunk.header,
                input.width,
            )));
            for line in &hunk.lines {
                let prefix = match line.origin {
                    DiffLineOrigin::Context => ' ',
                    DiffLineOrigin::Added => '+',
                    DiffLineOrigin::Removed => '-',
                };
                let suffix = if line.no_newline { " [no newline]" } else { "" };
                rows.push(HostControlRow::plain(fit_text_to_width(
                    &format!("{prefix}{}{suffix}", line.content),
                    input.width,
                )));
            }
        }
    }
    rows
}

fn selected_diff_file_id<'a>(
    body: &'a StructuredDiffBody,
    local: Option<&'a Id>,
) -> Option<&'a Id> {
    local
        .filter(|selected| body.files.iter().any(|file| &file.id == *selected))
        .or_else(|| {
            body.selected_file_id
                .as_ref()
                .filter(|selected| body.files.iter().any(|file| &file.id == *selected))
        })
        .or_else(|| body.files.first().map(|file| &file.id))
}

fn diff_file_name(file: &StructuredDiffFile) -> String {
    match (&file.old_path, &file.new_path) {
        (Some(old), Some(new)) if old != new => format!("{old} -> {new}"),
        (Some(path), _) | (_, Some(path)) => path.clone(),
        (None, None) => unreachable!("validated diff file has at least one path"),
    }
}

fn project_form(input: ProjectionInput<'_>) -> Vec<HostControlRow> {
    let PanelBody::Form(body) = &input.snapshot.body else {
        unreachable!("form factory received another body kind")
    };
    let mut rows = Vec::new();
    for field in &body.fields {
        let value = input
            .form_draft
            .and_then(|draft| draft.get(field.id()))
            .or_else(|| body.values.get(field.id()));
        let raw = value.map_or_else(
            || format!("{}: _", field.label()),
            |value| format!("{}: {}", field.label(), display_value(value)),
        );
        push_wrapped(
            &mut rows,
            &raw,
            input.width,
            Some(PanelHitTarget::Field(field.id().clone())),
        );
    }
    for error in &body.field_errors {
        push_wrapped(
            &mut rows,
            &format!("{}: {}", error.field_id, error.message),
            input.width,
            None,
        );
    }
    let target = input
        .snapshot
        .action_affordances
        .iter()
        .find(|affordance| affordance.action_id == body.submit_action)
        .map(|affordance| {
            if affordance.enabled {
                PanelHitTarget::Submit
            } else {
                PanelHitTarget::Unavailable
            }
        });
    push_wrapped(
        &mut rows,
        &format!("submit: {}", body.submit_action.as_str()),
        input.width,
        target,
    );
    rows
}

fn project_status(input: ProjectionInput<'_>) -> Vec<HostControlRow> {
    let PanelBody::Status(body) = &input.snapshot.body else {
        unreachable!("status factory received another body kind")
    };
    status_rows(body)
}

fn status_rows(body: &StatusBody) -> Vec<HostControlRow> {
    body.rows
        .iter()
        .map(|row| {
            HostControlRow::plain(format!(
                "[{}] {}: {}",
                row.state.as_str(),
                row.label,
                row.value
            ))
        })
        .collect()
}

fn project_progress(input: ProjectionInput<'_>) -> Vec<HostControlRow> {
    let PanelBody::Progress(body) = &input.snapshot.body else {
        unreachable!("progress factory received another body kind")
    };
    vec![progress_row(body)]
}

fn progress_row(body: &ProgressBody) -> HostControlRow {
    let progress = match (body.completed, body.total) {
        (Some(completed), Some(total)) => format!("{} {completed}/{total}", body.message),
        _ => body.message.clone(),
    };
    if body.cancellable {
        HostControlRow::targeted(format!("{progress} [Cancel]"), PanelHitTarget::Cancel)
    } else {
        HostControlRow::plain(progress)
    }
}

fn project_empty(input: ProjectionInput<'_>) -> Vec<HostControlRow> {
    let PanelBody::Empty(body) = &input.snapshot.body else {
        unreachable!("empty factory received another body kind")
    };
    vec![empty_row(input.snapshot, body)]
}

fn empty_row(snapshot: &PanelSnapshot, body: &EmptyBody) -> HostControlRow {
    let Some(action) = &body.action else {
        return HostControlRow::plain(body.message.clone());
    };
    let text = format!("{} [{action}]", body.message);
    match action_target(snapshot, action, PanelHitTarget::Action(action.clone())) {
        Some(target) => HostControlRow::targeted(text, target),
        None => HostControlRow::plain(text),
    }
}

fn project_error(input: ProjectionInput<'_>) -> Vec<HostControlRow> {
    let PanelBody::Error(body) = &input.snapshot.body else {
        unreachable!("error factory received another body kind")
    };
    vec![error_row(input.snapshot, body)]
}

fn error_row(snapshot: &PanelSnapshot, body: &ErrorBody) -> HostControlRow {
    let Some(action) = &body.retry_action else {
        return HostControlRow::plain(format!("{} {}", body.code, body.message));
    };
    let text = format!("{} {} [Retry: {action}]", body.code, body.message);
    let target = if body.retryable {
        action_target(snapshot, action, PanelHitTarget::Retry)
    } else {
        None
    };
    match target {
        Some(target) => HostControlRow::targeted(text, target),
        None => HostControlRow::plain(text),
    }
}

fn action_target(
    snapshot: &PanelSnapshot,
    id: &Id,
    enabled_target: PanelHitTarget,
) -> Option<PanelHitTarget> {
    snapshot
        .action_affordances
        .iter()
        .find(|affordance| &affordance.id == id)
        .map(|affordance| {
            if affordance.enabled {
                enabled_target
            } else {
                PanelHitTarget::Unavailable
            }
        })
}
