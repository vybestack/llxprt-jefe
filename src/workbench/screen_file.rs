//! The closed external screen-definition syntax (issue #385, CW05-02).
//!
//! These types are the *only* shape a `.screen.toml` file may take, and they
//! exist solely to be lowered away: nothing here survives into the composed
//! registry. Keeping the external syntax in its own closed vocabulary is what
//! lets the internal descriptor stay a private contract that can change without
//! breaking a user's file, and what lets every rejection be attributed to a
//! span in the text the user actually wrote.
//!
//! Three properties make the syntax closed:
//!
//! - every object denies unknown fields, so a typo is a rejection rather than a
//!   silently ignored setting;
//! - every enumerated value is a Rust enum, so `direction = "in"` and
//!   `type = "secret"` fail at the same place `axis = "diagonal"` does;
//! - nothing is optional that carries meaning. `focusable`, `required`, and
//!   `collapsible` must be written, because a defaulted `false` would let the
//!   lowerer invent behavior the file never asked for.
//!
//! Duplicate keys are rejected by the TOML parser itself, before any of this is
//! reached.

use serde::Deserialize;
use toml::Spanned;

use crate::domain::ByteSpan;

use super::screen_file_bounds::{ScreenSyntaxError, ScreenSyntaxReason, check_document_bounds};
use super::screen_file_shape::check_shape;

/// The oldest screen-file schema retained for compatibility.
pub const LEGACY_SCREEN_SCHEMA: u32 = 1;
/// The current screen-file schema understood by this build.
pub const SCREEN_SCHEMA: u32 = 2;

/// A parsed screen definition file.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScreenFile {
    /// Declared syntax version; must be supported by this build.
    pub screen_schema: u32,
    /// The screen's `local.<member>` identity.
    pub id: Spanned<String>,
    /// Human-readable title.
    pub title: String,
    /// Navigation route the screen declares.
    pub route: String,
    /// Immutable typed-resource schemas owned by this definition.
    #[serde(default)]
    pub resources: Vec<Spanned<ResourceFile>>,
    /// Configuration fields the screen's owner publishes.
    #[serde(default)]
    pub activation: Vec<Spanned<ActivationField>>,
    /// Panel focused when the screen is instantiated.
    pub initial_focus: String,
    /// Focus cycle order.
    pub focus_order: Vec<String>,
    /// Every panel the screen declares.
    pub panels: Vec<Spanned<PanelFile>>,
    /// Root of the layout tree.
    pub layout: LayoutFile,
    /// Typed same-screen port relationships.
    #[serde(default)]
    pub relationships: Vec<Spanned<RelationshipFile>>,
    /// Closed host-owned layers this screen may open.
    #[serde(default)]
    pub overlays: Vec<Spanned<OverlayFile>>,
    /// Action bindings the screen requests.
    #[serde(default)]
    pub bindings: Vec<Spanned<BindingRefFile>>,
}

/// One immutable typed-resource schema owned by this definition.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceFile {
    /// Unversioned resource type identity.
    pub type_id: String,
    /// Positive resource schema version.
    pub schema_version: u64,
    /// Field whose canonical value is the resource's semantic identity.
    pub semantic_key: String,
    /// Closed fields carried by every resource snapshot.
    pub fields: Vec<Spanned<ResourceFieldFile>>,
}

/// One exact field in a definition-owned resource schema.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceFieldFile {
    /// Field identity within the resource.
    pub id: String,
    /// Operator-facing label.
    pub label: String,
    /// Exact value kind.
    #[serde(rename = "type")]
    pub kind: ResourceFieldKind,
    /// Whether every snapshot must carry this field.
    pub required: bool,
}

/// Closed value kinds admitted by definition-owned resources.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResourceFieldKind {
    /// Boolean.
    Boolean,
    /// Free text.
    String,
    /// Signed integer.
    Integer,
    /// Canonical finite decimal.
    FiniteNumber,
    /// Filesystem path text.
    Path,
    /// List of text values.
    StringList,
}

/// One configuration field the screen's owner publishes.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationField {
    /// Field name within the owner's namespace.
    pub name: String,
    /// Closed value kind.
    #[serde(rename = "type")]
    pub kind: ActivationKind,
    /// Permitted values. Present exactly for [`ActivationKind::Enum`], so an
    /// absent list and an explicitly empty one stay distinguishable.
    pub values: Option<Vec<String>>,
}

