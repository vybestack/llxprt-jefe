//! Structural bounds on a parsed screen definition (issue #385, CW05-02).
//!
//! These are the counts and ranges the closed grammar declares — how many
//! panels, ports, relationships, activation fields, and bindings a screen may
//! have, how deep and how wide its layout may be, and which fields must be
//! present together. They run immediately after deserialization and before
//! anything is lowered, so an over-large or self-contradictory file never
//! reaches the internal descriptor contract at all.
//!
//! Presence rules are enforced as biconditionals rather than as "required when":
//! a `collapse_priority` on a non-collapsible child, and `values` on a
//! non-enum field, are rejected as loudly as their absence. A field that has no
//! effect is a mistaken belief about what the file does.

use super::ids::{
    MAX_ACTIVATION_FIELDS, MAX_BINDINGS_PER_SCREEN, MAX_FIELDS_PER_RESOURCE, MAX_LAYOUT_DEPTH,
    MAX_PANELS_PER_SCREEN, MAX_PORTS_PER_PANEL, MAX_RELATIONSHIPS_PER_SCREEN,
    MAX_RESOURCES_PER_SCREEN, MAX_SPLIT_CHILDREN, MIN_SPLIT_CHILDREN,
};
use super::screen_file::{
    ActivationKind, ChildFile, LayoutFile, RelationshipFile, ScreenFile, SizeFile, span_of,
};
use super::screen_file_bounds::{
    ScreenSyntaxError, ScreenSyntaxReason, check_component, check_declared_id,
    check_identifier_length, check_string_length,
};

/// Check every structural bound the grammar declares.
///
/// # Errors
///
/// Returns the first violated bound, attributed to the span of the offending
/// declaration where the syntax carries one.
pub fn check_shape(file: &ScreenFile) -> Result<(), ScreenSyntaxError> {
    check_collection_counts(file)?;
    // The layout depth bound runs before anything else walks the tree, so a
    // deeply nested layout is rejected on its shape rather than recursed into
    // by a check that carries no depth of its own.
    check_layout(&file.layout, 1)?;
    check_identifier_lengths(file)?;
    check_activation_fields(file)?;
    check_panels(file)
}

fn check_collection_counts(file: &ScreenFile) -> Result<(), ScreenSyntaxError> {
    if file.panels.is_empty() || file.panels.len() > MAX_PANELS_PER_SCREEN {
        return Err(ScreenSyntaxError::unspanned(
            ScreenSyntaxReason::PanelCount {
                count: file.panels.len(),
            },
        ));
    }
    if file.screen_schema == super::screen_file::LEGACY_SCREEN_SCHEMA && !file.resources.is_empty()
    {
        return Err(ScreenSyntaxError::unspanned(
            ScreenSyntaxReason::Malformed {
                detail: "`resources` requires screen_schema = 2".to_owned(),
            },
        ));
    }
    if file.resources.len() > MAX_RESOURCES_PER_SCREEN {
        return Err(ScreenSyntaxError::unspanned(
            ScreenSyntaxReason::ResourceCount {
                count: file.resources.len(),
            },
        ));
    }
    for resource in &file.resources {
        if resource.get_ref().fields.len() > MAX_FIELDS_PER_RESOURCE {
            return Err(ScreenSyntaxError::at(
                ScreenSyntaxReason::ResourceFieldCount {
                    count: resource.get_ref().fields.len(),
                },
                span_of(resource),
            ));
        }
    }
    if file.relationships.len() > MAX_RELATIONSHIPS_PER_SCREEN {
        return Err(ScreenSyntaxError::unspanned(
            ScreenSyntaxReason::RelationshipCount {
                count: file.relationships.len(),
            },
        ));
    }
    if file.activation.len() > MAX_ACTIVATION_FIELDS {
        return Err(ScreenSyntaxError::unspanned(
            ScreenSyntaxReason::ActivationFieldCount {
                count: file.activation.len(),
            },
        ));
    }
    if file.bindings.len() > MAX_BINDINGS_PER_SCREEN {
        return Err(ScreenSyntaxError::unspanned(
            ScreenSyntaxReason::BindingCount {
                count: file.bindings.len(),
            },
        ));
    }
    Ok(())
}

fn check_identifier_lengths(file: &ScreenFile) -> Result<(), ScreenSyntaxError> {
    check_identifier_length("id", file.id.get_ref())?;
    check_declared_id("route", &file.route)?;
    check_component("initial_focus", &file.initial_focus)?;
    for panel in &file.focus_order {
        check_component("focus_order entry", panel)?;
    }
    for binding in &file.bindings {
        check_identifier_length("bindings.context", &binding.get_ref().context)?;
        check_identifier_length("bindings.action", &binding.get_ref().action)?;
    }
    for resource in &file.resources {
        let declared = resource.get_ref();
        check_declared_id("resources.type_id", &declared.type_id)?;
        check_declared_id("resources.semantic_key", &declared.semantic_key)?;
        for field in &declared.fields {
            check_declared_id("resources.fields.id", &field.get_ref().id)?;
            check_string_length(&field.get_ref().label)?;
        }
    }
    for relationship in &file.relationships {
        let (source, target) = endpoints(relationship.get_ref());
        check_port_reference("relationships.source", source)?;
        check_port_reference("relationships.target", target)?;
    }
    check_layout_identifiers(&file.layout)
}

