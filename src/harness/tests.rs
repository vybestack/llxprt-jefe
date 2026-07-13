//! Behavioral tests for the pure harness scenario layer.
//!
//! RED phase: these tests define the typed scenario model, JSON parsing,
//! validation, and macro-expansion contracts before implementation.
//!
//! @plan PLAN-20260629-TMUX-HARNESS.P01
//! @requirement REQ-TMUX-HARNESS-001

use super::*;

/// Test-only helper: unwrap a `Result::Ok` or panic with context.
///
/// Mirrors the pattern in `src/github/tests.rs` to avoid `expect_used`.
trait TestResultExt<T> {
    fn value_or_panic(self, context: &str) -> T;
}

impl<T, E: std::fmt::Debug> TestResultExt<T> for Result<T, E> {
    fn value_or_panic(self, context: &str) -> T {
        match self {
            Ok(value) => value,
            Err(error) => panic!("{context}: {error:?}"),
        }
    }
}

/// Test-only helper: assert a `Result::Err` or panic.
fn error_or_panic<T: std::fmt::Debug, E>(result: Result<T, E>, context: &str) -> E {
    match result {
        Err(error) => error,
        Ok(value) => panic!("{context}: unexpectedly succeeded with {value:?}"),
    }
}

// ---------------------------------------------------------------------------
// Known-good scenario deserialization
// ---------------------------------------------------------------------------

/// A minimal valid scenario with a config block and a single `wait` step
/// deserializes into the expected typed `Scenario`.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
#[test]
fn parses_minimal_valid_scenario() {
    let json = r#"{
        "config": { "cols": 80, "rows": 24 },
        "steps": [ { "wait": 100 } ]
    }"#;
    let scenario = parse_scenario(json).value_or_panic("minimal scenario should parse");

    assert_eq!(scenario.config.cols, 80);
    assert_eq!(scenario.config.rows, 24);
    assert_eq!(scenario.steps.len(), 1);
    assert!(matches!(
        scenario.steps[0],
        Step::Wait { milliseconds } if milliseconds == 100
    ));
}

/// A scenario using every step primitive parses into its typed variant.
///
/// Exercises the full `Step` surface so a missing/renamed variant is caught.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
#[test]
fn parses_every_step_primitive() {
    let json = r#"{
        "config": { "cols": 120, "rows": 40, "history_limit": 5000 },
        "steps": [
            { "wait": 50 },
            { "line": "echo hello" },
            { "key": "Enter" },
            { "keys": ["a", "b"] },
            { "waitFor": "prompt>" },
            { "waitForNot": "loading" },
            { "expect": "done" },
            { "expectCount": "x", "count": 3 },
            { "capture": "shot1" },
            { "historySample": "hist1" },
            { "expectHistoryDelta": "diff1" },
            { "copyMode": true },
            { "waitForExit": 5000 }
        ]
    }"#;
    let scenario = parse_scenario(json).value_or_panic("full scenario should parse");

    assert_eq!(scenario.steps.len(), 13);
    assert_timing_steps(&scenario.steps);
    assert_text_steps(&scenario.steps);
    assert_pattern_steps(&scenario.steps);
    assert_capture_and_mode_steps(&scenario.steps);
}

/// Assert wait/waitForExit variants.
fn assert_timing_steps(steps: &[Step]) {
    assert!(matches!(&steps[0], Step::Wait { milliseconds } if *milliseconds == 50));
    assert!(matches!(
        &steps[12],
        Step::WaitForExit { timeout_ms } if *timeout_ms == 5000
    ));
}

/// Assert line/key/keys variants.
fn assert_text_steps(steps: &[Step]) {
    assert!(matches!(&steps[1], Step::Line { text } if text == "echo hello"));
    assert!(matches!(&steps[2], Step::Key { key } if key == "Enter"));
    assert!(matches!(
        &steps[3],
        Step::Keys { keys } if keys.as_slice() == ["a", "b"]
    ));
}

