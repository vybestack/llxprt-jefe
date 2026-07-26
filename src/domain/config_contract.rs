//! Closed, I/O-free configuration value and ownership contracts.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Display, Formatter};
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

macro_rules! impl_string_value {
    ($type:ty, $error:expr) => {
        impl Display for $type {
            fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
                formatter.write_str(self.as_str())
            }
        }

        impl Serialize for $type {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> Deserialize<'de> for $type {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::parse(&value).map_err(|_| serde::de::Error::custom($error))
            }
        }
    };
}

/// Maximum encoded size of a configuration identifier.
pub const ID_BYTE_LIMIT: usize = 128;

/// Error returned when a closed configuration value is invalid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigContractError {
    InvalidId,
    InvalidDecimal,
    InvalidDateTime,
    InvalidSemver,
    DuplicateOwner,
}

impl Display for ConfigContractError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::InvalidId => "invalid configuration identifier",
            Self::InvalidDecimal => "invalid canonical finite decimal",
            Self::InvalidDateTime => "invalid canonical TOML datetime",
            Self::InvalidSemver => "invalid canonical semantic version",
            Self::DuplicateOwner => "duplicate configuration owner",
        })
    }
}

impl std::error::Error for ConfigContractError {}

/// Validated lowercase configuration identifier.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Id(String);

impl Id {
    /// Parse and validate a configuration identifier.
    pub fn parse(value: &str) -> Result<Self, ConfigContractError> {
        if value.len() > ID_BYTE_LIMIT || !valid_id(value.as_bytes()) {
            return Err(ConfigContractError::InvalidId);
        }
        Ok(Self(value.to_owned()))
    }

    /// Borrow the validated identifier text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn valid_id(bytes: &[u8]) -> bool {
    let Some(first) = bytes.first() else {
        return false;
    };
    if !first.is_ascii_lowercase() {
        return false;
    }
    let mut separator = false;
    for byte in &bytes[1..] {
        if byte.is_ascii_lowercase() || byte.is_ascii_digit() {
            separator = false;
        } else if matches!(byte, b'.' | b'-') && !separator {
            separator = true;
        } else {
            return false;
        }
    }
    !separator
}

impl Display for Id {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Serialize for Id {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Id {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

/// Canonical, finite decimal text used where JSON number round-tripping is unsafe.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CanonicalDecimal(String);

impl CanonicalDecimal {
    /// Parse canonical decimal text without exponent notation or trailing zeroes.
    pub fn parse(value: &str) -> Result<Self, ConfigContractError> {
        if !valid_decimal_text(value) || !finite_decimal(value) {
            return Err(ConfigContractError::InvalidDecimal);
        }
        Ok(Self(value.to_owned()))
    }

