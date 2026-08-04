//! The layout tree editor's state and pure transitions (issue #388, CW-08).
//!
//! One screen's layout is edited as a whole tree, node by node, and only ever
//! leaves here as one complete [`LayoutNode`]. That is the point of the module:
//! a half-typed size or a split with one child is a normal moment in an edit,
//! and it must not be able to reach the draft, where it would become a document
//! nothing can compose.
//!
//! Two rules do the work:
//!
//! - a node dialog holds text, not a tree, so an unfinished edit has nowhere to
//!   escape to; and
//! - a structural change is offered to the descriptor validator before it is
//!   accepted, so the tree in hand is always one the screen could actually be
//!   built from.
//!
//! Nothing here validates a descriptor itself. The refusal reported is
//! [`crate::workbench::validate::validate_descriptor`]'s own.

use crate::domain::Id;
use crate::workbench::descriptor::{Axis, LayoutChild, LayoutNode, ScreenDescriptor, Size};
use crate::workbench::ids::{MAX_SPLIT_CHILDREN, MIN_SPLIT_CHILDREN, PanelId};
use crate::workbench::validate::validate_descriptor;

#[cfg(test)]
#[path = "layout_editor_tests.rs"]
mod layout_editor_tests;

/// One node's position in the tree, as the child indices leading to it.
///
/// The root is the empty path. Using the route rather than a reference is what
/// lets a selection survive an edit that rebuilds the tree it points into.
pub type NodePath = Vec<usize>;

/// The working state of one screen's layout edit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutEditorState {
    /// The screen being edited.
    pub screen_id: Id,
    /// The complete tree as it currently stands.
    pub tree: LayoutNode,
    /// Which node the user is on.
    pub selected: NodePath,
    /// The node dialog, while one is open.
    pub dialog: Option<NodeDialog>,
    /// Why the last structural change was refused, when it was.
    pub notice: Option<String>,
}

/// What a node dialog is collecting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeDialogKind {
    /// Add a leaf for one of the screen's unplaced panels.
    AddLeaf,
    /// Edit the selected child's allocation.
    EditChild,
}

/// One node dialog: text the user is typing, and why it does not parse yet.
///
/// The fields are text because that is what a half-finished edit is. They
/// become a [`LayoutChild`] only when every one of them parses, which is what
/// keeps an invalid intermediate local to this dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeDialog {
    /// What this dialog is collecting.
    pub kind: NodeDialogKind,
    /// Which of the dialog's fields has focus.
    pub field: usize,
    /// The panel a leaf would place, chosen from the screen's own panels.
    pub panel_choice: usize,
    /// `fixed` or `weight`.
    pub size_kind: SizeKind,
    /// The size extent, as typed.
    pub size: String,
    /// The minimum extent, as typed.
    pub min: String,
    /// The maximum extent, as typed; empty means unbounded.
    pub max: String,
    /// Whether the resolver may hide this child.
    pub collapsible: bool,
    /// The collapse order key, as typed; empty means none.
    pub collapse_priority: String,
    /// Why the dialog cannot be applied as it stands.
    pub error: Option<String>,
}

/// Which way a child claims cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeKind {
    /// Exactly this many cells.
    Fixed,
    /// A share of what is left.
    Weight,
}

/// The dialog's fields, in the order Tab moves through them.
pub const DIALOG_FIELDS: usize = 6;

impl NodeDialog {
    /// A dialog collecting a new leaf.
    #[must_use]
    pub fn adding() -> Self {
        Self {
            kind: NodeDialogKind::AddLeaf,
            field: 0,
            panel_choice: 0,
            size_kind: SizeKind::Weight,
            size: "1".to_owned(),
            min: "1".to_owned(),
            max: String::new(),
            collapsible: false,
            collapse_priority: String::new(),
            error: None,
        }
    }

    /// A dialog editing one existing child.
    #[must_use]
    pub fn editing(child: &LayoutChild) -> Self {
        Self {
            kind: NodeDialogKind::EditChild,
            field: 0,
            panel_choice: 0,
            size_kind: match child.size {
                Size::Fixed(_) => SizeKind::Fixed,
                Size::Weight(_) => SizeKind::Weight,
            },
            size: match child.size {
                Size::Fixed(cells) => cells.to_string(),
                Size::Weight(share) => share.to_string(),
            },
            min: child.min.to_string(),
            max: child.max.map(|max| max.to_string()).unwrap_or_default(),
            collapsible: child.collapsible,
            collapse_priority: child
                .collapse_priority
                .map(|priority| priority.to_string())
                .unwrap_or_default(),
            error: None,
        }
    }

