//! Declared bounds on the external screen syntax (issue #385, CW05-02).
//!
//! Every bound here is inclusive and is checked rather than clamped: a document
//! that is one entry, one element, one byte, or one level past a limit is
//! rejected with the measured value, so an author sees what to remove instead of
//! silently losing the tail of a list.
//!
//! The generic value bounds — depth, map size, array size, string length — are
//! measured on the raw document before it is deserialized, because a document
//! shaped to exhaust memory should be refused on its shape, not after the values
//! it describes have been built.

use crate::domain::ByteSpan;
use crate::persistence::diagnostic::{ARRAY_LIMIT, MAP_LIMIT, NESTING_LIMIT, STRING_LIMIT};

use super::ids::{
    ID_BYTE_LIMIT, MAX_ACTIVATION_FIELDS, MAX_BINDINGS_PER_SCREEN, MAX_LAYOUT_DEPTH,
    MAX_PANELS_PER_SCREEN, MAX_PORTS_PER_PANEL, MAX_RELATIONSHIPS_PER_SCREEN, MAX_SPLIT_CHILDREN,
    MIN_SPLIT_CHILDREN,
};

/// A violated syntax rule, with the measurement that violated it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScreenSyntaxReason {
    /// The document is not well-formed TOML, names an unknown field, repeats a
    /// key, or gives a field the wrong type.
    Malformed {
        /// The parser's own description, which never echoes a value.
        detail: String,
    },
    /// The document declares a schema this build does not implement.
    UnsupportedSchema {
        /// Declared schema version.
        found: u32,
    },
    /// A container nests past [`NESTING_LIMIT`].
    DocumentTooDeep {
        /// Measured depth.
        depth: usize,
    },
    /// A map holds more than [`MAP_LIMIT`] entries.
    MapTooLarge {
        /// Measured entry count.
        entries: usize,
    },
    /// An array holds more than [`ARRAY_LIMIT`] elements.
    ArrayTooLarge {
        /// Measured element count.
        elements: usize,
    },
    /// A string is longer than [`STRING_LIMIT`] bytes.
    StringTooLong {
        /// Measured byte length.
        bytes: usize,
    },
    /// An identifier is longer than [`ID_BYTE_LIMIT`] bytes.
    IdentifierTooLong {
        /// Which field carried it.
        field: &'static str,
        /// Measured byte length.
        bytes: usize,
    },
    /// The screen declares a panel count outside `1..=16`.
    PanelCount {
        /// Declared panel count.
        count: usize,
    },
    /// A panel declares more than [`MAX_PORTS_PER_PANEL`] ports.
    PortCount {
        /// Declared port count.
        count: usize,
    },
    /// The screen declares more than [`MAX_RELATIONSHIPS_PER_SCREEN`]
    /// relationships.
    RelationshipCount {
        /// Declared relationship count.
        count: usize,
    },
    /// The screen declares more than [`MAX_ACTIVATION_FIELDS`] activation
    /// fields.
    ActivationFieldCount {
        /// Declared field count.
        count: usize,
    },
    /// The screen declares more than [`MAX_BINDINGS_PER_SCREEN`] bindings.
    BindingCount {
        /// Declared binding count.
        count: usize,
    },
    /// A split declares a child count outside `2..=8`.
    SplitChildCount {
        /// Declared child count.
        count: usize,
    },
    /// The layout tree nests past [`MAX_LAYOUT_DEPTH`].
    LayoutTooDeep {
        /// Measured depth.
        depth: usize,
    },
    /// A size, minimum, or maximum is zero, which is not a size but a
    /// visibility decision.
    ZeroExtent {
        /// Which field carried it.
        field: &'static str,
    },
    /// A child declares a maximum below its minimum.
    MaxBelowMin {
        /// Declared minimum.
        min: u16,
        /// Declared maximum.
        max: u16,
    },
    /// A collapse priority is present without `collapsible`, or absent with it.
    CollapsePriorityMismatch {
        /// Whether the child declared itself collapsible.
        collapsible: bool,
    },
    /// An enum activation field declares no permitted values, or a non-enum
    /// field declares some.
    EnumValuesMismatch {
        /// Whether the field declared the enum kind.
        is_enum: bool,
    },
    /// An identifier that has to be addressable in a `<panel>.<port>` reference
    /// contains the separator those references are split on.
    SeparatorInComponent {
        /// Which field carried it.
        field: &'static str,
    },
    /// A relationship endpoint is not spelled `<panel>.<port>`.
    MalformedPortReference,
}