/// Assert waitFor/waitForNot/expect/expectCount variants.
fn assert_pattern_steps(steps: &[Step]) {
    assert!(matches!(&steps[4], Step::WaitFor { pattern } if pattern == "prompt>"));
    assert!(matches!(&steps[5], Step::WaitForNot { pattern } if pattern == "loading"));
    assert!(matches!(&steps[6], Step::Expect { pattern } if pattern == "done"));
    assert!(matches!(
        &steps[7],
        Step::ExpectCount { pattern, count } if pattern == "x" && *count == 3
    ));
}

/// Assert capture/historySample/expectHistoryDelta/copyMode variants.
fn assert_capture_and_mode_steps(steps: &[Step]) {
    assert!(matches!(&steps[8], Step::Capture { name } if name == "shot1"));
    assert!(matches!(&steps[9], Step::HistorySample { name } if name == "hist1"));
    assert!(matches!(
        &steps[10],
        Step::ExpectHistoryDelta { name } if name == "diff1"
    ));
    assert!(matches!(&steps[11], Step::CopyMode { enabled } if *enabled));
}

/// Config defaults apply when optional fields are omitted.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
#[test]
fn config_defaults_when_optional_fields_omitted() {
    let json = r#"{ "config": { "cols": 80, "rows": 24 }, "steps": [] }"#;
    let scenario = parse_scenario(json).value_or_panic("should parse");
    assert_eq!(scenario.config.assert_mode, AssertMode::Strict);
    assert!(!scenario.config.keep_session);
    assert!(scenario.config.out_dir.is_none());
    assert_eq!(scenario.config.history_limit, 10_000);
    assert_eq!(scenario.config.initial_wait_ms, 0);
}

/// `assert_mode` accepts the typed enum values.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
#[test]
fn config_parses_assert_mode() {
    let json = r#"{ "config": { "cols": 80, "rows": 24, "assert_mode": "soft" }, "steps": [] }"#;
    let scenario = parse_scenario(json).value_or_panic("should parse");
    assert_eq!(scenario.config.assert_mode, AssertMode::Soft);
}

// ---------------------------------------------------------------------------
// Malformed scenarios -> typed errors
// ---------------------------------------------------------------------------

/// An unrecognized step kind yields `ScenarioError::UnknownStepKind`.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
#[test]
fn unknown_step_kind_is_rejected() {
    let json = r#"{ "config": { "cols": 80, "rows": 24 }, "steps": [ { "fly": true } ] }"#;
    let err = error_or_panic(parse_scenario(json), "unknown step should fail");
    assert!(matches!(err, ScenarioError::UnknownStepKind { ref kind } if kind == "fly"));
}

/// A missing required config field (`cols`) yields a typed parse error.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
#[test]
fn missing_config_field_is_rejected() {
    let json = r#"{ "config": { "rows": 24 }, "steps": [] }"#;
    let err = error_or_panic(parse_scenario(json), "missing cols should fail");
    assert!(
        matches!(err, ScenarioError::MissingField { .. }),
        "got {err:?}"
    );
}

/// A zero column count yields `ScenarioError::InvalidConfig`.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
#[test]
fn zero_cols_is_rejected() {
    let json = r#"{ "config": { "cols": 0, "rows": 24 }, "steps": [] }"#;
    let err = error_or_panic(parse_scenario(json), "zero cols should fail");
    assert!(
        matches!(err, ScenarioError::InvalidConfig { .. }),
        "got {err:?}"
    );
}

/// A zero row count yields `ScenarioError::InvalidConfig`.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
#[test]
fn zero_rows_is_rejected() {
    let json = r#"{ "config": { "cols": 80, "rows": 0 }, "steps": [] }"#;
    let err = error_or_panic(parse_scenario(json), "zero rows should fail");
    assert!(
        matches!(err, ScenarioError::InvalidConfig { .. }),
        "got {err:?}"
    );
}

/// A missing `count` field in `expectCount` yields a typed missing-field error.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
#[test]
fn missing_expect_count_count_is_rejected() {
    let json = r#"{
        "config": { "cols": 80, "rows": 24 },
        "steps": [ { "expectCount": "done" } ]
    }"#;
    let err = error_or_panic(parse_scenario(json), "missing count should fail");
    assert!(matches!(
        err,
        ScenarioError::MissingField { ref field, ref context }
            if field == "count" && context == "step"
    ));
}

