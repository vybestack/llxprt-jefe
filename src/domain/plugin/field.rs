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
use crate::domain::{CanonicalDecimal, Id, InternalId, TypedValue};

/// Maximum UTF-8 byte length of a plugin path value.
pub const PATH_VALUE_BYTE_LIMIT: usize = 4_096;

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
    pub(crate) fn numeric_cmp(&self, other: &Self) -> Option<Ordering> {
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

    /// Whether this scalar is a legal bound for `kind`.
    ///
    /// Numeric kinds accept numeric bounds; [`FieldKind::String`] and
    /// [`FieldKind::StringList`] accept integer length bounds.
    fn matches_bound(&self, kind: FieldKind) -> bool {
        match kind {
            FieldKind::Integer | FieldKind::String | FieldKind::StringList => {
                matches!(self, Self::Integer(_))
            }
            FieldKind::FiniteNumber => matches!(self, Self::Integer(_) | Self::Decimal(_)),
            FieldKind::Boolean | FieldKind::Enum | FieldKind::Path | FieldKind::SecretReference => {
                false
            }
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

    /// Whether this kind may declare inclusive bounds.
    ///
    /// Numeric kinds carry value bounds; [`Self::String`] and
    /// [`Self::StringList`] carry integer length bounds.
    #[must_use]
    pub const fn allows_bounds(self) -> bool {
        matches!(
            self,
            Self::Integer | Self::FiniteNumber | Self::String | Self::StringList
        )
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
/// Provider declarations become [`Field`] values only through [`Field::parse`];
/// closed host-internal fields use [`Field::internal`] instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldDraft {
    /// Field identifier, unique within its owner.
    pub id: Id,
    /// Operator-facing label.
    pub label: String,
    /// Longer description, if any.
    pub description: Option<String>,
    /// What the field holds.
    pub kind: FieldKind,
    /// Whether a value must be supplied.
    pub required: bool,
    /// Default value, if any.
    pub default: Option<TypedValue>,
    /// Inclusive lower bound (numeric value or string/list length).
    pub min: Option<Scalar>,
    /// Inclusive upper bound (numeric value or string/list length).
    pub max: Option<Scalar>,
    /// Choices, enum kind only.
    pub choices: Vec<Scalar>,
    /// Whether list entries must be distinct (`string-list` only).
    pub unique: bool,
    /// Sibling field whose value gates this field's visibility.
    pub visible_when: Option<Id>,
    /// What must restart for a change to take effect.
    pub restart: RestartScope,
}

/// Closed host-internal fields whose complete declarations are fixed at compile time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InternalField {
    SearchQuery,
    ConfirmationDecision,
    DeleteWorkDir,
    RepositoryFormName,
    RepositoryFormBaseDir,
    RepositoryFormDefaultProfile,
    RepositoryFormDefaultModel,
    RepositoryFormDefaultYolo,
    RepositoryFormDefaultAgentType,
    RepositoryFormDefaultVersion,
    RepositoryFormDefaultMode,
    RepositoryFormDefaultLlxprtVersion,
    RepositoryFormGithubRepo,
    RepositoryFormIssuePrRepo,
    RepositoryFormRemoteEnabled,
    RepositoryFormLoginUser,
    RepositoryFormHost,
    RepositoryFormSshPort,
    RepositoryFormIdentityFile,
    RepositoryFormSshOptions,
    RepositoryFormRunAsUser,
    RepositoryFormSetupEnvDefault,
    RepositoryFormTransientAgentDir,
    RepositoryFormTransientMaxConcurrent,
    AgentFormShortcut,
    AgentFormName,
    AgentFormDescription,
    AgentFormWorkDir,
    AgentFormProfile,
    AgentFormAgentType,
    AgentFormModel,
    AgentFormVersion,
    AgentFormYolo,
    AgentFormQuickResume,
    AgentFormMode,
    AgentFormLlxprtVersion,
    AgentFormLlxprtDebug,
    AgentFormPassContinue,
    AgentFormSandbox,
    AgentFormSandboxEngine,
    AgentFormSandboxFlags,
}

/// A validated field declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    draft: FieldDraft,
}

impl InternalField {
    fn id(self) -> InternalId {
        match self {
            Self::SearchQuery => InternalId::OverlayQuery,
            Self::ConfirmationDecision => InternalId::OverlayDecision,
            Self::DeleteWorkDir => InternalId::OverlayDeleteWorkDir,
            Self::RepositoryFormName => InternalId::RepositoryFormName,
            Self::RepositoryFormBaseDir => InternalId::RepositoryFormBaseDir,
            Self::RepositoryFormDefaultProfile => InternalId::RepositoryFormDefaultProfile,
            Self::RepositoryFormDefaultModel => InternalId::RepositoryFormDefaultModel,
            Self::RepositoryFormDefaultYolo => InternalId::RepositoryFormDefaultYolo,
            Self::RepositoryFormDefaultAgentType => InternalId::RepositoryFormDefaultAgentType,
            Self::RepositoryFormDefaultVersion => InternalId::RepositoryFormDefaultVersion,
            Self::RepositoryFormDefaultMode => InternalId::RepositoryFormDefaultMode,
            Self::RepositoryFormDefaultLlxprtVersion => {
                InternalId::RepositoryFormDefaultLlxprtVersion
            }
            Self::RepositoryFormGithubRepo => InternalId::RepositoryFormGithubRepo,
            Self::RepositoryFormIssuePrRepo => InternalId::RepositoryFormIssuePrRepo,
            Self::RepositoryFormRemoteEnabled => InternalId::RepositoryFormRemoteEnabled,
            Self::RepositoryFormLoginUser => InternalId::RepositoryFormLoginUser,
            Self::RepositoryFormHost => InternalId::RepositoryFormHost,
            Self::RepositoryFormSshPort => InternalId::RepositoryFormSshPort,
            Self::RepositoryFormIdentityFile => InternalId::RepositoryFormIdentityFile,
            Self::RepositoryFormSshOptions => InternalId::RepositoryFormSshOptions,
            Self::RepositoryFormRunAsUser => InternalId::RepositoryFormRunAsUser,
            Self::RepositoryFormSetupEnvDefault => InternalId::RepositoryFormSetupEnvDefault,
            Self::RepositoryFormTransientAgentDir => InternalId::RepositoryFormTransientAgentDir,
            Self::RepositoryFormTransientMaxConcurrent => {
                InternalId::RepositoryFormTransientMaxConcurrent
            }
            Self::AgentFormShortcut => InternalId::AgentFormShortcut,
            Self::AgentFormName => InternalId::AgentFormName,
            Self::AgentFormDescription => InternalId::AgentFormDescription,
            Self::AgentFormWorkDir => InternalId::AgentFormWorkDir,
            Self::AgentFormProfile => InternalId::AgentFormProfile,
            Self::AgentFormAgentType => InternalId::AgentFormAgentType,
            Self::AgentFormModel => InternalId::AgentFormModel,
            Self::AgentFormVersion => InternalId::AgentFormVersion,
            Self::AgentFormYolo => InternalId::AgentFormYolo,
            Self::AgentFormQuickResume => InternalId::AgentFormQuickResume,
            Self::AgentFormMode => InternalId::AgentFormMode,
            Self::AgentFormLlxprtVersion => InternalId::AgentFormLlxprtVersion,
            Self::AgentFormLlxprtDebug => InternalId::AgentFormLlxprtDebug,
            Self::AgentFormPassContinue => InternalId::AgentFormPassContinue,
            Self::AgentFormSandbox => InternalId::AgentFormSandbox,
            Self::AgentFormSandboxEngine => InternalId::AgentFormSandboxEngine,
            Self::AgentFormSandboxFlags => InternalId::AgentFormSandboxFlags,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::SearchQuery => "Filter",
            Self::ConfirmationDecision => "Decision",
            Self::DeleteWorkDir => "Delete work directory",
            Self::RepositoryFormName | Self::AgentFormName => "Name",
            Self::RepositoryFormBaseDir => "Base Dir",
            Self::RepositoryFormDefaultProfile => "Default Profile",
            Self::RepositoryFormDefaultModel => "Default Model",
            Self::RepositoryFormDefaultYolo => "Default YOLO",
            Self::RepositoryFormDefaultAgentType => "Default Agent",
            Self::RepositoryFormDefaultVersion => "Default CP Version",
            Self::RepositoryFormDefaultMode => "Default Mode",
            Self::RepositoryFormDefaultLlxprtVersion => "Default LLxprt Version",
            Self::RepositoryFormGithubRepo => "GitHub Repo",
            Self::RepositoryFormIssuePrRepo => "Issues / PRs Repo",
            Self::RepositoryFormRemoteEnabled => "Remote Repository",
            Self::RepositoryFormLoginUser => "Login User",
            Self::RepositoryFormHost => "Host / IP",
            Self::RepositoryFormSshPort => "SSH Port",
            Self::RepositoryFormIdentityFile => "Identity File",
            Self::RepositoryFormSshOptions => "SSH Options",
            Self::RepositoryFormRunAsUser => "Run As User",
            Self::RepositoryFormSetupEnvDefault => "Setup Env Default",
            Self::RepositoryFormTransientAgentDir => "Transient Dir",
            Self::RepositoryFormTransientMaxConcurrent => "Max Transient",
            Self::AgentFormShortcut => "Shortcut (1-9)",
            Self::AgentFormDescription => "Description",
            Self::AgentFormWorkDir => "Work Dir",
            Self::AgentFormProfile => "Profile",
            Self::AgentFormAgentType => "Agent Runtime",
            Self::AgentFormModel => "Model",
            Self::AgentFormVersion => "CP Version",
            Self::AgentFormYolo => "YOLO",
            Self::AgentFormQuickResume => "Quick resume",
            Self::AgentFormMode => "Mode Flags",
            Self::AgentFormLlxprtVersion => "LLxprt Version",
            Self::AgentFormLlxprtDebug => "LLXPRT_DEBUG",
            Self::AgentFormPassContinue => "Pass --continue",
            Self::AgentFormSandbox => "Sandbox",
            Self::AgentFormSandboxEngine => "Sandbox Engine",
            Self::AgentFormSandboxFlags => "Sandbox Flags",
        }
    }

    fn kind(self) -> FieldKind {
        match self {
            Self::DeleteWorkDir
            | Self::RepositoryFormDefaultYolo
            | Self::RepositoryFormRemoteEnabled
            | Self::RepositoryFormSetupEnvDefault
            | Self::AgentFormYolo
            | Self::AgentFormQuickResume
            | Self::AgentFormPassContinue
            | Self::AgentFormSandbox => FieldKind::Boolean,
            _ => FieldKind::String,
        }
    }

    fn required(self) -> bool {
        matches!(
            self,
            Self::SearchQuery
                | Self::ConfirmationDecision
                | Self::RepositoryFormName
                | Self::AgentFormName
        )
    }
}

impl Field {
    /// Construct one closed host-internal field declaration.
    pub(crate) fn internal(field: InternalField) -> Self {
        Self {
            draft: FieldDraft {
                id: Id::internal(field.id()),
                label: field.label().to_owned(),
                description: None,
                kind: field.kind(),
                required: field.required(),
                default: None,
                min: None,
                max: None,
                choices: Vec::new(),
                unique: false,
                visible_when: None,
                restart: RestartScope::None,
            },
        }
    }

    /// Validate a field declaration.
    ///
    /// # Errors
    ///
    /// Returns [`FieldError`] when the label is blank, choices, bounds, or the
    /// default are inconsistent with the declared kind or with each other, or
    /// `unique` is declared on a non-list kind.
    pub fn parse(draft: FieldDraft) -> Result<Self, FieldError> {
        if draft.label.trim().is_empty() {
            return Err(FieldError::BlankLabel);
        }
        validate_choices(&draft)?;
        validate_bounds(&draft)?;
        validate_unique(&draft)?;
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

    /// The operator-facing label.
    #[must_use]
    pub fn label(&self) -> &str {
        &self.draft.label
    }

    /// The longer description, if any.
    #[must_use]
    pub fn description(&self) -> Option<&str> {
        self.draft.description.as_deref()
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
    pub const fn default(&self) -> Option<&TypedValue> {
        self.draft.default.as_ref()
    }

    /// The declared inclusive lower bound, if any.
    #[must_use]
    pub const fn min(&self) -> Option<&Scalar> {
        self.draft.min.as_ref()
    }

    /// The declared inclusive upper bound, if any.
    #[must_use]
    pub const fn max(&self) -> Option<&Scalar> {
        self.draft.max.as_ref()
    }

    /// The declared choices, empty for every kind but `enum`.
    #[must_use]
    pub fn choices(&self) -> &[Scalar] {
        &self.draft.choices
    }

    /// Whether list entries must be distinct.
    #[must_use]
    pub const fn unique(&self) -> bool {
        self.draft.unique
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
    let declared = draft.min.as_ref().or(draft.max.as_ref());
    if declared.is_some() && !draft.kind.allows_bounds() {
        return Err(FieldError::BoundsOnUnsupportedKind);
    }
    for bound in [&draft.min, &draft.max].into_iter().flatten() {
        if !bound.matches_bound(draft.kind) {
            return Err(FieldError::BoundKindMismatch);
        }
    }
    if let (Some(min), Some(max)) = (&draft.min, &draft.max)
        && min.numeric_cmp(max) == Some(Ordering::Greater)
    {
        return Err(FieldError::InvertedBounds);
    }
    Ok(())
}

/// `unique` is legal only for `string-list`.
fn validate_unique(draft: &FieldDraft) -> Result<(), FieldError> {
    if draft.unique && draft.kind != FieldKind::StringList {
        return Err(FieldError::UniqueOnNonList);
    }
    Ok(())
}

fn validate_default(draft: &FieldDraft) -> Result<(), FieldError> {
    let Some(default) = &draft.default else {
        return Ok(());
    };
    if !typed_value_matches_kind(draft.kind, default) {
        return Err(FieldError::DefaultKindMismatch);
    }
    if draft.kind == FieldKind::Path
        && matches!(default, TypedValue::String(value) if value.len() > PATH_VALUE_BYTE_LIMIT)
    {
        return Err(FieldError::DefaultOutOfBounds);
    }
    if draft.unique
        && let TypedValue::List(values) = default
        && values
            .iter()
            .enumerate()
            .any(|(index, value)| values[..index].contains(value))
    {
        return Err(FieldError::DuplicateDefaultEntry);
    }
    if draft.kind == FieldKind::Enum && !enum_choice_matches(draft, default) {
        return Err(FieldError::DefaultNotAChoice);
    }
    if default_within_bounds(draft, default) {
        return Ok(());
    }
    Err(FieldError::DefaultOutOfBounds)
}

/// Whether a [`TypedValue`] is a legal value for `kind`.
///
/// Each kind accepts exactly one closed typed-value variant, so a wrong-type
/// default is caught at parse time rather than by the runtime validator. A
/// secret-reference default is a reference (`SecretRef`), never a literal
/// value.
fn typed_value_matches_kind(kind: FieldKind, value: &TypedValue) -> bool {
    match (kind, value) {
        (FieldKind::Boolean, TypedValue::Bool(_))
        | (FieldKind::String | FieldKind::Enum | FieldKind::Path, TypedValue::String(_))
        | (FieldKind::Integer, TypedValue::Integer(_))
        | (FieldKind::FiniteNumber, TypedValue::Integer(_) | TypedValue::Decimal(_))
        | (FieldKind::SecretReference, TypedValue::SecretRef(_)) => true,
        (FieldKind::StringList, TypedValue::List(values)) => values
            .iter()
            .all(|value| matches!(value, TypedValue::String(_))),
        _ => false,
    }
}

/// Whether an enum default string is one of the declared text choices.
fn enum_choice_matches(draft: &FieldDraft, default: &TypedValue) -> bool {
    let TypedValue::String(text) = default else {
        return false;
    };
    draft
        .choices
        .iter()
        .any(|choice| matches!(choice, Scalar::Text(choice_text) if choice_text == text))
}

/// Whether the default lies within its declared inclusive bounds.
///
/// Numeric defaults compare by value. String defaults compare their UTF-8 byte
/// length against integer length bounds. List defaults compare their item count
/// against integer length bounds.
fn default_within_bounds(draft: &FieldDraft, default: &TypedValue) -> bool {
    match draft.kind {
        FieldKind::Integer | FieldKind::FiniteNumber => {
            let scalar = match default {
                TypedValue::Integer(value) => Scalar::Integer(*value),
                TypedValue::Decimal(value) => Scalar::Decimal(value.clone()),
                _ => return false,
            };
            let below = draft
                .min
                .as_ref()
                .is_some_and(|min| scalar.numeric_cmp(min) == Some(Ordering::Less));
            let above = draft
                .max
                .as_ref()
                .is_some_and(|max| scalar.numeric_cmp(max) == Some(Ordering::Greater));
            !below && !above
        }
        FieldKind::String => {
            let TypedValue::String(text) = default else {
                return false;
            };
            length_within_bounds(draft, i64::try_from(text.len()).unwrap_or(i64::MAX))
        }
        FieldKind::StringList => {
            let TypedValue::List(values) = default else {
                return false;
            };
            length_within_bounds(draft, i64::try_from(values.len()).unwrap_or(i64::MAX))
        }
        _ => true,
    }
}

/// Whether a measured length falls within integer length bounds.
fn length_within_bounds(draft: &FieldDraft, length: i64) -> bool {
    let below = draft
        .min
        .as_ref()
        .is_some_and(|min| matches!(min, Scalar::Integer(bound) if length < *bound));
    let above = draft
        .max
        .as_ref()
        .is_some_and(|max| matches!(max, Scalar::Integer(bound) if length > *bound));
    !below && !above
}

/// Why a field declaration is invalid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldError {
    /// The label is empty or only whitespace.
    BlankLabel,
    /// An `enum` field declared no choices.
    EnumWithoutChoices,
    /// A non-`enum` field declared choices.
    ChoicesOnNonEnum,
    /// More than [`FIELD_CHOICE_LIMIT`] choices.
    TooManyChoices { len: usize },
    /// The same choice was declared twice.
    DuplicateChoice,
    /// A kind that does not support bounds declared one.
    BoundsOnUnsupportedKind,
    /// A bound's type does not match the field kind.
    BoundKindMismatch,
    /// The minimum exceeds the maximum.
    InvertedBounds,
    /// `unique` was declared on a non-`string-list` kind.
    UniqueOnNonList,
    /// The default's type does not match the field kind.
    DefaultKindMismatch,
    /// An `enum` default is not one of its choices.
    DefaultNotAChoice,
    /// A unique string-list default contains a repeated entry.
    DuplicateDefaultEntry,
    /// The default falls outside the declared bounds.
    DefaultOutOfBounds,
    /// A field referenced itself for visibility.
    SelfVisibility,
}

impl fmt::Display for FieldError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BlankLabel => formatter.write_str("a field label may not be blank"),
            Self::EnumWithoutChoices => formatter.write_str("an enum field declares no choices"),
            Self::ChoicesOnNonEnum => formatter.write_str("only an enum field may declare choices"),
            Self::TooManyChoices { len } => {
                write!(
                    formatter,
                    "{len} choices exceeds the {FIELD_CHOICE_LIMIT} limit"
                )
            }
            Self::DuplicateChoice => formatter.write_str("a choice is declared twice"),
            Self::BoundsOnUnsupportedKind => formatter
                .write_str("only a numeric, string, or string-list field may declare bounds"),
            Self::BoundKindMismatch => formatter.write_str("a bound does not match the field kind"),
            Self::InvertedBounds => formatter.write_str("the minimum exceeds the maximum"),
            Self::UniqueOnNonList => {
                formatter.write_str("only a string-list field may declare unique entries")
            }
            Self::DefaultKindMismatch => {
                formatter.write_str("the default does not match the field kind")
            }
            Self::DefaultNotAChoice => {
                formatter.write_str("the default is not one of the declared choices")
            }
            Self::DuplicateDefaultEntry => {
                formatter.write_str("a unique string-list default contains a duplicate")
            }
            Self::DefaultOutOfBounds => {
                formatter.write_str("the default falls outside the declared bounds")
            }
            Self::SelfVisibility => formatter.write_str("a field may not gate its own visibility"),
        }
    }
}

impl std::error::Error for FieldError {}

#[cfg(test)]
#[path = "field_tests.rs"]
mod tests;