    /// Move focus to the next field, wrapping.
    pub const fn next_field(&mut self) {
        self.field = (self.field + 1) % DIALOG_FIELDS;
    }

    /// Type one character into the focused field.
    pub fn push(&mut self, character: char) {
        match self.field {
            1 => self.size.push(character),
            2 => self.min.push(character),
            3 => self.max.push(character),
            5 => self.collapse_priority.push(character),
            // The size kind and the collapse flag are chosen, not typed.
            _ => return,
        }
        self.error = None;
    }

    /// Delete one character from the focused field.
    pub fn backspace(&mut self) {
        match self.field {
            1 => self.size.pop(),
            2 => self.min.pop(),
            3 => self.max.pop(),
            5 => self.collapse_priority.pop(),
            _ => None,
        };
        self.error = None;
    }

    /// Flip whichever of the chosen fields has focus.
    pub fn toggle(&mut self) {
        match self.field {
            0 => {
                self.size_kind = match self.size_kind {
                    SizeKind::Fixed => SizeKind::Weight,
                    SizeKind::Weight => SizeKind::Fixed,
                };
            }
            4 => self.collapsible = !self.collapsible,
            _ => {}
        }
        self.error = None;
    }

    /// The child this dialog describes, or why it does not describe one yet.
    ///
    /// # Errors
    ///
    /// Returns the first field that does not parse, named.
    pub fn child(&self, node: LayoutNode) -> Result<LayoutChild, String> {
        let allocation = self.allocation()?;
        Ok(LayoutChild {
            node,
            size: allocation.size,
            min: allocation.min,
            max: allocation.max,
            collapsible: self.collapsible,
            collapse_priority: allocation.collapse_priority,
        })
    }

    /// Everything the dialog's fields say, once every one of them parses.
    ///
    /// The fields are checked before anything else a dialog needs, so a user
    /// who mistyped a size is told about the size rather than about whatever
    /// the dialog would have gone on to discover.
    ///
    /// # Errors
    ///
    /// Returns the first field that does not parse, named.
    pub fn allocation(&self) -> Result<Allocation, String> {
        let extent = parse_extent(&self.size, "size")?;
        let size = match self.size_kind {
            SizeKind::Fixed => Size::Fixed(nonzero(extent, "size")?),
            SizeKind::Weight => Size::Weight(nonzero(extent, "size")?),
        };
        let min = parse_extent(&self.min, "min")?;
        let max = parse_optional_extent(&self.max, "max")?;
        if let Some(max) = max
            && max < min
        {
            return Err("max must not be less than min".to_owned());
        }
        let collapse_priority = parse_optional_priority(&self.collapse_priority)?;
        if self.collapsible && collapse_priority.is_none() {
            return Err("a collapsible child needs a collapse order key".to_owned());
        }
        Ok(Allocation {
            size,
            min,
            max,
            collapse_priority,
        })
    }
}

/// One child's allocation, once every field of it parses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Allocation {
    /// How the child claims cells.
    pub size: Size,
    /// Fewest cells while visible.
    pub min: u16,
    /// Most cells, if bounded.
    pub max: Option<u16>,
    /// Collapse order key, if the child collapses.
    pub collapse_priority: Option<i32>,
}

fn parse_extent(text: &str, field: &str) -> Result<u16, String> {
    text.trim()
        .parse::<u16>()
        .map_err(|_| format!("{field} must be a cell count"))
}

fn parse_optional_extent(text: &str, field: &str) -> Result<Option<u16>, String> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    parse_extent(text, field).map(Some)
}

fn parse_optional_priority(text: &str) -> Result<Option<i32>, String> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    text.trim()
        .parse::<i32>()
        .map(Some)
        .map_err(|_| "collapse order must be a whole number".to_owned())
}

fn nonzero(value: u16, field: &str) -> Result<std::num::NonZeroU16, String> {
    std::num::NonZeroU16::new(value).ok_or_else(|| format!("{field} must be greater than zero"))
}

impl LayoutEditorState {
    /// Open the editor on one screen's current layout.
    #[must_use]
    pub fn open(screen_id: Id, tree: LayoutNode) -> Self {
        Self {
            screen_id,
            tree,
            selected: Vec::new(),
            dialog: None,
            notice: None,
        }
    }