/// Unknown config keys are rejected so typos do not silently change scenarios.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
#[test]
fn unknown_config_field_is_rejected() {
    let json = r#"{
        "config": { "cols": 80, "rows": 24, "colz": 80 },
        "steps": []
    }"#;
    let err = error_or_panic(parse_scenario(json), "unknown config key should fail");
    assert!(matches!(err, ScenarioError::Json { .. }), "got {err:?}");
}

/// A zero history limit is rejected as an out-of-range config value.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
#[test]
fn zero_history_limit_is_rejected() {
    let json = r#"{
        "config": { "cols": 80, "rows": 24, "history_limit": 0 },
        "steps": []
    }"#;
    let err = error_or_panic(parse_scenario(json), "zero history limit should fail");
    assert!(matches!(
        err,
        ScenarioError::InvalidConfig { ref field, .. } if field == "history_limit"
    ));
}

/// A missing `args` field in a macro invocation yields a typed missing-field error.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
#[test]
fn missing_macro_args_field_is_rejected() {
    let json = r#"{
        "config": { "cols": 80, "rows": 24 },
        "steps": [ { "macro": "greet" } ]
    }"#;
    let err = error_or_panic(parse_scenario(json), "missing macro args should fail");
    assert!(matches!(
        err,
        ScenarioError::MissingField { ref field, ref context }
            if field == "args" && context == "step"
    ));
}

/// A macro definition missing `params` yields a typed missing-field error.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
#[test]
fn missing_macro_definition_params_is_rejected() {
    let json = r#"{
        "config": { "cols": 80, "rows": 24 },
        "macros": {
            "bad": { "steps": [ { "line": "x" } ] }
        },
        "steps": []
    }"#;

    let err = error_or_panic(parse_scenario(json), "missing macro params should fail");
    assert!(matches!(
        err,
        ScenarioError::MissingField { ref field, ref context }
            if field == "params" && context == "macro"
    ));
}

/// Duplicate fields in a single-field step are rejected.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
#[test]
fn duplicate_single_field_step_keys_are_rejected() {
    let json = r#"{
        "config": { "cols": 80, "rows": 24 },
        "steps": [ { "line": "first", "line": "second" } ]
    }"#;
    let err = error_or_panic(parse_scenario(json), "duplicate step key should fail");
    assert!(
        matches!(err, ScenarioError::InvalidStep { ref reason } if reason.contains("duplicate field"))
    );
}

/// Duplicate auxiliary fields in a multi-field step are rejected.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
#[test]
fn duplicate_multi_field_step_keys_are_rejected() {
    let json = r#"{
        "config": { "cols": 80, "rows": 24 },
        "steps": [ { "expectCount": "done", "count": 1, "count": 2 } ]
    }"#;
    let err = error_or_panic(parse_scenario(json), "duplicate auxiliary key should fail");
    assert!(
        matches!(err, ScenarioError::InvalidStep { ref reason } if reason.contains("duplicate field"))
    );
}

/// Empty key names are rejected during semantic validation.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
#[test]
fn empty_key_is_rejected() {
    let json = r#"{
        "config": { "cols": 80, "rows": 24 },
        "steps": [ { "key": "" } ]
    }"#;
    let err = error_or_panic(parse_scenario(json), "empty key should fail");
    assert!(matches!(
        err,
        ScenarioError::InvalidStep { ref reason } if reason.contains("key")
    ));
}

/// Empty key arrays are rejected during semantic validation.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
#[test]
fn empty_keys_array_is_rejected() {
    let json = r#"{
        "config": { "cols": 80, "rows": 24 },
        "steps": [ { "keys": [] } ]
    }"#;
    let err = error_or_panic(parse_scenario(json), "empty keys array should fail");
    assert!(matches!(
        err,
        ScenarioError::InvalidStep { ref reason } if reason.contains("keys")
    ));
}

/// Duplicate macro names are rejected before a map can overwrite earlier definitions.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
#[test]
fn duplicate_macro_names_are_rejected() {
    let json = r#"{
        "config": { "cols": 80, "rows": 24 },
        "macros": {
            "dup": { "params": [], "steps": [ { "line": "first" } ] },
            "dup": { "params": [], "steps": [ { "line": "second" } ] }
        },
        "steps": []
    }"#;
    let err = error_or_panic(parse_scenario(json), "duplicate macro names should fail");
    assert!(matches!(err, ScenarioError::Json { .. }), "got {err:?}");
}