/// The endpoints of one relationship, whatever its kind.
fn endpoints(relationship: &RelationshipFile) -> (&str, &str) {
    match relationship {
        RelationshipFile::Scope { source, target }
        | RelationshipFile::MasterDetail { source, target, .. }
        | RelationshipFile::SessionTarget { source, target, .. } => (source, target),
    }
}

/// Check one `<panel>.<port>` reference.
///
/// The reference is split on the first separator, which is why a definition's
/// panel and port identifiers may not contain one — see [`check_component`].
fn check_port_reference(field: &'static str, reference: &str) -> Result<(), ScreenSyntaxError> {
    let Some((panel, port)) = reference.split_once('.') else {
        return Err(ScreenSyntaxError::unspanned(
            ScreenSyntaxReason::MalformedPortReference,
        ));
    };
    check_component(field, panel)?;
    check_component(field, port)
}

/// Check every panel a layout tree names.
fn check_layout_identifiers(node: &LayoutFile) -> Result<(), ScreenSyntaxError> {
    match node {
        LayoutFile::Leaf { panel } => check_component("layout.panel", panel),
        LayoutFile::Split { children, .. } => {
            for child in children {
                check_layout_identifiers(&child.node)?;
            }
            Ok(())
        }
    }
}

fn check_activation_fields(file: &ScreenFile) -> Result<(), ScreenSyntaxError> {
    for field in &file.activation {
        let declared = field.get_ref();
        check_declared_id("activation.name", &declared.name)?;
        let is_enum = declared.kind == ActivationKind::Enum;
        // Presence and emptiness are separate facts: `values = []` on a
        // non-enum field is a mistaken belief about what the field does, so it
        // is rejected as loudly as omitting it from an enum field.
        let permitted = match &declared.values {
            None => false,
            Some(values) if values.is_empty() => {
                return Err(ScreenSyntaxError::at(
                    ScreenSyntaxReason::EnumValuesMismatch { is_enum },
                    span_of(field),
                ));
            }
            Some(values) => {
                for value in values {
                    check_string_length(value)?;
                }
                true
            }
        };
        if is_enum != permitted {
            return Err(ScreenSyntaxError::at(
                ScreenSyntaxReason::EnumValuesMismatch { is_enum },
                span_of(field),
            ));
        }
    }
    Ok(())
}

fn check_panels(file: &ScreenFile) -> Result<(), ScreenSyntaxError> {
    for panel in &file.panels {
        let declared = panel.get_ref();
        check_component("panels.id", &declared.id)?;
        check_declared_id("panels.type", &declared.panel_type)?;
        if declared.ports.len() > MAX_PORTS_PER_PANEL {
            return Err(ScreenSyntaxError::at(
                ScreenSyntaxReason::PortCount {
                    count: declared.ports.len(),
                },
                span_of(panel),
            ));
        }
        for port in &declared.ports {
            check_component("ports.id", &port.get_ref().id)?;
            match &port.get_ref().owner {
                Some(owner) => check_declared_id("ports.owner", owner)?,
                None if file.screen_schema >= super::screen_file::SCREEN_SCHEMA => {
                    return Err(ScreenSyntaxError::at(
                        ScreenSyntaxReason::Malformed {
                            detail: "missing field `owner`".to_owned(),
                        },
                        span_of(port),
                    ));
                }
                None => {}
            }
            check_identifier_length("ports.type_id", &port.get_ref().type_id)?;
        }
    }
    Ok(())
}

fn check_layout(node: &LayoutFile, depth: usize) -> Result<(), ScreenSyntaxError> {
    if depth > MAX_LAYOUT_DEPTH {
        return Err(ScreenSyntaxError::unspanned(
            ScreenSyntaxReason::LayoutTooDeep { depth },
        ));
    }
    let LayoutFile::Split { children, .. } = node else {
        return Ok(());
    };
    if children.len() < MIN_SPLIT_CHILDREN || children.len() > MAX_SPLIT_CHILDREN {
        return Err(ScreenSyntaxError::unspanned(
            ScreenSyntaxReason::SplitChildCount {
                count: children.len(),
            },
        ));
    }
    for child in children {
        check_child(child)?;
        check_layout(&child.node, depth + 1)?;
    }
    Ok(())
}

fn check_child(child: &ChildFile) -> Result<(), ScreenSyntaxError> {
    let (field, extent) = match child.size {
        SizeFile::Fixed(cells) => ("size.fixed", cells),
        SizeFile::Weight(share) => ("size.weight", share),
    };
    if extent == 0 {
        return Err(ScreenSyntaxError::unspanned(
            ScreenSyntaxReason::ZeroExtent { field },
        ));
    }
    if child.min == 0 {
        return Err(ScreenSyntaxError::unspanned(
            ScreenSyntaxReason::ZeroExtent { field: "min" },
        ));
    }
    check_child_maximum(child)?;
    if child.collapsible != child.collapse_priority.is_some() {
        return Err(ScreenSyntaxError::unspanned(
            ScreenSyntaxReason::CollapsePriorityMismatch {
                collapsible: child.collapsible,
            },
        ));
    }
    Ok(())
}

fn check_child_maximum(child: &ChildFile) -> Result<(), ScreenSyntaxError> {
    let Some(max) = child.max else {
        return Ok(());
    };
    if max == 0 {
        return Err(ScreenSyntaxError::unspanned(
            ScreenSyntaxReason::ZeroExtent { field: "max" },
        ));
    }
    if max < child.min {
        return Err(ScreenSyntaxError::unspanned(
            ScreenSyntaxReason::MaxBelowMin {
                min: child.min,
                max,
            },
        ));
    }
    Ok(())
}