impl std::fmt::Display for ScreenSyntaxReason {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.fmt_shape(formatter)
            .unwrap_or_else(|| self.fmt_count(formatter))
    }
}

impl ScreenSyntaxReason {
    /// Render the well-formedness and generic value bounds, if this is one.
    fn fmt_shape(&self, formatter: &mut std::fmt::Formatter<'_>) -> Option<std::fmt::Result> {
        Some(match self {
            Self::Malformed { detail } => write!(formatter, "{detail}"),
            Self::UnsupportedSchema { found } => write!(
                formatter,
                "screen_schema {found} is not supported (expected {})",
                super::screen_file::SCREEN_SCHEMA
            ),
            Self::DocumentTooDeep { depth } => {
                write!(
                    formatter,
                    "document nests {depth} levels (max {NESTING_LIMIT})"
                )
            }
            Self::MapTooLarge { entries } => {
                write!(formatter, "map holds {entries} entries (max {MAP_LIMIT})")
            }
            Self::ArrayTooLarge { elements } => write!(
                formatter,
                "array holds {elements} elements (max {ARRAY_LIMIT})"
            ),
            Self::StringTooLong { bytes } => {
                write!(formatter, "string is {bytes} bytes (max {STRING_LIMIT})")
            }
            Self::IdentifierTooLong { field, bytes } => {
                write!(formatter, "{field} is {bytes} bytes (max {ID_BYTE_LIMIT})")
            }
            _ => return None,
        })
    }

    /// Render the structural count and range bounds.
    fn fmt_count(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PanelCount { count } => write!(
                formatter,
                "screen declares {count} panels (allowed 1..={MAX_PANELS_PER_SCREEN})"
            ),
            Self::PortCount { count } => write!(
                formatter,
                "panel declares {count} ports (max {MAX_PORTS_PER_PANEL})"
            ),
            Self::RelationshipCount { count } => write!(
                formatter,
                "screen declares {count} relationships (max {MAX_RELATIONSHIPS_PER_SCREEN})"
            ),
            Self::ActivationFieldCount { count } => write!(
                formatter,
                "screen declares {count} activation fields (max {MAX_ACTIVATION_FIELDS})"
            ),
            Self::BindingCount { count } => write!(
                formatter,
                "screen declares {count} bindings (max {MAX_BINDINGS_PER_SCREEN})"
            ),
            Self::SplitChildCount { count } => write!(
                formatter,
                "split declares {count} children (allowed {MIN_SPLIT_CHILDREN}..={MAX_SPLIT_CHILDREN})"
            ),
            Self::LayoutTooDeep { depth } => write!(
                formatter,
                "layout nests {depth} levels (max {MAX_LAYOUT_DEPTH})"
            ),
            Self::ZeroExtent { field } => write!(formatter, "{field} must be at least 1"),
            Self::MaxBelowMin { min, max } => {
                write!(formatter, "max {max} is below min {min}")
            }
            Self::CollapsePriorityMismatch { collapsible } => write!(
                formatter,
                "collapse_priority must be present exactly when collapsible is true (collapsible = {collapsible})"
            ),
            Self::EnumValuesMismatch { is_enum } => write!(
                formatter,
                "values must be present exactly for enum fields (enum = {is_enum})"
            ),
            Self::SeparatorInComponent { field } => write!(
                formatter,
                "{field} may not contain '.', because port references are split on it"
            ),
            Self::MalformedPortReference => {
                formatter.write_str("port reference must be spelled '<panel>.<port>'")
            }
            _ => Ok(()),
        }
    }
}

/// A violated syntax rule and where it occurred.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenSyntaxError {
    /// The violated rule.
    pub reason: ScreenSyntaxReason,
    /// Byte range of the offending text, when the parser can attribute one.
    pub span: Option<ByteSpan>,
}

impl std::fmt::Display for ScreenSyntaxError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.reason)
    }
}

impl std::error::Error for ScreenSyntaxError {}

