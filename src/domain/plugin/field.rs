//! Configuration and argument field declarations
//! (issue #389 CW-09, acceptance rows D5 and D6).
//!
//! A [`Field`] is validated on construction, so an inconsistent declaration —
//! an enum with no choices, a default outside its own bounds, a secret with a
//! literal default — is unrepresentable rather than something later layers must
//! re-check.
//!
//! Cross-field concerns stay out. A `visible_when` reference is recorded here
//! and resolved by manifest validation, which is the only layer that can see
//! the sibling set and the visibility graph.

use std::cmp::Ordering;
use std::fmt;

use super::limits::FIELD_CHOICE_LIMIT;
use crate::domain::{CanonicalDecimal, Id};

/// A scalar declared by a field: its default, a bound, or an enum choice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scalar {
    /// Boolean literal.
    Bool(bool),
    /// Decimal integer.
    Integer(i64),
    /// Canonical finite decimal.
    Decimal(CanonicalDecimal),
    /// Text literal.
    Text(String),
}

impl Scalar {
    /// Compare two numeric scalars by value.
    ///
    /// Comparison is exact decimal arithmetic on the canonical text, never a
    /// float conversion: `f64` has a 52-bit mantissa and would silently
    /// collapse large `i64` bounds onto each other. So `9.5` is below `10.5`
    /// even though it sorts after it lexically, and two `i64` values that
    /// differ only in their low bits still compare as different.
    fn numeric_cmp(&self, other: &Self) -> Option<Ordering> {
        let left = self.numeric_text()?;
        let right = other.numeric_text()?;
        Some(decimal_cmp(&left, &right))
    }

    /// This scalar's canonical decimal text, when it is numeric.
    fn numeric_text(&self) -> Option<String> {
        match self {
            Self::Integer(value) => Some(value.to_string()),
            Self::Decimal(value) => Some(value.as_str().to_owned()),
            Self::Bool(_) | Self::Text(_) => None,
        }
    }

    /// Whether this scalar is a legal value for `kind`.
    fn matches(&self, kind: FieldKind) -> bool {
        match kind {
            FieldKind::Boolean => matches!(self, Self::Bool(_)),
            FieldKind::Integer => matches!(self, Self::Integer(_)),
            FieldKind::FiniteNumber => matches!(self, Self::Integer(_) | Self::Decimal(_)),
            FieldKind::String | FieldKind::Enum | FieldKind::Path | FieldKind::StringList => {
                matches!(self, Self::Text(_))
            }
            // A secret is never a literal, so no scalar is a legal value.
            FieldKind::SecretReference => false,
        }
    }
}

/// Compare two canonical decimal texts by value, exactly.
///
/// Both inputs are canonical: no leading zeros, no trailing fraction zeroes,
/// no exponent, and no `-0`. That is what lets integer parts be compared by
/// digit count first and fraction parts be compared after zero-padding to a
/// common width.
fn decimal_cmp(left: &str, right: &str) -> Ordering {
    let (left_negative, left_magnitude) = split_sign(left);
    let (right_negative, right_magnitude) = split_sign(right);
    match (left_negative, right_negative) {
        (true, false) => return Ordering::Less,
        (false, true) => return Ordering::Greater,
        _ => {}
    }
    let magnitude = magnitude_cmp(left_magnitude, right_magnitude);
    if left_negative {
        magnitude.reverse()
    } else {
        magnitude
    }
}

/// Split a canonical decimal into its sign and unsigned magnitude.
fn split_sign(value: &str) -> (bool, &str) {
    value
        .strip_prefix('-')
        .map_or((false, value), |magnitude| (true, magnitude))
}

/// Compare two unsigned canonical decimal magnitudes.
fn magnitude_cmp(left: &str, right: &str) -> Ordering {
    let (left_integer, left_fraction) = left.split_once('.').unwrap_or((left, ""));
    let (right_integer, right_fraction) = right.split_once('.').unwrap_or((right, ""));
    left_integer
        .len()
        .cmp(&right_integer.len())
        .then_with(|| left_integer.cmp(right_integer))
        .then_with(|| fraction_cmp(left_fraction, right_fraction))
}

/// Compare two fraction digit strings by padding to a common width.
fn fraction_cmp(left: &str, right: &str) -> Ordering {
    let width = left.len().max(right.len());
    let pad = |digits: &str| format!("{digits:0<width$}");
    pad(left).cmp(&pad(right))
}

/// What a field holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FieldKind {
    /// Boolean.
    Boolean,
    /// Free text.
    String,
    /// Decimal integer, optionally bounded.
    Integer,
    /// Canonical finite decimal, optionally bounded.
    FiniteNumber,
    /// One of the declared choices.
    Enum,
    /// A filesystem path.
    Path,
    /// A list of text values.
    StringList,
    /// The name of an environment variable holding a secret.
    SecretReference,
}

