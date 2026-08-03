//! Typed navigation routes and the activations that reach them (issue #386).
//!
//! A route is how something asks to navigate to a screen, and an activation is
//! the argument list it supplies. Both are closed: a route exists only because
//! a screen descriptor declares it, and an activation may only carry the field
//! kinds [`ActivationKind`] already publishes. Nothing here mutates navigation
//! state — this module answers "is this a real route, and is this a legal
//! argument list for it?", so the reducer can refuse before it touches
//! anything.
//!
//! Three properties follow from keeping the answer here rather than in the
//! reducer:
//!
//! - validation is total and value-free, so a refusal can be rendered without
//!   quoting whatever the caller passed;
//! - there is no secret kind, and no generic payload, so an activation cannot
//!   carry credential material into navigation state or into a diagnostic;
//! - the bounds (field count, serialized size, identifier length) are enforced
//!   at construction, so an over-large activation never reaches the reducer.
//!
//! This module is I/O-free and depends only on the descriptor vocabulary.

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

use crate::domain::Id;

use super::activation::{ActivationField, ActivationKind};
use super::ids::{MAX_ACTIVATION_FIELDS, RouteId, ScreenIdentity};
use super::screens::ScreenRegistry;

/// Maximum serialized byte size of one activation's values.
pub const MAX_ACTIVATION_BYTES: usize = 262_144;

/// Closed navigation diagnostic code set.
///
/// Navigation has one code because every navigation refusal has the same
/// operator response: the current screen was retained and the request was not
/// performed. What differs is the detail, which [`ActivationError`] supplies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NavCode {
    /// A navigation request was refused and the current instance was retained.
    E001,
}

impl NavCode {
    /// The stable operator-facing code.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::E001 => "NAV-E001",
        }
    }
}

impl fmt::Display for NavCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// One value carried by an activation field.
///
/// The variants mirror [`ActivationKind`] exactly, and deliberately stop there:
/// there is no secret variant and no nested map, so an activation is always a
/// flat list of values a screen definition could legitimately have declared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivationValue {
    /// A boolean that must be present.
    Boolean(bool),
    /// A boolean that may be absent.
    OptionalBoolean(Option<bool>),
    /// Free text.
    Text(String),
    /// A signed integer.
    Integer(i64),
    /// One member of the field's declared permitted set.
    Enumerated(String),
    /// A filesystem path.
    Path(PathBuf),
    /// A list of strings.
    TextList(Vec<String>),
}

impl ActivationValue {
    /// The stable text naming this value's kind, matching
    /// [`ActivationKind::as_str`].
    #[must_use]
    pub const fn kind_name(&self) -> &'static str {
        match self {
            Self::Boolean(_) => "boolean",
            Self::OptionalBoolean(_) => "optional-boolean",
            Self::Text(_) => "string",
            Self::Integer(_) => "integer",
            Self::Enumerated(_) => "enum",
            Self::Path(_) => "path",
            Self::TextList(_) => "string-list",
        }
    }

    /// Whether this value satisfies `kind`, ignoring the permitted set.
    ///
    /// Enumerated membership is checked separately so a value of the right
    /// kind but the wrong member reports why it was refused rather than
    /// claiming a type mismatch.
    #[must_use]
    fn has_kind(&self, kind: &ActivationKind) -> bool {
        matches!(
            (self, kind),
            (Self::Boolean(_), ActivationKind::Boolean)
                | (Self::OptionalBoolean(_), ActivationKind::OptionalBoolean)
                | (Self::Text(_), ActivationKind::Text)
                | (Self::Integer(_), ActivationKind::Integer)
                | (Self::Enumerated(_), ActivationKind::Enumerated { .. })
                | (Self::Path(_), ActivationKind::Path)
                | (Self::TextList(_), ActivationKind::TextList)
        )
    }

    /// Deterministic serialized byte cost of this value.
    ///
    /// The bound exists to keep an activation small enough to carry, compare,
    /// and log, so the cost is the payload bytes plus a fixed per-value
    /// overhead rather than the exact width of any particular encoding.
    #[must_use]
    fn serialized_len(&self) -> usize {
        const SCALAR_COST: usize = 8;
        match self {
            Self::Boolean(_) | Self::OptionalBoolean(_) | Self::Integer(_) => SCALAR_COST,
            Self::Text(value) | Self::Enumerated(value) => value.len(),
            Self::Path(value) => value.as_os_str().len(),
            Self::TextList(values) => values
                .iter()
                .map(|value| value.len().saturating_add(1))
                .sum(),
        }
    }
}