/// The closed set of activation field kinds.
///
/// There is deliberately no secret kind. A screen definition is a plain file in
/// a directory a user can share, so a syntax that could *name* a secret field
/// would invite secret values into it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActivationKind {
    /// A required boolean.
    Boolean,
    /// A boolean that may be absent.
    OptionalBoolean,
    /// Free text.
    String,
    /// A signed integer.
    Integer,
    /// One of a declared set of strings.
    Enum,
    /// A filesystem path.
    Path,
    /// A list of strings.
    StringList,
}

/// One panel declaration.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PanelFile {
    /// Panel identity within the screen.
    pub id: String,
    /// Panel type, resolved against the immutable panel-type registry.
    #[serde(rename = "type")]
    pub panel_type: String,
    /// Panel configuration values.
    #[serde(default)]
    pub config: toml::value::Table,
    /// Whether the panel participates in the focus cycle.
    pub focusable: bool,
    /// Whether the panel must stay visible.
    pub required: bool,
    /// Typed connection points.
    #[serde(default)]
    pub ports: Vec<Spanned<PortFile>>,
}

/// One typed port declaration.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortFile {
    /// Port identity within the panel.
    pub id: String,
    /// Immutable owner of the named resource schema.
    ///
    /// Schema 1 files may omit this field; lowering applies the closed legacy
    /// owner mapping. Schema 2 requires it explicitly.
    pub owner: Option<String>,
    /// Which way values cross.
    pub direction: PortDirectionFile,
    /// `<name>@<version>` identity of the carried value.
    pub type_id: String,
    /// Whether the panel needs a value here.
    pub required: bool,
    /// Whether the port keeps its last value when its source goes absent.
    pub retained: bool,
}

/// External spelling of a port direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PortDirectionFile {
    /// The panel consumes a value here.
    Input,
    /// The panel publishes a value here.
    Output,
}

/// External spelling of a split axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AxisFile {
    /// Children are placed left to right.
    Horizontal,
    /// Children are placed top to bottom.
    Vertical,
}

/// One layout tree node.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
pub enum LayoutFile {
    /// A single panel.
    Leaf {
        /// The panel occupying this rectangle.
        panel: String,
    },
    /// A division among ordered children.
    Split {
        /// Axis the rectangle divides along.
        axis: AxisFile,
        /// Children in declaration order.
        ///
        /// Children carry no span. Serde buffers the body of an internally
        /// tagged enum before it dispatches on `type`, and a buffered value has
        /// lost the source positions a span needs, so asking for one here fails
        /// the whole parse. Layout violations therefore name the offending
        /// structure rather than its byte range.
        children: Vec<ChildFile>,
    },
}

/// One child of a split node.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChildFile {
    /// The subtree this child occupies.
    pub node: LayoutFile,
    /// How the child claims cells.
    pub size: SizeFile,
    /// Fewest cells while visible.
    pub min: u16,
    /// Most cells, if bounded.
    pub max: Option<u16>,
    /// Whether the resolver may hide this child.
    pub collapsible: bool,
    /// Collapse order key; required exactly when `collapsible`.
    pub collapse_priority: Option<i32>,
}

/// How a child claims cells along its parent's axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SizeFile {
    /// Claim exactly this many cells.
    Fixed(u16),
    /// Claim a share of what is left.
    Weight(u16),
}

/// One typed relationship between two same-screen ports.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum RelationshipFile {
    /// The source narrows what the target operates on.
    Scope {
        /// `<panel>.<port>` output reference.
        source: String,
        /// `<panel>.<port>` input reference.
        target: String,
    },
    /// The source selects the subject the target elaborates.
    MasterDetail {
        /// `<panel>.<port>` output reference.
        source: String,
        /// `<panel>.<port>` input reference.
        target: String,
        /// Whether the target follows the source at once or on an action.
        activation: ActivationModeFile,
        /// What the target shows when the source is absent.
        empty: EmptyPolicyFile,
    },
    /// The source names the session the target attaches to.
    SessionTarget {
        /// `<panel>.<port>` output reference.
        source: String,
        /// `<panel>.<port>` input reference.
        target: String,
        /// What the target does when the source is absent.
        empty: SessionEmptyPolicyFile,
    },
}

/// When a master-detail target follows its source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActivationModeFile {
    /// Follow in the same transition the source changed in.
    Immediate,
    /// Stage the source and follow only on the declared activation action.
    Explicit,
}

/// What a master-detail target shows when its source is absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EmptyPolicyFile {
    /// Clear the target.
    ShowNone,
    /// Set the target to the typed all-value.
    ShowAll,
    /// Leave the target's prior value in place.
    Retain,
}