/// A macro definition missing `steps` yields a typed missing-field error.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
#[test]
fn missing_macro_definition_steps_is_rejected() {
    let json = r#"{
        "config": { "cols": 80, "rows": 24 },
        "macros": {
            "bad": { "params": [] }
        },
        "steps": []
    }"#;
    let err = error_or_panic(parse_scenario(json), "missing macro steps should fail");
    assert!(matches!(
        err,
        ScenarioError::MissingField { ref field, ref context }
            if field == "steps" && context == "macro"
    ));
}

/// Duplicate parameter names in a macro definition are rejected during validation.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
#[test]
fn duplicate_macro_definition_params_are_rejected() {
    let json = r#"{
        "config": { "cols": 80, "rows": 24 },
        "macros": {
            "bad": { "params": ["name", "name"], "steps": [ { "line": "$name" } ] }
        },
        "steps": []
    }"#;
    let err = error_or_panic(parse_scenario(json), "duplicate params should fail");
    assert!(matches!(
        err,
        ScenarioError::InvalidMacro { ref name, ref reason }
            if name == "bad" && reason.contains("duplicate parameter")
    ));
}

/// Malformed JSON yields a typed JSON error, not a panic.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
#[test]
fn malformed_json_is_rejected() {
    let err = error_or_panic(parse_scenario("{ not json"), "malformed json should fail");
    assert!(matches!(err, ScenarioError::Json { .. }), "got {err:?}");
}

/// An invalid `assert_mode` value yields a typed parse error.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
#[test]
fn invalid_assert_mode_is_rejected() {
    let json = r#"{ "config": { "cols": 80, "rows": 24, "assert_mode": "loud" }, "steps": [] }"#;
    let err = error_or_panic(parse_scenario(json), "bad assert_mode should fail");
    assert!(matches!(err, ScenarioError::Json { .. }), "got {err:?}");
}

// ---------------------------------------------------------------------------
// Macro expansion
// ---------------------------------------------------------------------------

/// A macro invocation expands to its body with parameter substitution.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
#[test]
fn expands_simple_macro_substitution() {
    let json = r#"{
        "config": { "cols": 80, "rows": 24 },
        "macros": {
            "greet": {
                "params": ["who"],
                "steps": [ { "line": "$who" }, { "key": "Enter" } ]
            }
        },
        "steps": [ { "macro": "greet", "args": { "who": "world" } } ]
    }"#;
    let scenario = parse_scenario(json).value_or_panic("should parse");
    let expanded = expand_macros(&scenario).value_or_panic("should expand");

    assert_eq!(expanded.steps.len(), 2, "macro body has 2 steps");
    assert!(matches!(
        &expanded.steps[0],
        Step::Line { text } if text == "world"
    ));
    assert!(matches!(&expanded.steps[1], Step::Key { key } if key == "Enter"));
}

/// A macro invocation with the right count but wrong argument name yields MissingMacroArg.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
#[test]
fn wrong_macro_arg_name_is_rejected() {
    let json = r#"{
        "config": { "cols": 80, "rows": 24 },
        "macros": {
            "greet": {
                "params": ["who"],
                "steps": [ { "line": "$who" } ]
            }
        },
        "steps": [ { "macro": "greet", "args": { "what": "world" } } ]
    }"#;
    let scenario = parse_scenario(json).value_or_panic("should parse");
    let err = error_or_panic(expand_macros(&scenario), "wrong arg name should fail");
    assert!(matches!(
        err,
        ScenarioError::MissingMacroArg { ref name, ref param }
            if name == "greet" && param == "who"
    ));
}

/// An unknown macro name yields `ScenarioError::UnknownMacro`.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
#[test]
fn unknown_macro_is_rejected() {
    let json = r#"{
        "config": { "cols": 80, "rows": 24 },
        "steps": [ { "macro": "nope", "args": {} } ]
    }"#;
    let scenario = parse_scenario(json).value_or_panic("should parse");
    let err = error_or_panic(expand_macros(&scenario), "unknown macro should fail");
    assert!(matches!(
        err,
        ScenarioError::UnknownMacro { ref name } if name == "nope"
    ));
}