    /// The node the user is on, if the selection still names one.
    #[must_use]
    pub fn selected_node(&self) -> Option<&LayoutNode> {
        node_at(&self.tree, &self.selected)
    }

    /// Move to the selected node's parent.
    pub fn select_parent(&mut self) {
        self.selected.pop();
        self.notice = None;
    }

    /// Move to the selected node's first child.
    pub fn select_child(&mut self) {
        if let Some(LayoutNode::Split { children, .. }) = self.selected_node()
            && !children.is_empty()
        {
            self.selected.push(0);
            self.notice = None;
        }
    }

    /// Move to the next sibling, stopping at the last.
    pub fn select_next(&mut self) {
        self.step_sibling(1);
    }

    /// Move to the previous sibling, stopping at the first.
    pub fn select_previous(&mut self) {
        self.step_sibling(-1);
    }

    fn step_sibling(&mut self, delta: isize) {
        let Some((last, parent)) = self.selected.split_last() else {
            return;
        };
        let Some(LayoutNode::Split { children, .. }) = node_at(&self.tree, parent) else {
            return;
        };
        let Some(next) = last
            .checked_add_signed(delta)
            .filter(|index| *index < children.len())
        else {
            return;
        };
        let depth = self.selected.len();
        self.selected[depth - 1] = next;
        self.notice = None;
    }

    /// Every panel the screen declares that this tree does not place yet.
    ///
    /// A layout must place each declared panel exactly once, so this is the
    /// complete set of leaves a valid tree could still gain — which is what
    /// makes the add chooser closed rather than free text.
    #[must_use]
    pub fn addable_panels(&self, screen: &ScreenDescriptor) -> Vec<PanelId> {
        let placed = self.tree.panels_depth_first();
        screen
            .panels
            .iter()
            .map(|panel| panel.id)
            .filter(|id| !placed.contains(&id))
            .collect()
    }

    /// Apply the open dialog, or report why it cannot be applied.
    ///
    /// A refusal leaves the dialog open with its reason, so the invalid
    /// intermediate stays exactly where the user can correct it.
    pub fn apply_dialog(&mut self, screen: &ScreenDescriptor) {
        let Some(dialog) = self.dialog.clone() else {
            return;
        };
        let outcome = match dialog.kind {
            NodeDialogKind::AddLeaf => self.added(screen, &dialog),
            NodeDialogKind::EditChild => self.edited(&dialog),
        };
        match outcome.and_then(|tree| match refusal(screen, &tree) {
            Some(reason) => Err(reason),
            None => Ok(tree),
        }) {
            Ok(tree) => {
                self.tree = tree;
                self.dialog = None;
                self.notice = None;
            }
            Err(reason) => self.refuse(reason),
        }
    }

    fn refuse(&mut self, reason: String) {
        if let Some(dialog) = self.dialog.as_mut() {
            dialog.error = Some(reason);
        }
    }

    /// The tree the add dialog describes.
    fn added(&self, screen: &ScreenDescriptor, dialog: &NodeDialog) -> Result<LayoutNode, String> {
        // The fields are checked first so a mistyped size is reported as a
        // mistyped size, whatever else the dialog would go on to discover.
        let _ = dialog.allocation()?;
        let choices = self.addable_panels(screen);
        let Some(panel) = choices.get(dialog.panel_choice).copied() else {
            return Err("this screen places every panel it declares".to_owned());
        };
        let child = dialog.child(LayoutNode::Leaf { panel })?;
        let mut tree = self.tree.clone();
        let parent = self.split_parent_path();
        let Some(LayoutNode::Split { children, .. }) = node_at_mut(&mut tree, &parent) else {
            return Err("a leaf can only be added inside a split".to_owned());
        };
        if children.len() >= MAX_SPLIT_CHILDREN {
            return Err(format!(
                "a split holds at most {MAX_SPLIT_CHILDREN} children"
            ));
        }
        children.push(child);
        Ok(tree)
    }

    /// The split the add dialog would add into.
    ///
    /// A split node takes the child directly; a leaf takes it beside itself, in
    /// the leaf's own parent, which is what the user means by "add here".
    fn split_parent_path(&self) -> NodePath {
        match self.selected_node() {
            Some(LayoutNode::Split { .. }) => self.selected.clone(),
            _ => self
                .selected
                .split_last()
                .map(|(_, parent)| parent.to_vec())
                .unwrap_or_default(),
        }
    }