/// The bounded, deterministically ordered values one activation carries.
///
/// Keyed by field name, so a caller cannot supply two values for one declared
/// field. Both bounds are enforced here rather than at validation time,
/// because an over-large activation must be impossible to hold at all — not
/// merely impossible to navigate with.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActivationValues(BTreeMap<Id, ActivationValue>);

impl ActivationValues {
    /// Build bounded activation values.
    ///
    /// Later entries for the same field name replace earlier ones.
    ///
    /// # Errors
    ///
    /// Returns [`ActivationError::TooManyFields`] beyond
    /// [`MAX_ACTIVATION_FIELDS`] distinct fields, or
    /// [`ActivationError::TooLarge`] beyond [`MAX_ACTIVATION_BYTES`] serialized
    /// bytes.
    pub fn new(
        entries: impl IntoIterator<Item = (Id, ActivationValue)>,
    ) -> Result<Self, ActivationError> {
        let values: BTreeMap<Id, ActivationValue> = entries.into_iter().collect();
        if values.len() > MAX_ACTIVATION_FIELDS {
            return Err(ActivationError::TooManyFields {
                count: values.len(),
            });
        }
        let bytes = values.iter().fold(0usize, |total, (name, value)| {
            total
                .saturating_add(name.as_str().len())
                .saturating_add(value.serialized_len())
        });
        if bytes > MAX_ACTIVATION_BYTES {
            return Err(ActivationError::TooLarge { bytes });
        }
        Ok(Self(values))
    }

    /// The activation that carries nothing, which every compiled screen accepts.
    #[must_use]
    pub fn empty() -> Self {
        Self(BTreeMap::new())
    }

    /// The value supplied for `field`, if one was.
    #[must_use]
    pub fn get(&self, field: &Id) -> Option<&ActivationValue> {
        self.0.get(field)
    }

    /// How many fields carry a value.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether no field carries a value.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Iterate the supplied fields in field-name order.
    pub fn iter(&self) -> impl Iterator<Item = (&Id, &ActivationValue)> {
        self.0.iter()
    }
}

/// What a screen's route accepts, and which screen it reaches.
///
/// Built from a descriptor rather than declared separately, so a screen and the
/// route that reaches it cannot drift apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteDeclaration {
    /// The route identifier callers name.
    pub id: RouteId,
    /// The fields this route accepts, in declaration order.
    pub activation_schema: Vec<ActivationField>,
    /// The screen this route reaches.
    pub target_screen: ScreenIdentity,
}

impl RouteDeclaration {
    /// Check `values` against this route's declared schema.
    ///
    /// Declared fields are checked in declaration order, then any supplied
    /// field the schema does not declare is refused. Nothing is mutated and
    /// nothing is coerced: a value either satisfies its declared kind or the
    /// whole activation is refused.
    ///
    /// # Errors
    ///
    /// Returns the first categorized reason the activation does not satisfy the
    /// schema.
    pub fn validate(&self, values: &ActivationValues) -> Result<(), ActivationError> {
        for declared in &self.activation_schema {
            let Some(value) = values.get(&declared.name) else {
                return Err(ActivationError::MissingField {
                    field: declared.name.clone(),
                });
            };
            if !value.has_kind(&declared.kind) {
                return Err(ActivationError::WrongKind {
                    field: declared.name.clone(),
                    expected: declared.kind.as_str(),
                    actual: value.kind_name(),
                });
            }
            if let (ActivationKind::Enumerated { permitted }, ActivationValue::Enumerated(supplied)) =
                (&declared.kind, value)
                && !permitted.iter().any(|member| member == supplied)
            {
                return Err(ActivationError::NotPermitted {
                    field: declared.name.clone(),
                });
            }
        }
        for (name, _) in values.iter() {
            if !self
                .activation_schema
                .iter()
                .any(|declared| &declared.name == name)
            {
                return Err(ActivationError::UnknownField {
                    field: name.clone(),
                });
            }
        }
        Ok(())
    }
}