/// A macro invocation with a missing argument yields
/// `ScenarioError::MacroArityMismatch`.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
#[test]
fn missing_macro_arg_is_rejected() {
    let json = r#"{
        "config": { "cols": 80, "rows": 24 },
        "macros": {
            "greet": {
                "params": ["who"],
                "steps": [ { "line": "$who" } ]
            }
        },
        "steps": [ { "macro": "greet", "args": {} } ]
    }"#;
    let scenario = parse_scenario(json).value_or_panic("should parse");
    let err = error_or_panic(expand_macros(&scenario), "missing arg should fail");
    assert!(
        matches!(err, ScenarioError::MacroArityMismatch { .. }),
        "got {err:?}"
    );
}

/// A macro invocation with too many arguments yields
/// `ScenarioError::MacroArityMismatch`.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
#[test]
fn extra_macro_arg_is_rejected() {
    let json = r#"{
        "config": { "cols": 80, "rows": 24 },
        "macros": {
            "ping": { "params": [], "steps": [ { "line": "pong" } ] }
        },
        "steps": [ { "macro": "ping", "args": { "surprise": "1" } } ]
    }"#;
    let scenario = parse_scenario(json).value_or_panic("should parse");
    let err = error_or_panic(expand_macros(&scenario), "extra arg should fail");
    assert!(
        matches!(err, ScenarioError::MacroArityMismatch { .. }),
        "got {err:?}"
    );
}

/// A macro cycle yields `ScenarioError::MacroCycle`.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
#[test]
fn macro_cycle_is_detected() {
    let json = r#"{
        "config": { "cols": 80, "rows": 24 },
        "macros": {
            "a": { "params": [], "steps": [ { "macro": "b", "args": {} } ] },
            "b": { "params": [], "steps": [ { "macro": "a", "args": {} } ] }
        },
        "steps": [ { "macro": "a", "args": {} } ]
    }"#;
    let scenario = parse_scenario(json).value_or_panic("should parse");
    let err = error_or_panic(expand_macros(&scenario), "cycle should fail");
    assert!(
        matches!(err, ScenarioError::MacroCycle { .. }),
        "got {err:?}"
    );
}

/// Macro substitution is exact/raw: a non-string argument value (here a JSON
/// number) is spliced verbatim into a text field without reinterpretation.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
#[test]
fn macro_substitutes_raw_nonstring_values() {
    let json = r#"{
        "config": { "cols": 80, "rows": 24 },
        "macros": {
            "say": { "params": ["n"], "steps": [ { "line": "count=$n" } ] }
        },
        "steps": [ { "macro": "say", "args": { "n": 42 } } ]
    }"#;
    let scenario = parse_scenario(json).value_or_panic("should parse");
    let expanded = expand_macros(&scenario).value_or_panic("should expand");
    assert!(matches!(
        &expanded.steps[0],
        Step::Line { text } if text == "count=42"
    ));
}

/// Macro substitution preserves the visible decimal point for integral floats.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
#[test]
fn macro_substitutes_integral_float_with_decimal_point() {
    let json = r#"{
        "config": { "cols": 80, "rows": 24 },
        "macros": {
            "say": { "params": ["n"], "steps": [ { "line": "count=$n" } ] }
        },
        "steps": [ { "macro": "say", "args": { "n": 1.0 } } ]
    }"#;
    let scenario = parse_scenario(json).value_or_panic("should parse");
    let expanded = expand_macros(&scenario).value_or_panic("should expand");

    assert!(matches!(
        &expanded.steps[0],
        Step::Line { text } if text == "count=1.0"
    ));
}
/// Unknown placeholders are preserved rather than partially substituting prefixes.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
#[test]
fn macro_preserves_unknown_placeholder_prefixes() {
    let json = r#"{
        "config": { "cols": 80, "rows": 24 },
        "macros": {
            "say": { "params": ["n"], "steps": [ { "line": "$name and $n" } ] }
        },
        "steps": [ { "macro": "say", "args": { "n": "42" } } ]
    }"#;
    let scenario = parse_scenario(json).value_or_panic("should parse");
    let expanded = expand_macros(&scenario).value_or_panic("should expand");
    assert!(matches!(
        &expanded.steps[0],
        Step::Line { text } if text == "$name and 42"
    ));
}