    /// Borrow the canonical decimal text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn valid_decimal_text(value: &str) -> bool {
    if value == "0" {
        return true;
    }
    let unsigned = value.strip_prefix('-').unwrap_or(value);
    if unsigned.is_empty() || value == "-0" {
        return false;
    }
    let Some((integer, fraction)) = unsigned.split_once('.') else {
        return valid_nonzero_integer(unsigned);
    };
    !fraction.is_empty()
        && !fraction.ends_with('0')
        && fraction.bytes().all(|byte| byte.is_ascii_digit())
        && valid_integer(integer)
}

fn valid_integer(value: &str) -> bool {
    value == "0" || valid_nonzero_integer(value)
}

fn valid_nonzero_integer(value: &str) -> bool {
    value
        .as_bytes()
        .first()
        .is_some_and(|byte| matches!(byte, b'1'..=b'9'))
        && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn finite_decimal(value: &str) -> bool {
    value.parse::<f64>().is_ok_and(f64::is_finite)
}

impl_string_value!(CanonicalDecimal, ConfigContractError::InvalidDecimal);

/// Canonical TOML datetime text.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CanonicalDateTime(String);

impl CanonicalDateTime {
    /// Parse text accepted and emitted identically by the existing TOML authority.
    pub fn parse(value: &str) -> Result<Self, ConfigContractError> {
        let parsed = toml::value::Datetime::from_str(value)
            .map_err(|_| ConfigContractError::InvalidDateTime)?;
        if parsed.to_string() != value {
            return Err(ConfigContractError::InvalidDateTime);
        }
        Ok(Self(value.to_owned()))
    }

    /// Borrow the canonical datetime text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl_string_value!(CanonicalDateTime, ConfigContractError::InvalidDateTime);

/// Reference to secret material owned outside persisted configuration values.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SecretRef {
    /// Identifier resolved only at a secret-owning boundary.
    pub id: Id,
}

/// Closed recursive configuration value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum TypedValue {
    String(String),
    Bool(bool),
    Integer(i64),
    Decimal(CanonicalDecimal),
    Datetime(CanonicalDateTime),
    List(Vec<Self>),
    Map(TypedMap),
    SecretRef(SecretRef),
}

/// Deterministically ordered map of validated configuration values.
pub type TypedMap = BTreeMap<Id, TypedValue>;

/// Semantic version with strict SemVer 2.0.0 syntax.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CanonicalSemver {
    original: String,
    core: [String; 3],
    prerelease: Vec<PrereleaseIdentifier>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum PrereleaseIdentifier {
    Numeric(String),
    Alpha(String),
}

impl CanonicalSemver {
    /// Parse a complete SemVer 2.0.0 value.
    pub fn parse(value: &str) -> Result<Self, ConfigContractError> {
        let (version, build) = split_optional(value, '+', false)?;
        validate_build(build)?;
        let (core, prerelease) = split_optional(version, '-', true)?;
        let core = parse_core(core)?;
        let prerelease = parse_prerelease(prerelease)?;
        Ok(Self {
            original: value.to_owned(),
            core,
            prerelease,
        })
    }

    /// Borrow the canonical semantic-version text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.original
    }

