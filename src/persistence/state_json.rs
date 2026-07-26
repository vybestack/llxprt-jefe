//! Duplicate-preserving JSON preflight for strict state documents.
//!
//! Serde derives reject unknown struct fields but ordinary map decoding can
//! overwrite duplicate keys. This visitor consumes the whole document first
//! and rejects every duplicate object key before the typed StateV2 decode.

use std::collections::BTreeSet;
use std::fmt::Formatter;

use serde::Deserialize;
use serde::de::{MapAccess, SeqAccess, Visitor};

pub(super) fn reject_duplicate_keys(bytes: &[u8]) -> Result<(), serde_json::Error> {
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    StrictValue::deserialize(&mut deserializer)?;
    deserializer.end()
}

struct StrictValue;

impl<'de> Deserialize<'de> for StrictValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(StrictVisitor)
    }
}

struct StrictVisitor;

impl<'de> Visitor<'de> for StrictVisitor {
    type Value = StrictValue;

    fn expecting(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a JSON value without duplicate object keys")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut keys = BTreeSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if !keys.insert(key.clone()) {
                return Err(serde::de::Error::custom(format!(
                    "duplicate object key {key:?}"
                )));
            }
            let _: StrictValue = map.next_value()?;
        }
        Ok(StrictValue)
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        while sequence.next_element::<StrictValue>()?.is_some() {}
        Ok(StrictValue)
    }

    fn visit_bool<E>(self, _value: bool) -> Result<Self::Value, E> {
        Ok(StrictValue)
    }

    fn visit_i64<E>(self, _value: i64) -> Result<Self::Value, E> {
        Ok(StrictValue)
    }

    fn visit_u64<E>(self, _value: u64) -> Result<Self::Value, E> {
        Ok(StrictValue)
    }

    fn visit_f64<E>(self, _value: f64) -> Result<Self::Value, E> {
        Ok(StrictValue)
    }

    fn visit_str<E>(self, _value: &str) -> Result<Self::Value, E> {
        Ok(StrictValue)
    }

    fn visit_string<E>(self, _value: String) -> Result<Self::Value, E> {
        Ok(StrictValue)
    }

    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(StrictValue)
    }

    fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        StrictValue::deserialize(deserializer)
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(StrictValue)
    }
}
