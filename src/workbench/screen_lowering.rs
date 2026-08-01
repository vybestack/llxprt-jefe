//! The sole conversion from external screen syntax to internal descriptors
//! (issue #385, CW05-02).
//!
//! Lowering happens exactly once per file, and nothing it consumes survives it:
//! the composed registry holds internal descriptors only, so a later change to
//! the external syntax cannot reach code that renders or resolves a screen.
//!
//! Lowering copies. It resolves names against immutable registries — panel
//! types, actions, contexts — and it interns identifier text, but it supplies no
//! semantic default and derives no value the file did not state. Anything the
//! syntax leaves out is a rejection during parsing, not a default invented here,
//! because a default chosen at this layer would be behavior no one wrote down.
//!
//! The screen's identity must match the file it lives in. `local.review` is
//! declared in `review.screen.toml` and nowhere else, so one screen has exactly
//! one home on disk and a duplicate identity is a duplicate file name, which the
//! filesystem already forbids.

use std::path::Path;

use crate::domain::ByteSpan;
use crate::persistence::diagnostic::DiagnosticPath;

use super::descriptor::{
    PanelDescriptor, PortDescriptor, PortDirection, PortRef, ScreenDescriptor,
};
use super::ids::{
    CustomScreenId, IdError, PanelId, PortId, RouteId, ScreenIdentity, VersionedTypeId,
};
use super::intern::intern;
use super::lowering_error::LoweringError;
use super::panel_types::resolve_panel_type;
use super::screen_file::{
    BindingRefFile, PanelFile, PortDirectionFile, PortFile, ScreenFile, span_of,
};
use super::screen_lowering_layout::{lower_layout, lower_relationships};
use super::screen_lowering_values::{lower_config, resolve_binding};
use super::validate::validate_descriptor;

/// Where a lowered screen came from.
///
/// Provenance is kept beside the descriptor rather than inside it, because a
/// descriptor describes a screen and says nothing about how it reached the
/// program: the compiled screens have no file to point at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenProvenance {
    /// The definition file.
    pub path: DiagnosticPath,
    /// Byte range of the declared identity, for diagnostics that name the
    /// screen rather than one of its parts.
    pub id_span: ByteSpan,
}

/// One screen definition, lowered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredScreen {
    /// The internal descriptor, already structurally validated.
    pub descriptor: ScreenDescriptor,
    /// Where it came from.
    pub provenance: ScreenProvenance,
}

/// Lower one parsed screen definition into an internal descriptor.
///
/// `member` is the file-name stem, which the declared identity must match.
///
/// # Errors
///
/// Returns the first rule the definition broke, classified so composition can
/// pair it with the right configuration diagnostic.
pub fn lower_screen(
    file: &ScreenFile,
    member: &str,
    path: &Path,
) -> Result<LoweredScreen, LoweringError> {
    let expected = format!("local.{member}");
    if file.id.get_ref() != &expected {
        return Err(LoweringError::IdentityMismatch { expected });
    }
    let id =
        CustomScreenId::parse(intern(&expected)?).map_err(|reason| LoweringError::Identifier {
            field: "id",
            reason,
        })?;
    let panels = file
        .panels
        .iter()
        .map(|panel| lower_panel(panel.get_ref()))
        .collect::<Result<Vec<_>, _>>()?;
    let descriptor = ScreenDescriptor {
        id: ScreenIdentity::Custom(id),
        title: file.title.clone(),
        route: parse_id("route", &file.route, RouteId::parse)?,
        initial_focus: parse_id("initial_focus", &file.initial_focus, PanelId::parse)?,
        focus_order: file
            .focus_order
            .iter()
            .map(|panel| parse_id("focus_order entry", panel, PanelId::parse))
            .collect::<Result<Vec<_>, _>>()?,
        panels,
        layout: lower_layout(&file.layout)?,
        relationships: lower_relationships(&file.relationships)?,
    };
    for binding in &file.bindings {
        check_binding(binding.get_ref())?;
    }
    validate_descriptor(&descriptor)?;
    Ok(LoweredScreen {
        descriptor,
        provenance: ScreenProvenance {
            path: DiagnosticPath::new(path.to_string_lossy()),
            id_span: span_of(&file.id),
        },
    })
}

/// Intern one declared identifier and parse it with `parse`.
fn parse_id<T>(
    field: &'static str,
    value: &str,
    parse: fn(&'static str) -> Result<T, IdError>,
) -> Result<T, LoweringError> {
    parse(intern(value)?).map_err(|reason| LoweringError::Identifier { field, reason })
}

fn lower_panel(panel: &PanelFile) -> Result<PanelDescriptor, LoweringError> {
    Ok(PanelDescriptor {
        id: parse_id("panels.id", &panel.id, PanelId::parse)?,
        panel_type: resolve_panel_type(intern(&panel.panel_type)?)?,
        config: lower_config(&panel.config)?,
        focusable: panel.focusable,
        required: panel.required,
        ports: panel
            .ports
            .iter()
            .map(|port| lower_port(port.get_ref()))
            .collect::<Result<Vec<_>, _>>()?,
    })
}

fn lower_port(port: &PortFile) -> Result<PortDescriptor, LoweringError> {
    Ok(PortDescriptor {
        id: parse_id("ports.id", &port.id, PortId::parse)?,
        direction: match port.direction {
            PortDirectionFile::Input => PortDirection::Input,
            PortDirectionFile::Output => PortDirection::Output,
        },
        type_id: parse_id("ports.type_id", &port.type_id, VersionedTypeId::parse)?,
        required: port.required,
        retained: port.retained,
    })
}

/// Resolve a panel identifier named from the layout tree.
///
/// # Errors
///
/// Returns an identifier error when the name is malformed, or an interning
/// error when the identifier table is exhausted.
pub fn lower_panel_ref(value: &str) -> Result<PanelId, LoweringError> {
    parse_id("layout panel", value, PanelId::parse)
}

/// Resolve a `<panel>.<port>` reference.
///
/// The split is on the first separator, so a panel identifier may not contain
/// one. That is narrower than the panel grammar allows, and it is why a
/// definition's panels are named `pr-list` rather than `pr.list`.
///
/// # Errors
///
/// Returns [`LoweringError::PortReference`] when the reference has no separator,
/// or an identifier error when either half is malformed.
pub fn lower_port_ref(value: &str) -> Result<PortRef, LoweringError> {
    let Some((panel, port)) = value.split_once('.') else {
        return Err(LoweringError::PortReference);
    };
    Ok(PortRef {
        panel: parse_id("relationship panel", panel, PanelId::parse)?,
        port: parse_id("relationship port", port, PortId::parse)?,
    })
}

/// Check that a binding names an action and a context the registry publishes.
///
/// Nothing in this issue consumes the resolved binding: there is no keymap
/// composition here and no editor. Resolving it anyway is the point — a
/// definition that names an action the program does not have is rejected before
/// it can be enabled, rather than silently doing nothing once it is.
fn check_binding(binding: &BindingRefFile) -> Result<(), LoweringError> {
    resolve_binding(&binding.context, &binding.action)
}