/// What a session target does when its source is absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SessionEmptyPolicyFile {
    /// Clear the session attachment.
    Detach,
    /// Leave the attachment in place.
    Retain,
}

/// One host-owned overlay declaration.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OverlayFile {
    /// Closed host implementation selected by this declaration.
    pub kind: OverlayKindFile,
}

/// Closed host-owned overlay vocabulary admitted by screen definitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OverlayKindFile {
    /// Keyboard-shortcut reference content.
    Help,
    /// Host text-query editor.
    Search,
    /// Host yes/no confirmation surface.
    Confirmation,
}

/// One action binding the screen requests.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BindingRefFile {
    /// Input context the binding applies in.
    pub context: String,
    /// Action the binding invokes.
    pub action: String,
}

/// Parse one screen definition file.
///
/// The text is walked twice on purpose. Deserializing from the text is what
/// makes byte spans available, because spans exist only while the parser can
/// still see the source; deserializing from a materialized value would lose
/// them and leave every diagnostic pointing at the file as a whole. The generic
/// value bounds, however, are easiest to measure over that materialized value,
/// so the document is parsed once as data and once as this syntax. Both passes
/// are bounded by the file-size cap discovery already applied.
///
/// Reporting order runs from the most specific rule to the least: a layout that
/// nests too deep is reported as a layout depth violation rather than as generic
/// document depth, because the author needs to know which structure to flatten.
///
/// # Errors
///
/// Returns the first violated syntax rule with the span of the offending text
/// where the parser can attribute one.
pub fn parse_screen_file(text: &str) -> Result<ScreenFile, ScreenSyntaxError> {
    let document: toml::Table = text.parse().map_err(malformed)?;
    // The declared version is read first, so a file written for a schema this
    // build does not implement is reported as that rather than as whichever
    // field the newer schema happens to spell differently.
    check_schema(&document)?;
    let file: ScreenFile = toml::from_str(text).map_err(malformed)?;
    check_shape(&file)?;
    check_document_bounds(&document)?;
    Ok(file)
}

/// Check the declared schema version before anything else is interpreted.
fn check_schema(document: &toml::Table) -> Result<(), ScreenSyntaxError> {
    let declared = document
        .get("screen_schema")
        .and_then(toml::Value::as_integer)
        .and_then(|value| u32::try_from(value).ok());
    // An absent or non-integer `screen_schema` is an ordinary shape error and
    // is left to the typed parse, which can attribute a span to it.
    match declared {
        Some(found) if !(LEGACY_SCREEN_SCHEMA..=SCREEN_SCHEMA).contains(&found) => Err(
            ScreenSyntaxError::unspanned(ScreenSyntaxReason::UnsupportedSchema { found }),
        ),
        _ => Ok(()),
    }
}

/// Convert a TOML deserialization failure into a span-bearing syntax error.
///
/// The parser's message names the offending key and the expected type, which is
/// what an author needs, but for a type mismatch it also quotes the value that
/// was found. Definition files are not secret-bearing, but a title or a config
/// string is still the user's text and has no business in a log line, so quoted
/// content is elided and the span carries the location instead.
fn malformed(error: toml::de::Error) -> ScreenSyntaxError {
    let span = error.span().map(|range| {
        ByteSpan::new(
            u64::try_from(range.start).unwrap_or(u64::MAX),
            u64::try_from(range.end).unwrap_or(u64::MAX),
        )
    });
    ScreenSyntaxError {
        reason: ScreenSyntaxReason::Malformed {
            detail: elide_quoted(error.message()),
        },
        span,
    }
}

/// Replace every double-quoted run in `message` with an ellipsis.
///
/// Only the parser's own structural text survives: `invalid type: string "…",
/// expected u32` still says which type was wrong and which was wanted, without
/// repeating what the file said.
fn elide_quoted(message: &str) -> String {
    let mut redacted = String::with_capacity(message.len());
    let mut inside = false;
    for character in message.chars() {
        if character == '"' {
            if !inside {
                redacted.push_str("\"\u{2026}\"");
            }
            inside = !inside;
            continue;
        }
        if !inside {
            redacted.push(character);
        }
    }
    redacted
}

/// Convert a `Spanned` wrapper's byte range into the shared span type.
#[must_use]
pub fn span_of<T>(value: &Spanned<T>) -> ByteSpan {
    ByteSpan::new(
        u64::try_from(value.span().start).unwrap_or(u64::MAX),
        u64::try_from(value.span().end).unwrap_or(u64::MAX),
    )
}