    /// Compare SemVer precedence, excluding build metadata as required by SemVer.
    #[must_use]
    pub fn precedence_cmp(&self, other: &Self) -> Ordering {
        for (left, right) in self.core.iter().zip(&other.core) {
            let ordering = numeric_cmp(left, right);
            if ordering != Ordering::Equal {
                return ordering;
            }
        }
        prerelease_cmp(&self.prerelease, &other.prerelease)
    }
}

/// Split `value` at its first `separator`.
///
/// `repeatable` states whether the separator may appear again in the remainder:
/// build metadata is introduced by a single `+`, while SemVer 2.0.0 permits
/// hyphens inside prerelease identifiers (for example `1.0.0-rc-beta`).
fn split_optional(
    value: &str,
    separator: char,
    repeatable: bool,
) -> Result<(&str, Option<&str>), ConfigContractError> {
    let Some((left, right)) = value.split_once(separator) else {
        return Ok((value, None));
    };
    if right.is_empty() || (!repeatable && right.contains(separator)) {
        return Err(ConfigContractError::InvalidSemver);
    }
    Ok((left, Some(right)))
}

fn parse_core(value: &str) -> Result<[String; 3], ConfigContractError> {
    let mut parts = value.split('.');
    let major = parts.next().ok_or(ConfigContractError::InvalidSemver)?;
    let minor = parts.next().ok_or(ConfigContractError::InvalidSemver)?;
    let patch = parts.next().ok_or(ConfigContractError::InvalidSemver)?;
    if parts.next().is_some()
        || ![major, minor, patch]
            .iter()
            .all(|part| valid_core_number(part))
    {
        return Err(ConfigContractError::InvalidSemver);
    }
    Ok([major.to_owned(), minor.to_owned(), patch.to_owned()])
}

fn valid_core_number(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && (value == "0" || !value.starts_with('0'))
}

fn parse_prerelease(value: Option<&str>) -> Result<Vec<PrereleaseIdentifier>, ConfigContractError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    value.split('.').map(parse_prerelease_identifier).collect()
}

fn parse_prerelease_identifier(value: &str) -> Result<PrereleaseIdentifier, ConfigContractError> {
    if !valid_semver_identifier(value) {
        return Err(ConfigContractError::InvalidSemver);
    }
    if value.bytes().all(|byte| byte.is_ascii_digit()) {
        if value.len() > 1 && value.starts_with('0') {
            return Err(ConfigContractError::InvalidSemver);
        }
        Ok(PrereleaseIdentifier::Numeric(value.to_owned()))
    } else {
        Ok(PrereleaseIdentifier::Alpha(value.to_owned()))
    }
}

fn validate_build(value: Option<&str>) -> Result<(), ConfigContractError> {
    if value.is_some_and(|build| !build.split('.').all(valid_semver_identifier)) {
        return Err(ConfigContractError::InvalidSemver);
    }
    Ok(())
}

fn valid_semver_identifier(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

fn numeric_cmp(left: &str, right: &str) -> Ordering {
    left.len().cmp(&right.len()).then_with(|| left.cmp(right))
}

fn prerelease_cmp(left: &[PrereleaseIdentifier], right: &[PrereleaseIdentifier]) -> Ordering {
    match (left.is_empty(), right.is_empty()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => compare_prerelease_parts(left, right),
    }
}

fn compare_prerelease_parts(
    left: &[PrereleaseIdentifier],
    right: &[PrereleaseIdentifier],
) -> Ordering {
    for (left, right) in left.iter().zip(right) {
        let ordering = match (left, right) {
            (PrereleaseIdentifier::Numeric(left), PrereleaseIdentifier::Numeric(right)) => {
                numeric_cmp(left, right)
            }
            (PrereleaseIdentifier::Numeric(_), PrereleaseIdentifier::Alpha(_)) => Ordering::Less,
            (PrereleaseIdentifier::Alpha(_), PrereleaseIdentifier::Numeric(_)) => Ordering::Greater,
            (PrereleaseIdentifier::Alpha(left), PrereleaseIdentifier::Alpha(right)) => {
                left.cmp(right)
            }
        };
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
    left.len().cmp(&right.len())
}

impl_string_value!(CanonicalSemver, ConfigContractError::InvalidSemver);

/// Half-open byte span in an original configuration document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ByteSpan {
    pub start: u64,
    pub end: u64,
}

impl ByteSpan {
    /// Construct a half-open byte span.
    #[must_use]
    pub const fn new(start: u64, end: u64) -> Self {
        Self { start, end }
    }
}

/// Source category for one effective configuration value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProvenanceKind {
    BuiltInDefault,
    SelectedDocument,
}

/// Origin of one effective configuration value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProvenanceOrigin {
    pub kind: ProvenanceKind,
    pub canonical_path: Option<String>,
    pub span: Option<ByteSpan>,
}

/// Kind of configuration owner publishing a typed subtree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnerKind {
    Agent,
    Screen,
    Plugin,
}

/// Static, I/O-free contract for one known configuration owner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnerDescriptor {
    pub owner_id: Id,
    pub version: CanonicalSemver,
    pub kind: OwnerKind,
    pub defaults: TypedMap,
    pub secret_paths: BTreeSet<Vec<Id>>,
}

/// Deterministic catalog of active known configuration owners.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OwnerCatalog {
    owners: BTreeMap<Id, OwnerDescriptor>,
}

impl OwnerCatalog {
    /// Insert one owner, rejecting duplicate IDs.
    pub fn insert(&mut self, owner: OwnerDescriptor) -> Result<(), ConfigContractError> {
        if self.owners.contains_key(&owner.owner_id) {
            return Err(ConfigContractError::DuplicateOwner);
        }
        self.owners.insert(owner.owner_id.clone(), owner);
        Ok(())
    }

    /// Find an active owner by validated ID.
    #[must_use]
    pub fn get(&self, owner_id: &Id) -> Option<&OwnerDescriptor> {
        self.owners.get(owner_id)
    }

    /// Iterate owners in deterministic ID order.
    pub fn iter(&self) -> impl Iterator<Item = (&Id, &OwnerDescriptor)> {
        self.owners.iter()
    }
}
