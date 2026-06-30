//! Macro definitions for the harness scenario language.
//!
//! @plan PLAN-20260629-TMUX-HARNESS.P01
//! @requirement REQ-TMUX-HARNESS-001

use serde::Deserialize;
use serde::de::{self, MapAccess, Visitor};

use super::step::Step;

/// The body of a macro definition: its parameter names and the steps it
/// expands to. Step text may contain `$param` placeholders that are
/// substituted verbatim during expansion.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
#[derive(Debug, Clone)]
pub struct MacroDef {
    pub params: Vec<String>,
    pub steps: Vec<Step>,
}

impl<'de> Deserialize<'de> for MacroDef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        deserializer.deserialize_map(MacroDefVisitor)
    }
}

struct MacroDefVisitor;

impl<'de> Visitor<'de> for MacroDefVisitor {
    type Value = MacroDef;

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a macro definition object with params and steps")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut params: Option<Vec<String>> = None;
        let mut steps: Option<Vec<Step>> = None;
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "params" => {
                    reject_duplicate_field(params.is_some(), "params")?;
                    params = Some(map.next_value()?);
                }
                "steps" => {
                    reject_duplicate_field(steps.is_some(), "steps")?;
                    steps = Some(map.next_value()?);
                }
                _ => {
                    return Err(de::Error::unknown_field(&key, &["params", "steps"]));
                }
            }
        }
        let params =
            params.ok_or_else(|| de::Error::custom("missing required field 'params' in macro"))?;
        let steps =
            steps.ok_or_else(|| de::Error::custom("missing required field 'steps' in macro"))?;
        Ok(MacroDef { params, steps })
    }
}
/// Reject a duplicate macro definition field.
fn reject_duplicate_field<E>(seen: bool, field: &str) -> Result<(), E>
where
    E: de::Error,
{
    if seen {
        return Err(E::custom(format!("duplicate field '{field}' in macro")));
    }
    Ok(())
}