    /// The tree the edit dialog describes.
    fn edited(&self, dialog: &NodeDialog) -> Result<LayoutNode, String> {
        let Some((index, parent)) = self.selected.split_last() else {
            return Err("the root node has no allocation of its own".to_owned());
        };
        let (index, parent) = (*index, parent.to_vec());
        let mut tree = self.tree.clone();
        let Some(LayoutNode::Split { children, .. }) = node_at_mut(&mut tree, &parent) else {
            return Err("only a child of a split has an allocation".to_owned());
        };
        let Some(existing) = children.get(index) else {
            return Err("that node is no longer there".to_owned());
        };
        let replacement = dialog.child(existing.node.clone())?;
        children[index] = replacement;
        Ok(tree)
    }

    /// Wrap the selected node in a split along `axis`.
    ///
    /// A split needs children, and the node being wrapped is the first of them,
    /// so the tree stays one the validator would accept until the second child
    /// arrives — at which point it is offered for validation.
    pub fn split_selected(&mut self, axis: Axis) {
        let Some(node) = self.selected_node().cloned() else {
            return;
        };
        let wrapped = LayoutNode::Split {
            axis,
            gap: 0,
            children: vec![even_child(node)],
        };
        let mut tree = self.tree.clone();
        let Some(target) = node_at_mut(&mut tree, &self.selected.clone()) else {
            return;
        };
        *target = wrapped;
        self.tree = tree;
        self.notice = Some(format!(
            "add a second child: a split needs at least {MIN_SPLIT_CHILDREN}"
        ));
    }

    /// Remove the selected child, when the descriptor's invariants survive it.
    pub fn remove_selected(&mut self, screen: &ScreenDescriptor) {
        let Some((index, parent)) = self.selected.split_last() else {
            self.notice = Some("the root node cannot be removed".to_owned());
            return;
        };
        let (index, parent) = (*index, parent.to_vec());
        let mut tree = self.tree.clone();
        let Some(LayoutNode::Split { children, .. }) = node_at_mut(&mut tree, &parent) else {
            return;
        };
        if index >= children.len() {
            return;
        }
        children.remove(index);
        if let Some(reason) = refusal(screen, &tree) {
            self.notice = Some(reason);
            return;
        }
        self.tree = tree;
        self.selected = parent;
        self.notice = None;
    }

    /// The complete tree, when the validator accepts it.
    ///
    /// This is the only way a tree leaves the editor, so nothing the validator
    /// refuses can reach `ReplaceLayout`.
    ///
    /// # Errors
    ///
    /// Returns the validator's own refusal.
    pub fn complete(&self, screen: &ScreenDescriptor) -> Result<LayoutNode, String> {
        match refusal(screen, &self.tree) {
            Some(reason) => Err(reason),
            None => Ok(self.tree.clone()),
        }
    }
}

/// A child claiming an even share, which is what a wrapped node starts as.
fn even_child(node: LayoutNode) -> LayoutChild {
    LayoutChild {
        node,
        size: Size::Weight(std::num::NonZeroU16::MIN),
        min: 1,
        max: None,
        collapsible: false,
        collapse_priority: None,
    }
}

/// The descriptor validator's refusal of this candidate tree, if it has one.
fn refusal(screen: &ScreenDescriptor, tree: &LayoutNode) -> Option<String> {
    let mut candidate = screen.clone();
    candidate.layout = tree.clone();
    validate_descriptor(&candidate)
        .err()
        .map(|error| error.to_string())
}

fn node_at<'tree>(tree: &'tree LayoutNode, path: &[usize]) -> Option<&'tree LayoutNode> {
    let Some((index, rest)) = path.split_first() else {
        return Some(tree);
    };
    let LayoutNode::Split { children, .. } = tree else {
        return None;
    };
    node_at(&children.get(*index)?.node, rest)
}

fn node_at_mut<'tree>(
    tree: &'tree mut LayoutNode,
    path: &[usize],
) -> Option<&'tree mut LayoutNode> {
    let Some((index, rest)) = path.split_first() else {
        return Some(tree);
    };
    let LayoutNode::Split { children, .. } = tree else {
        return None;
    };
    node_at_mut(&mut children.get_mut(*index)?.node, rest)
}