impl FieldKind {
    /// Every kind, in declaration order.
    pub const ALL: [Self; 8] = [
        Self::Boolean,
        Self::String,
        Self::Integer,
        Self::FiniteNumber,
        Self::Enum,
        Self::Path,
        Self::StringList,
        Self::SecretReference,
    ];

    /// The lower-kebab-case name used on the wire.
    #[must_use]
    pub const fn as_wire(self) -> &'static str {
        match self {
            Self::Boolean => "boolean",
            Self::String => "string",
            Self::Integer => "integer",
            Self::FiniteNumber => "finite-number",
            Self::Enum => "enum",
            Self::Path => "path",
            Self::StringList => "string-list",
            Self::SecretReference => "secret-reference",
        }
    }

    /// Resolve a wire name, exactly and case-sensitively.
    #[must_use]
    pub fn from_wire(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_wire() == value)
    }

    /// Whether this kind may declare numeric bounds.
    #[must_use]
    pub const fn is_numeric(self) -> bool {
        matches!(self, Self::Integer | Self::FiniteNumber)
    }
}

/// What must restart before a changed value takes effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RestartScope {
    /// The change applies immediately.
    None,
    /// The plugin's provider must restart.
    Provider,
    /// Jefe itself must restart.
    Host,
}

impl RestartScope {
    /// Every scope, in increasing disruption.
    pub const ALL: [Self; 3] = [Self::None, Self::Provider, Self::Host];

    /// The lower-kebab-case name used on the wire.
    #[must_use]
    pub const fn as_wire(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Provider => "provider",
            Self::Host => "host",
        }
    }

    /// Resolve a wire name, exactly and case-sensitively.
    #[must_use]
    pub fn from_wire(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|scope| scope.as_wire() == value)
    }
}

/// An unvalidated field declaration, as read from a manifest.
///
/// [`Field::parse`] is the only way to turn one into a [`Field`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldDraft {
    /// Field identifier, unique within its owner.
    pub id: Id,
    /// What the field holds.
    pub kind: FieldKind,
    /// Whether a value must be supplied.
    pub required: bool,
    /// Default value, if any.
    pub default: Option<Scalar>,
    /// Inclusive lower bound, numeric kinds only.
    pub minimum: Option<Scalar>,
    /// Inclusive upper bound, numeric kinds only.
    pub maximum: Option<Scalar>,
    /// Choices, enum kind only.
    pub choices: Vec<Scalar>,
    /// Sibling field whose value gates this field's visibility.
    pub visible_when: Option<Id>,
    /// What must restart for a change to take effect.
    pub restart: RestartScope,
}

/// A validated field declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    draft: FieldDraft,
}

impl Field {
    /// Validate a field declaration.
    ///
    /// # Errors
    ///
    /// Returns [`FieldError`] when choices, bounds, or the default are
    /// inconsistent with the declared kind or with each other.
    pub fn parse(draft: FieldDraft) -> Result<Self, FieldError> {
        validate_choices(&draft)?;
        validate_bounds(&draft)?;
        validate_default(&draft)?;
        if draft.visible_when.as_ref() == Some(&draft.id) {
            return Err(FieldError::SelfVisibility);
        }
        Ok(Self { draft })
    }

    /// The field identifier.
    #[must_use]
    pub const fn id(&self) -> &Id {
        &self.draft.id
    }

    /// What the field holds.
    #[must_use]
    pub const fn kind(&self) -> FieldKind {
        self.draft.kind
    }

    /// Whether a value must be supplied.
    #[must_use]
    pub const fn required(&self) -> bool {
        self.draft.required
    }

    /// The declared default, if any.
    #[must_use]
    pub const fn default(&self) -> Option<&Scalar> {
        self.draft.default.as_ref()
    }

    /// The declared choices, empty for every kind but `enum`.
    #[must_use]
    pub fn choices(&self) -> &[Scalar] {
        &self.draft.choices
    }

    /// The sibling that gates this field's visibility, if any.
    ///
    /// The reference is recorded, not resolved; manifest validation resolves it
    /// against the sibling set and checks the graph for cycles.
    #[must_use]
    pub const fn visible_when(&self) -> Option<&Id> {
        self.draft.visible_when.as_ref()
    }

    /// What must restart for a change to take effect.
    #[must_use]
    pub const fn restart(&self) -> RestartScope {
        self.draft.restart
    }
}