/// Longest matching parameter names win when placeholders share a prefix.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
#[test]
fn macro_uses_longest_placeholder_match() {
    let json = r#"{
        "config": { "cols": 80, "rows": 24 },
        "macros": {
            "say": { "params": ["n", "name"], "steps": [ { "line": "$name/$n" } ] }
        },
        "steps": [ { "macro": "say", "args": { "n": "42", "name": "alice" } } ]
    }"#;
    let scenario = parse_scenario(json).value_or_panic("should parse");
    let expanded = expand_macros(&scenario).value_or_panic("should expand");
    assert!(matches!(
        &expanded.steps[0],
        Step::Line { text } if text == "alice/42"
    ));
}

/// A literal dollar sign remains literal when not followed by a parameter name.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
#[test]
fn macro_preserves_literal_dollar_text() {
    let json = r#"{
        "config": { "cols": 80, "rows": 24 },
        "macros": {
            "say": { "params": ["n"], "steps": [ { "line": "cost $$ then $n" } ] }
        },
        "steps": [ { "macro": "say", "args": { "n": "42" } } ]
    }"#;
    let scenario = parse_scenario(json).value_or_panic("should parse");
    let expanded = expand_macros(&scenario).value_or_panic("should expand");
    assert!(matches!(
        &expanded.steps[0],
        Step::Line { text } if text == "cost $$ then 42"
    ));
}

/// Nested macro invocations receive the outer macro's substituted arguments.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
#[test]
fn nested_macro_substitutes_outer_args() {
    let json = r#"{
        "config": { "cols": 80, "rows": 24 },
        "macros": {
            "inner": {
                "params": ["message"],
                "steps": [ { "line": "$message" } ]
            },
            "outer": {
                "params": ["who"],
                "steps": [ { "macro": "inner", "args": { "message": "hello $who" } } ]
            }
        },
        "steps": [ { "macro": "outer", "args": { "who": "world" } } ]
    }"#;
    let scenario = parse_scenario(json).value_or_panic("should parse");
    let expanded = expand_macros(&scenario).value_or_panic("should expand nested macros");

    assert_eq!(expanded.steps.len(), 1);
    assert!(matches!(
        &expanded.steps[0],
        Step::Line { text } if text == "hello world"
    ));
}

/// Macro expansion rejects substituted concrete steps that violate semantic validation.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
#[test]
fn macro_expansion_rejects_semantically_invalid_substituted_step() {
    let json = r#"{
        "config": { "cols": 80, "rows": 24 },
        "macros": {
            "press": { "params": ["k"], "steps": [ { "key": "$k" } ] }
        },
        "steps": [ { "macro": "press", "args": { "k": "" } } ]
    }"#;
    let scenario = parse_scenario(json).value_or_panic("should parse before expansion");
    let err = error_or_panic(expand_macros(&scenario), "invalid expansion should fail");

    assert!(matches!(
        err,
        ScenarioError::InvalidStep { ref reason } if reason.contains("key")
    ));
}
/// Programmatically constructed invalid macro invocations cannot bypass expansion validation.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
#[test]
fn expand_macros_validates_macro_invocation_steps() {
    let scenario = Scenario {
        config: ScenarioConfig {
            cols: 80,
            rows: 24,
            history_limit: 10_000,
            initial_wait_ms: 0,
            out_dir: None,
            keep_session: false,
            assert_mode: AssertMode::Strict,
        },
        macros: std::collections::BTreeMap::new(),
        steps: vec![Step::Macro {
            name: String::new(),
            args: std::collections::BTreeMap::new(),
        }],
    };

    let err = error_or_panic(expand_macros(&scenario), "invalid macro step should fail");
    assert!(matches!(
        err,
        ScenarioError::InvalidStep { ref reason } if reason.contains("macro name")
    ));
}

