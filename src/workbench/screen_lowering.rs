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

use super::activation::ScreenBinding;
use super::descriptor::{
    PanelDescriptor, PortDescriptor, PortDirection, PortRef, ScreenDescriptor,
};
use super::ids::{
    CUSTOM_SCREEN_NAMESPACE, CustomScreenId, IdError, PanelId, PanelTypeId, PluginScreenId, PortId,
    RouteId, ScreenIdentity, VersionedTypeId, check_identifier,
};
use super::intern::intern;
use super::lowering_error::LoweringError;
use super::panel_types::{DEFINABLE_PANEL_TYPES, find_panel_type};
use super::screen_file::{PanelFile, PortDirectionFile, PortFile, ScreenFile, span_of};
use super::screen_lowering_layout::{lower_layout, lower_relationships};
use super::screen_lowering_values::{
    lower_activation, lower_bindings, lower_config, published_actions,
};
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
    let expected = format!("{CUSTOM_SCREEN_NAMESPACE}{member}");
    let id =
        CustomScreenId::parse(intern(&expected)?).map_err(|reason| LoweringError::Identifier {
            field: "id",
            reason,
        })?;
    lower_with(
        file,
        &expected,
        ScreenIdentity::Custom(id),
        &DEFINABLE_PANEL_TYPES,
        path,
    )
}

/// Lower one parsed package screen definition into an internal descriptor.
///
/// `expected_id` is the manifest-declared owner-qualified screen identity this
/// file must declare.  `allowed_panels` is the set of panel-type identifiers
/// the declaring manifest exposes, so a package screen may resolve only panel
/// types that package declared — without adding dynamic panel ids to the global
/// built-in registry.
///
/// # Errors
///
/// Returns the first rule the definition broke, classified so composition can
/// pair it with the right configuration diagnostic.
pub fn lower_package_screen(
    file: &ScreenFile,
    expected_id: &str,
    allowed_panels: &[&str],
    path: &Path,
) -> Result<LoweredScreen, LoweringError> {
    let id = PluginScreenId::parse(intern(expected_id)?).map_err(|reason| {
        LoweringError::Identifier {
            field: "id",
            reason,
        }
    })?;
    lower_with(
        file,
        expected_id,
        ScreenIdentity::Package(id),
        allowed_panels,
        path,
    )
}

/// Shared lowering core for user definitions and package screens.
///
/// The identity check, panel resolution, layout, relationships, activation, and
/// bindings are identical for both sources; only the expected identity string,
/// the `ScreenIdentity` variant, and the allowed panel-type set differ.
fn lower_with(
    file: &ScreenFile,
    expected_id: &str,
    identity: ScreenIdentity,
    allowed_panels: &[&str],
    path: &Path,
) -> Result<LoweredScreen, LoweringError> {
    if file.id.get_ref() != expected_id {
        return Err(LoweringError::IdentityMismatch {
            expected: expected_id.to_owned(),
        });
    }
    let panels = file
        .panels
        .iter()
        .map(|panel| lower_panel(panel.get_ref(), allowed_panels))
        .collect::<Result<Vec<_>, _>>()?;
    let descriptor = ScreenDescriptor {
        id: identity,
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
        activation: lower_activation(
            &file
                .activation
                .iter()
                .map(|field| field.get_ref().clone())
                .collect::<Vec<_>>(),
        )?,
        bindings: lower_declared_bindings(file)?,
    };
    validate_descriptor(&descriptor)?;
    Ok(LoweredScreen {
        descriptor,
        provenance: ScreenProvenance {
            path: DiagnosticPath::new(path.to_string_lossy()),
            id_span: span_of(&file.id),
        },
    })
}

/// Validate one declared identifier, then intern it and parse it with `parse`.
///
/// The grammar is checked first so text that can never become an identifier
/// never consumes a slot in the process-lifetime interning table.
fn parse_id<T>(
    field: &'static str,
    value: &str,
    parse: fn(&'static str) -> Result<T, IdError>,
) -> Result<T, LoweringError> {
    let bad = |reason| LoweringError::Identifier { field, reason };
    check_declared(value).map_err(bad)?;
    parse(intern(value)?).map_err(bad)
}

/// Pre-intern grammar check covering both plain identifiers and versioned
/// types, which share every rule except the trailing `@<version>`.
fn check_declared(value: &str) -> Result<(), IdError> {
    match value.split_once('@') {
        Some((name, _)) if !name.is_empty() => check_identifier(name),
        Some(_) => Err(IdError::Empty),
        None => check_identifier(value),
    }
}

fn lower_panel(panel: &PanelFile, allowed: &[&str]) -> Result<PanelDescriptor, LoweringError> {
    find_panel_type(&panel.panel_type, allowed)?;
    Ok(PanelDescriptor {
        id: parse_id("panels.id", &panel.id, PanelId::parse)?,
        panel_type: parse_id("panels.type", &panel.panel_type, PanelTypeId::parse)?,
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

/// Resolve every binding a definition requests against the compiled inventory.
///
/// The inventory is built once per screen, because compiling it is not free and
/// a screen may request up to 256 bindings.
fn lower_declared_bindings(file: &ScreenFile) -> Result<Vec<ScreenBinding>, LoweringError> {
    if file.bindings.is_empty() {
        return Ok(Vec::new());
    }
    let published = published_actions()?;
    let declared: Vec<(&str, &str)> = file
        .bindings
        .iter()
        .map(|binding| {
            (
                binding.get_ref().context.as_str(),
                binding.get_ref().action.as_str(),
            )
        })
        .collect();
    lower_bindings(&published, &declared)
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