/// Resolve the declaration for `route` from `registry`.
///
/// A route exists only because a descriptor declares it, so this is the only
/// way to obtain a [`RouteDeclaration`] for a route named at runtime.
///
/// # Errors
///
/// Returns [`ActivationError::UnknownRoute`] when no descriptor declares
/// `route`.
pub fn route_declaration(
    registry: &ScreenRegistry,
    route: RouteId,
) -> Result<RouteDeclaration, ActivationError> {
    registry
        .screens()
        .iter()
        .find(|descriptor| descriptor.route == route)
        .map(|descriptor| RouteDeclaration {
            id: descriptor.route,
            activation_schema: descriptor.activation.clone(),
            target_screen: descriptor.id,
        })
        .ok_or(ActivationError::UnknownRoute { route })
}

/// Categorized reason a navigation request was refused.
///
/// Every variant names identifiers the program itself declared — a route, a
/// field, a kind, or a bound. None of them carries a value the caller supplied,
/// so a refusal is safe to render anywhere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivationError {
    /// No descriptor declares the named route.
    UnknownRoute {
        /// The route that resolved to nothing.
        route: RouteId,
    },
    /// A value was supplied for a field the route does not declare.
    UnknownField {
        /// The undeclared field.
        field: Id,
    },
    /// No value was supplied for a field the route declares.
    MissingField {
        /// The field with no value.
        field: Id,
    },
    /// A value did not satisfy its declared kind.
    WrongKind {
        /// The field whose value was refused.
        field: Id,
        /// The kind the route declared.
        expected: &'static str,
        /// The kind the value carried.
        actual: &'static str,
    },
    /// An enumerated value is not in its field's permitted set.
    NotPermitted {
        /// The field whose value is not a permitted member.
        field: Id,
    },
    /// More than [`MAX_ACTIVATION_FIELDS`] distinct fields were supplied.
    TooManyFields {
        /// The number of distinct fields supplied.
        count: usize,
    },
    /// The serialized activation exceeds [`MAX_ACTIVATION_BYTES`].
    TooLarge {
        /// The serialized size that was refused.
        bytes: usize,
    },
}

impl ActivationError {
    /// The coded diagnostic this refusal reports.
    #[must_use]
    pub const fn code(&self) -> NavCode {
        NavCode::E001
    }
}

impl fmt::Display for ActivationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: ", self.code())?;
        match self {
            Self::UnknownRoute { route } => write!(formatter, "no screen declares route '{route}'"),
            Self::UnknownField { field } => {
                write!(formatter, "the route does not declare field '{field}'")
            }
            Self::MissingField { field } => {
                write!(formatter, "the route requires field '{field}'")
            }
            Self::WrongKind {
                field,
                expected,
                actual,
            } => write!(
                formatter,
                "field '{field}' is declared {expected} but a {actual} was supplied"
            ),
            Self::NotPermitted { field } => write!(
                formatter,
                "field '{field}' was given a value outside its permitted set"
            ),
            Self::TooManyFields { count } => write!(
                formatter,
                "an activation carries at most {MAX_ACTIVATION_FIELDS} fields; {count} were supplied"
            ),
            Self::TooLarge { bytes } => write!(
                formatter,
                "an activation serializes to at most {MAX_ACTIVATION_BYTES} bytes; {bytes} were supplied"
            ),
        }
    }
}

impl std::error::Error for ActivationError {}