/// Multiple top-level steps, with a macro in the middle, expand in order.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
#[test]
fn expands_macros_preserving_order() {
    let json = r#"{
        "config": { "cols": 80, "rows": 24 },
        "macros": {
            "two": { "params": [], "steps": [ { "line": "x" }, { "line": "y" } ] }
        },
        "steps": [
            { "line": "before" },
            { "macro": "two", "args": {} },
            { "line": "after" }
        ]
    }"#;
    let scenario = parse_scenario(json).value_or_panic("should parse");
    let expanded = expand_macros(&scenario).value_or_panic("should expand");
    assert_eq!(expanded.steps.len(), 4);
    assert!(matches!(&expanded.steps[0], Step::Line { text } if text == "before"));
    assert!(matches!(&expanded.steps[1], Step::Line { text } if text == "x"));
    assert!(matches!(&expanded.steps[2], Step::Line { text } if text == "y"));
    assert!(matches!(&expanded.steps[3], Step::Line { text } if text == "after"));
}

/// Expansion is idempotent: expanding an already-expanded scenario is a
/// no-op (no macros remain).
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
#[test]
fn expansion_is_idempotent() {
    let json = r#"{
        "config": { "cols": 80, "rows": 24 },
        "macros": {
            "greet": { "params": ["who"], "steps": [ { "line": "$who" } ] }
        },
        "steps": [ { "macro": "greet", "args": { "who": "z" } } ]
    }"#;
    let scenario = parse_scenario(json).value_or_panic("should parse");
    let once = expand_macros(&scenario).value_or_panic("first expand");
    let twice = expand_macros(&once).value_or_panic("second expand");
    assert_eq!(scenario.steps.len(), 1);
    assert!(matches!(&scenario.steps[0], Step::Macro { name, .. } if name == "greet"));
    assert_eq!(
        once.steps,
        vec![Step::Line {
            text: "z".to_string()
        }]
    );
    assert_eq!(twice.steps, once.steps);
}

/// Macro expansion substitutes `$param` placeholders in `Step::Type` steps,
/// not just `Step::Line`. This is required for tutorial-capture scenarios
/// that use `{"type": "$value"}` inside macros to inject unique values
/// (issue numbers, PR numbers, agent names) into form fields.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
#[test]
fn macro_expansion_substitutes_type_step() {
    let json = r#"{
        "config": { "cols": 80, "rows": 24 },
        "macros": {
            "fill": {
                "params": ["value"],
                "steps": [ { "type": "$value" }, { "key": "Enter" } ]
            }
        },
        "steps": [ { "macro": "fill", "args": { "value": "issue-42" } } ]
    }"#;
    let scenario = parse_scenario(json).value_or_panic("should parse");
    let expanded = expand_macros(&scenario).value_or_panic("should expand");
    assert_eq!(expanded.steps.len(), 2, "macro body has 2 steps");
    assert!(
        matches!(&expanded.steps[0], Step::Type { text } if text == "issue-42"),
        "type step must have substituted text"
    );
    assert!(matches!(&expanded.steps[1], Step::Key { key } if key == "Enter"));
}

/// Macro expansion substitutes `$param` placeholders in multiple `Step::Type`
/// fields within a single macro body, each with a different argument.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
#[test]
fn macro_expansion_substitutes_multiple_type_steps() {
    let json = r#"{
        "config": { "cols": 80, "rows": 24 },
        "macros": {
            "double": {
                "params": ["first", "second"],
                "steps": [
                    { "type": "$first" },
                    { "key": "Tab" },
                    { "type": "$second" }
                ]
            }
        },
        "steps": [ { "macro": "double", "args": { "first": "aaa", "second": "bbb" } } ]
    }"#;
    let scenario = parse_scenario(json).value_or_panic("should parse");
    let expanded = expand_macros(&scenario).value_or_panic("should expand");
    assert_eq!(expanded.steps.len(), 3);
    assert!(
        matches!(&expanded.steps[0], Step::Type { text } if text == "aaa"),
        "first type must substitute"
    );
    assert!(matches!(&expanded.steps[1], Step::Key { key } if key == "Tab"));
    assert!(
        matches!(&expanded.steps[2], Step::Type { text } if text == "bbb"),
        "second type must substitute"
    );
}
