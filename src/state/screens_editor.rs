//! The Screens/Layout editor's pure projection (issue #388, CW-08).
//!
//! One published screen registry and one candidate document become one row per
//! screen, in the order the document asks for. Nothing here decides whether a
//! layout is usable: a candidate descriptor is handed to
//! [`crate::workbench::validate::validate_descriptor`], which is the same
//! validator every registry publication goes through, and its refusal is
//! reported verbatim.
//!
//! The preview is the standard resolver run over a candidate descriptor. It
//! produces a [`ResolvedLayout`] the screen can draw and throws it away; the
//! session's own geometry is never touched, because a preview of a layout the
//! user has not saved must not move the screen they are previewing it on.

use crate::domain::Id;
use crate::domain::action_registry::Provenance;
use crate::persistence::settings_document::PublishedSettings;
use crate::workbench::allocate::LayoutError;
use crate::workbench::descriptor::{LayoutNode, ScreenDescriptor};
use crate::workbench::diagnostics::ScrCode;
use crate::workbench::geometry::Rect;
use crate::workbench::ids::{ScreenIdentity, ScreenInstanceId};
use crate::workbench::resolve::{PanelState, ResolvedLayout, resolve_layout};
use crate::workbench::screens::ScreenRegistry;
use crate::workbench::validate::validate_descriptor;

#[cfg(test)]
#[path = "screens_editor_tests.rs"]
mod screens_editor_tests;

/// Why a mandatory shipped screen's membership cannot be edited.
///
/// Composition includes the open Dashboard definition and every residual
/// compiled adapter unconditionally, so a toggle that appeared to turn one off
/// would write a preference nothing reads.
pub const MANDATORY_SCREEN_REASON: &str =
    "shipped screens are always composed and cannot be turned off";

/// Whether one screen's candidate descriptor still satisfies its invariants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompositionStatus {
    /// The candidate descriptor validates.
    Valid,
    /// The descriptor validator refused this candidate.
    Invalid {
        /// The stable composition diagnostic code.
        code: String,
        /// The validator's own reason, verbatim.
        reason: String,
    },
}

/// One screen as the editor presents it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenEditorRow {
    /// The screen's stable identity.
    pub screen_id: ScreenIdentity,
    /// The screen's identity as a configuration owner, when it spells one.
    ///
    /// Every registered screen should: the registry's identifiers and the
    /// configuration grammar are the same grammar. A screen that somehow does
    /// not is kept as a row and reported, because dropping it here would write
    /// a membership array missing a screen with nothing to say it was lost.
    pub owner: Option<Id>,
    /// The screen's title, from its descriptor.
    pub title: String,
    /// Whether composition includes this screen.
    pub enabled: bool,
    /// Why enablement is read-only for this screen, when it is.
    pub enablement_locked: Option<&'static str>,
    /// This row's position in the presented order.
    pub order_index: u16,
    /// Whether the candidate descriptor still validates.
    pub composition: CompositionStatus,
    /// Where the effective order and layout came from.
    pub provenance: Provenance,
}

/// One typed intent the Screens/Layout editor emits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScreenIntent {
    /// Include this screen in composition, or stop including it.
    SetEnabled {
        /// The screen to change.
        screen_id: Id,
        /// Whether composition includes it.
        enabled: bool,
    },
    /// Move this screen immediately before `anchor`.
    MoveBefore {
        /// The screen to move.
        screen_id: Id,
        /// The screen it lands in front of.
        anchor: Id,
    },
    /// Move this screen immediately after `anchor`.
    MoveAfter {
        /// The screen to move.
        screen_id: Id,
        /// The screen it lands behind.
        anchor: Id,
    },
    /// Replace this screen's whole layout tree.
    ReplaceLayout {
        /// The screen to override.
        screen_id: Id,
        /// The complete tree to write.
        layout: Box<LayoutNode>,
    },
    /// Remove this screen's whole layout override.
    ResetLayout {
        /// The screen to reset.
        screen_id: Id,
    },
}

impl ScreenIntent {
    /// The screen this intent names.
    #[must_use]
    pub const fn screen_id(&self) -> &Id {
        match self {
            Self::SetEnabled { screen_id, .. }
            | Self::MoveBefore { screen_id, .. }
            | Self::MoveAfter { screen_id, .. }
            | Self::ReplaceLayout { screen_id, .. }
            | Self::ResetLayout { screen_id } => screen_id,
        }
    }
}

/// Project one screen registry and one candidate document into editor rows.
#[must_use]
pub fn project_screens(
    registry: &ScreenRegistry,
    published: &PublishedSettings,
) -> Vec<ScreenEditorRow> {
    let mut screens: Vec<&ScreenDescriptor> = registry.screens().iter().collect();
    screens.sort_by_key(|screen| presented_position(published, screen));
    screens
        .into_iter()
        .enumerate()
        .map(|(index, screen)| project_row(index, screen, published))
        .collect()
}