fn validate_choices(draft: &FieldDraft) -> Result<(), FieldError> {
    if draft.kind == FieldKind::Enum {
        if draft.choices.is_empty() {
            return Err(FieldError::EnumWithoutChoices);
        }
    } else if !draft.choices.is_empty() {
        return Err(FieldError::ChoicesOnNonEnum);
    }
    if draft.choices.len() > FIELD_CHOICE_LIMIT {
        return Err(FieldError::TooManyChoices {
            len: draft.choices.len(),
        });
    }
    for (index, choice) in draft.choices.iter().enumerate() {
        if draft.choices[..index].contains(choice) {
            return Err(FieldError::DuplicateChoice);
        }
    }
    Ok(())
}

fn validate_bounds(draft: &FieldDraft) -> Result<(), FieldError> {
    let declared = draft.minimum.as_ref().or(draft.maximum.as_ref());
    if declared.is_some() && !draft.kind.is_numeric() {
        return Err(FieldError::BoundsOnNonNumeric);
    }
    for bound in [&draft.minimum, &draft.maximum].into_iter().flatten() {
        if !bound.matches(draft.kind) {
            return Err(FieldError::BoundKindMismatch);
        }
    }
    if let (Some(minimum), Some(maximum)) = (&draft.minimum, &draft.maximum)
        && minimum.numeric_cmp(maximum) != Some(Ordering::Less)
        && minimum.numeric_cmp(maximum) != Some(Ordering::Equal)
    {
        return Err(FieldError::InvertedBounds);
    }
    Ok(())
}

fn validate_default(draft: &FieldDraft) -> Result<(), FieldError> {
    let Some(default) = &draft.default else {
        return Ok(());
    };
    if draft.kind == FieldKind::SecretReference {
        return Err(FieldError::SecretDefault);
    }
    if !default.matches(draft.kind) {
        return Err(FieldError::DefaultKindMismatch);
    }
    if draft.kind == FieldKind::Enum && !draft.choices.contains(default) {
        return Err(FieldError::DefaultNotAChoice);
    }
    let below = draft
        .minimum
        .as_ref()
        .is_some_and(|minimum| default.numeric_cmp(minimum) == Some(Ordering::Less));
    let above = draft
        .maximum
        .as_ref()
        .is_some_and(|maximum| default.numeric_cmp(maximum) == Some(Ordering::Greater));
    if below || above {
        return Err(FieldError::DefaultOutOfBounds);
    }
    Ok(())
}

/// Why a field declaration is invalid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldError {
    /// An `enum` field declared no choices.
    EnumWithoutChoices,
    /// A non-`enum` field declared choices.
    ChoicesOnNonEnum,
    /// More than [`FIELD_CHOICE_LIMIT`] choices.
    TooManyChoices { len: usize },
    /// The same choice was declared twice.
    DuplicateChoice,
    /// A non-numeric field declared a bound.
    BoundsOnNonNumeric,
    /// A bound's type does not match the field kind.
    BoundKindMismatch,
    /// The minimum exceeds the maximum.
    InvertedBounds,
    /// The default's type does not match the field kind.
    DefaultKindMismatch,
    /// An `enum` default is not one of its choices.
    DefaultNotAChoice,
    /// The default falls outside the declared bounds.
    DefaultOutOfBounds,
    /// A secret-reference field declared a literal default.
    SecretDefault,
    /// A field referenced itself for visibility.
    SelfVisibility,
}

impl fmt::Display for FieldError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EnumWithoutChoices => formatter.write_str("an enum field declares no choices"),
            Self::ChoicesOnNonEnum => formatter.write_str("only an enum field may declare choices"),
            Self::TooManyChoices { len } => {
                write!(
                    formatter,
                    "{len} choices exceeds the {FIELD_CHOICE_LIMIT} limit"
                )
            }
            Self::DuplicateChoice => formatter.write_str("a choice is declared twice"),
            Self::BoundsOnNonNumeric => {
                formatter.write_str("only a numeric field may declare bounds")
            }
            Self::BoundKindMismatch => formatter.write_str("a bound does not match the field kind"),
            Self::InvertedBounds => formatter.write_str("the minimum exceeds the maximum"),
            Self::DefaultKindMismatch => {
                formatter.write_str("the default does not match the field kind")
            }
            Self::DefaultNotAChoice => {
                formatter.write_str("the default is not one of the declared choices")
            }
            Self::DefaultOutOfBounds => {
                formatter.write_str("the default falls outside the declared bounds")
            }
            Self::SecretDefault => {
                formatter.write_str("a secret reference may not declare a literal default")
            }
            Self::SelfVisibility => formatter.write_str("a field may not gate its own visibility"),
        }
    }
}

impl std::error::Error for FieldError {}

#[cfg(test)]
#[path = "field_tests.rs"]
mod tests;
