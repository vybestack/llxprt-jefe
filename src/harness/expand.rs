//! Macro expansion: resolve `Step::Macro` invocations into concrete steps.
//!
//! Expansion is deterministic, rejects unknown/cyclic/arity-mismatched macros,
//! and performs raw (exact) placeholder substitution. No side effects.
//!
//! @plan PLAN-20260629-TMUX-HARNESS.P01
//! @requirement REQ-TMUX-HARNESS-001

use std::collections::BTreeMap;

use super::error::ScenarioError;
use super::macro_def::MacroDef;
use super::scenario::Scenario;
use super::step::Step;

/// Placeholder sigil: `$param` in step text is replaced by the argument.
const PLACEHOLDER: char = '$';

/// Expand every macro invocation in `scenario.steps` into concrete steps.
///
/// Returns a new [`Scenario`] whose `steps` contain only concrete primitives
/// (no `Step::Macro`). The original `macros` table is retained so re-running
/// expansion is a no-op.
///
/// # Errors
///
/// Returns [`ScenarioError::UnknownMacro`], [`ScenarioError::MacroArityMismatch`],
/// [`ScenarioError::MissingMacroArg`], or [`ScenarioError::MacroCycle`] as
/// appropriate.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
pub fn expand_macros(scenario: &Scenario) -> Result<Scenario, ScenarioError> {
    let mut expanded: Vec<Step> = Vec::with_capacity(scenario.steps.len());
    for step in &scenario.steps {
        expand_one(&scenario.macros, step, &mut Vec::new(), &mut expanded)?;
    }
    Ok(Scenario {
        config: scenario.config.clone(),
        macros: scenario.macros.clone(),
        steps: expanded,
    })
}

/// Expand a single step, appending concrete steps to `out`.
///
/// `chain` tracks the macro names currently being expanded for cycle
/// detection. Concrete steps are substituted (in case they were produced by
/// a parent macro's body) and appended directly.
fn expand_one(
    macros: &BTreeMap<String, MacroDef>,
    step: &Step,
    chain: &mut Vec<String>,
    out: &mut Vec<Step>,
) -> Result<(), ScenarioError> {
    step.validate()?;
    match step {
        Step::Macro { name, args } => expand_macro_invocation(macros, name, args, chain, out),
        other => {
            out.push(other.clone());
            Ok(())
        }
    }
}

/// Expand a macro invocation, with cycle detection and arity validation.
fn expand_macro_invocation(
    macros: &BTreeMap<String, MacroDef>,
    name: &str,
    args: &BTreeMap<String, String>,
    chain: &mut Vec<String>,
    out: &mut Vec<Step>,
) -> Result<(), ScenarioError> {
    if chain.iter().any(|c| c == name) {
        let mut cycle = chain.clone();
        cycle.push(name.to_string());
        return Err(ScenarioError::MacroCycle { chain: cycle });
    }

    let def = macros
        .get(name)
        .ok_or_else(|| ScenarioError::UnknownMacro {
            name: name.to_string(),
        })?;

    validate_arity(name, def, args)?;
    for body_step in &def.steps {
        let substituted = substitute_step(body_step, args);
        chain.push(name.to_string());
        let result = expand_one(macros, &substituted, chain, out);
        chain.pop();
        result?;
    }
    Ok(())
}

/// Validate that the provided arguments match the macro's parameter list.
fn validate_arity(
    name: &str,
    def: &MacroDef,
    args: &BTreeMap<String, String>,
) -> Result<(), ScenarioError> {
    let expected = def.params.len();
    if args.len() != expected {
        return Err(ScenarioError::MacroArityMismatch {
            name: name.to_string(),
            expected,
            provided: args.len(),
        });
    }
    for param in &def.params {
        if !args.contains_key(param) {
            return Err(ScenarioError::MissingMacroArg {
                name: name.to_string(),
                param: param.clone(),
            });
        }
    }
    Ok(())
}

/// Substitute `$param` placeholders in a single step using `args`.
///
/// Substitution is exact/raw: the argument string replaces the placeholder
/// verbatim, so numeric or boolean argument text is spliced without
/// reinterpretation.
fn substitute_step(step: &Step, args: &BTreeMap<String, String>) -> Step {
    match step {
        Step::Line { text } => Step::Line {
            text: substitute(text, args),
        },
        Step::Key { key } => Step::Key {
            key: substitute(key, args),
        },
        Step::Keys { keys } => Step::Keys {
            keys: keys.iter().map(|k| substitute(k, args)).collect(),
        },
        Step::WaitFor { pattern } => Step::WaitFor {
            pattern: substitute(pattern, args),
        },
        Step::WaitForNot { pattern } => Step::WaitForNot {
            pattern: substitute(pattern, args),
        },
        Step::Expect { pattern } => Step::Expect {
            pattern: substitute(pattern, args),
        },
        Step::ExpectRightEdge { pattern } => Step::ExpectRightEdge {
            pattern: substitute(pattern, args),
        },
        Step::ExpectCount { pattern, count } => Step::ExpectCount {
            pattern: substitute(pattern, args),
            count: *count,
        },
        Step::Capture { name } => Step::Capture {
            name: substitute(name, args),
        },
        Step::HistorySample { name } => Step::HistorySample {
            name: substitute(name, args),
        },
        Step::ExpectHistoryDelta { name } => Step::ExpectHistoryDelta {
            name: substitute(name, args),
        },
        Step::Macro { name, args: margs } => Step::Macro {
            name: name.clone(),
            args: substitute_args(margs, args),
        },
        other => other.clone(),
    }
}

/// Substitute placeholders in each nested macro argument value.
fn substitute_args(
    macro_args: &BTreeMap<String, String>,
    outer_args: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    macro_args
        .iter()
        .map(|(name, value)| (name.clone(), substitute(value, outer_args)))
        .collect()
}

/// Replace every `$name` placeholder in `text` with the matching argument.
///
/// An unknown placeholder is left as-is rather than erroring, so macro authors
/// can include literal `$` text that is not a parameter.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
fn substitute(text: &str, args: &BTreeMap<String, String>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut cursor = text;
    while let Some(dollar) = cursor.find(PLACEHOLDER) {
        out.push_str(&cursor[..dollar]);
        let after = &cursor[dollar + PLACEHOLDER.len_utf8()..];
        if let Some((param_len, value)) = match_param(after, args) {
            out.push_str(&value);
            cursor = &after[param_len..];
        } else {
            out.push(PLACEHOLDER);
            cursor = after;
        }
    }
    out.push_str(cursor);
    out
}

/// Match the longest parameter name occurring at the start of `text`.
///
/// Returns the matched name's length and its substitution value. Longest-match
/// avoids ambiguity between params like `$a` and `$ab`.
fn match_param(text: &str, args: &BTreeMap<String, String>) -> Option<(usize, String)> {
    args.iter()
        .filter_map(|(name, value)| {
            text.strip_prefix(name.as_str())
                .filter(|rest| is_placeholder_boundary(rest))
                .map(|_| (name.len(), value.clone()))
        })
        .max_by_key(|(len, _)| *len)
}

/// A placeholder ends before another identifier character.
fn is_placeholder_boundary(rest: &str) -> bool {
    match rest.chars().next() {
        Some(c) => !is_param_char(c),
        None => true,
    }
}

/// Parameter names use identifier-like characters.
fn is_param_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}