/// Every enabled screen, once each, in presented order.
///
/// This is what a membership or order edit serializes, so "each enabled ID
/// exactly once and no disabled ID" is a property of how the array is built
/// rather than a rule something has to check afterwards.
#[must_use]
pub fn screen_membership(rows: &[ScreenEditorRow]) -> Vec<Id> {
    rows.iter()
        .filter(|row| row.enabled)
        .filter_map(|row| row.owner.clone())
        .collect()
}

/// Resolve one candidate layout at the given dimensions, changing nothing.
///
/// The resolver is the standard one, so a preview and the geometry a restart
/// would produce cannot disagree. The resolved snapshot is the caller's to
/// draw and discard.
///
/// # Errors
///
/// Returns the resolver's own [`LayoutError`] when the candidate's arithmetic
/// leaves the checked range.
pub fn preview_layout(
    screen: &ScreenDescriptor,
    layout: &LayoutNode,
    cols: u16,
    rows: u16,
) -> Result<ResolvedLayout, LayoutError> {
    let candidate = with_layout(screen, layout.clone());
    resolve_layout(
        &candidate,
        ScreenInstanceId::preview(),
        Rect::new(0, 0, cols, rows),
        &PanelState::all_visible(),
    )
}

/// The descriptor this screen would have with `layout` in place.
fn with_layout(screen: &ScreenDescriptor, layout: LayoutNode) -> ScreenDescriptor {
    let mut candidate = screen.clone();
    candidate.layout = layout;
    candidate
}

fn project_row(
    index: usize,
    screen: &ScreenDescriptor,
    published: &PublishedSettings,
) -> ScreenEditorRow {
    // Composition includes every shipped screen whatever settings say, so a
    // mandatory row reports the truth and says why it cannot be changed.
    let mandatory =
        screen.id.compiled().is_some() || screen.id == crate::workbench::DASHBOARD_IDENTITY;
    let owner = Id::parse(screen.id.as_str()).ok();
    let composition = owner.as_ref().map_or_else(
        || CompositionStatus::Invalid {
            code: ScrCode::E301.as_str().to_owned(),
            reason: format!(
                "screen {} is not a configuration owner identity",
                screen.id.as_str()
            ),
        },
        |_| composition(screen, published),
    );
    ScreenEditorRow {
        screen_id: screen.id,
        owner,
        title: screen.title.clone(),
        enabled: mandatory || names_screen(&published.workbench.enabled_screens, screen),
        enablement_locked: mandatory.then_some(MANDATORY_SCREEN_REASON),
        order_index: u16::try_from(index).unwrap_or(u16::MAX),
        composition,
        provenance: provenance(screen, published),
    }
}

/// Where this row's order and layout came from.
fn provenance(screen: &ScreenDescriptor, published: &PublishedSettings) -> Provenance {
    let named = names_screen(&published.workbench.screen_order, screen)
        || names_screen(&published.workbench.enabled_screens, screen)
        || layout_override(screen, published).is_some();
    if named {
        Provenance::Settings {
            source: "settings".to_owned(),
        }
    } else {
        Provenance::Compiled
    }
}

/// Whether the candidate descriptor still satisfies its invariants.
///
/// The refusal is the descriptor validator's; this only says which screen it
/// was asked about.
fn composition(screen: &ScreenDescriptor, published: &PublishedSettings) -> CompositionStatus {
    let Some(layout) = layout_override(screen, published) else {
        return CompositionStatus::Valid;
    };
    let candidate = match layout {
        Ok(layout) => with_layout(screen, layout),
        Err(reason) => {
            return CompositionStatus::Invalid {
                code: ScrCode::E301.as_str().to_owned(),
                reason,
            };
        }
    };
    match validate_descriptor(&candidate) {
        Ok(()) => CompositionStatus::Valid,
        Err(error) => CompositionStatus::Invalid {
            code: ScrCode::E301.as_str().to_owned(),
            reason: error.to_string(),
        },
    }
}

/// This screen's layout override, when the document carries one.
///
/// The inner result separates "the document says nothing about this screen"
/// from "the document says something the layout grammar cannot read", because
/// those are different answers and only the second is a problem to report. The
/// grammar is the workbench's own, so an override and a screen definition file
/// are read by exactly one reader.
fn layout_override(
    screen: &ScreenDescriptor,
    published: &PublishedSettings,
) -> Option<Result<LayoutNode, String>> {
    let id = Id::parse(screen.id.as_str()).ok()?;
    let values = published.workbench.layout_overrides.get(&id)?;
    Some(crate::workbench::screen_lowering_layout::lower_settings_layout(values))
}

/// Where in the presented order this screen sits.
///
/// Screens the document orders lead, in the order it gives; everything else
/// follows in the registry's own order, so a document that names one screen
/// does not reshuffle the rest.
fn presented_position(published: &PublishedSettings, screen: &ScreenDescriptor) -> (usize, usize) {
    let ordered = published
        .workbench
        .screen_order
        .iter()
        .position(|id| id.as_str() == screen.id.as_str());
    ordered.map_or((1, 0), |position| (0, position))
}

fn names_screen(ids: &[Id], screen: &ScreenDescriptor) -> bool {
    ids.iter().any(|id| id.as_str() == screen.id.as_str())
}
