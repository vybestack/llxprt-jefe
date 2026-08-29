//! Definition-content SHA-256 fingerprint (issue #382 CW-02).
//!
//! A thin newtype over the existing dependency-free
//! [`crate::domain::sha256`] primitive. The shipped definitions carry their
//! own content digest so a launch plan can stamp `definition_sha256` without
//! re-deriving it at runtime. The hex parser rejects non-canonical input
//! rather than falling back to a sentinel digest.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// 32-byte content digest of a shipped agent definition's canonical
/// serialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct DefinitionSha256([u8; 32]);

impl DefinitionSha256 {
    /// Compute the digest of an in-memory byte slice.
    #[must_use]
    pub fn digest(input: &[u8]) -> Self {
        Self(
            crate::domain::sha256::Sha256::digest(input)
                .as_bytes()
                .to_owned(),
        )
    }

    /// Borrow the fixed digest bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Lowercase hexadecimal encoding.
    #[must_use]
    pub fn to_hex(&self) -> String {
        let mut out = String::with_capacity(64);
        for byte in self.0 {
            use std::fmt::Write as _;
            let _ = write!(out, "{byte:02x}");
        }
        out
    }
}

impl fmt::Display for DefinitionSha256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl Serialize for DefinitionSha256 {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for DefinitionSha256 {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let hex = String::deserialize(d)?;
        parse_hex(&hex).map_err(serde::de::Error::custom)
    }
}

fn parse_hex(hex: &str) -> Result<DefinitionSha256, String> {
    if hex.len() != 64 {
        return Err(format!(
            "definition SHA-256 must be 64 hex chars, found {}",
            hex.len()
        ));
    }
    let mut bytes = [0u8; 32];
    for (i, pair) in hex.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        let high = hex_val(pair[0])?;
        let low = hex_val(pair[1])?;
        bytes[i] = (high << 4) | low;
    }
    Ok(DefinitionSha256(bytes))
}

fn hex_val(byte: u8) -> Result<u8, String> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err("invalid hex digit".to_string()),
    }
}

#[cfg(test)]
#[path = "sha256_tests.rs"]
mod tests;