impl ScreenSyntaxError {
    /// A violation with no attributable span.
    #[must_use]
    pub const fn unspanned(reason: ScreenSyntaxReason) -> Self {
        Self { reason, span: None }
    }

    /// A violation attributed to a byte range.
    #[must_use]
    pub const fn at(reason: ScreenSyntaxReason, span: ByteSpan) -> Self {
        Self {
            reason,
            span: Some(span),
        }
    }
}

/// Check the generic value bounds over a raw document.
///
/// Depth counts nested *tables*, not arrays. An array of tables is one level of
/// data, not two, which is what makes the two declared depth budgets consistent:
/// a layout nested to its own maximum of [`MAX_LAYOUT_DEPTH`] levels spends
/// exactly [`NESTING_LIMIT`] table levels, so neither bound makes the other
/// unreachable.
///
/// # Errors
///
/// Returns the first container that nests too deep or holds too much, or the
/// first string that is too long.
pub fn check_document_bounds(document: &toml::Table) -> Result<(), ScreenSyntaxError> {
    check_table(document, 1)
}

fn check_table(table: &toml::Table, depth: usize) -> Result<(), ScreenSyntaxError> {
    if depth > NESTING_LIMIT {
        return Err(ScreenSyntaxError::unspanned(
            ScreenSyntaxReason::DocumentTooDeep { depth },
        ));
    }
    if table.len() > MAP_LIMIT {
        return Err(ScreenSyntaxError::unspanned(
            ScreenSyntaxReason::MapTooLarge {
                entries: table.len(),
            },
        ));
    }
    for value in table.values() {
        check_value(value, depth)?;
    }
    Ok(())
}

fn check_value(value: &toml::Value, depth: usize) -> Result<(), ScreenSyntaxError> {
    match value {
        toml::Value::String(text) if text.len() > STRING_LIMIT => Err(
            ScreenSyntaxError::unspanned(ScreenSyntaxReason::StringTooLong { bytes: text.len() }),
        ),
        toml::Value::Array(elements) => {
            if elements.len() > ARRAY_LIMIT {
                return Err(ScreenSyntaxError::unspanned(
                    ScreenSyntaxReason::ArrayTooLarge {
                        elements: elements.len(),
                    },
                ));
            }
            for element in elements {
                check_value(element, depth)?;
            }
            Ok(())
        }
        toml::Value::Table(table) => check_table(table, depth + 1),
        _ => Ok(()),
    }
}

/// Check that one identifier-shaped field fits the identifier byte limit.
///
/// # Errors
///
/// Returns [`ScreenSyntaxReason::IdentifierTooLong`] naming the field.
pub fn check_identifier_length(field: &'static str, value: &str) -> Result<(), ScreenSyntaxError> {
    if value.len() > ID_BYTE_LIMIT {
        return Err(ScreenSyntaxError::unspanned(
            ScreenSyntaxReason::IdentifierTooLong {
                field,
                bytes: value.len(),
            },
        ));
    }
    Ok(())
}

/// Check one string-valued field against the string byte limit.
///
/// # Errors
///
/// Returns [`ScreenSyntaxReason::StringTooLong`].
pub fn check_string_length(value: &str) -> Result<(), ScreenSyntaxError> {
    if value.len() > STRING_LIMIT {
        return Err(ScreenSyntaxError::unspanned(
            ScreenSyntaxReason::StringTooLong { bytes: value.len() },
        ));
    }
    Ok(())
}

/// Check one identifier that must also be addressable inside a port reference.
///
/// Panel and port identifiers are named again as `<panel>.<port>`, and that
/// reference is split on its first separator. An identifier containing one
/// would therefore be either unreachable or ambiguous with a different pair, so
/// the external grammar is narrower here than the internal identifier grammar.
///
/// # Errors
///
/// Returns the length violation or [`ScreenSyntaxReason::SeparatorInComponent`].
pub fn check_component(field: &'static str, value: &str) -> Result<(), ScreenSyntaxError> {
    check_identifier_length(field, value)?;
    if value.contains('.') {
        return Err(ScreenSyntaxError::unspanned(
            ScreenSyntaxReason::SeparatorInComponent { field },
        ));
    }
    Ok(())
}
