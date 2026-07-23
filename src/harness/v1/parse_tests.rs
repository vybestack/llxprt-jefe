//! Behavioral tests for schema-1 scenario parsing (issue #380, CW00-01/02/04/10).

use super::super::contract::{FileContent, Modifier, Platform, Step, WaitSource};
use super::super::error::{HarCode, HarnessError};
use super::parse_scenario_v1;

fn minimal_with_steps(steps: &str) -> String {
    format!(
        r#"{{"schema":1,"name":"t","platform":"linux",
            "terminal":{{"cols":100,"rows":30}},
            "workspace":{{"mode":448,"dirs":[],"files":[],"env":[]}},
            "steps":{steps},"secrets":[]}}"#
    )
}

fn parse(json: &str) -> Result<super::super::contract::ScenarioV1, HarnessError> {
    parse_scenario_v1(json.as_bytes())
}

const LAUNCH: &str = r#"{"op":"launch","argv":["app"],"env":[],"cwd":"work"}"#;

#[test]
fn parses_full_scenario_with_every_op() {
    let steps = format!(
        r#"[
            {{"op":"mkdir","dir":{{"path":"work","mode":493}}}},
            {{"op":"write","file":{{"path":"work/a.txt","content":{{"utf8":"hi"}},"mode":420}}}},
            {{"op":"capture","name":"gh","path":"bin/gh","behavior":{{"stdout":"ok","stderr":"","exit_code":0,"stdin_limit":1024,"hang":false,"spawn_child_hang":false}}}},
            {LAUNCH},
            {{"op":"wait","source":"frame","literal":"ready","timeout_ms":5000}},
            {{"op":"key","key":"F5","modifiers":["control","shift"]}},
            {{"op":"text","text":"hello"}},
            {{"op":"resize","size":{{"cols":70,"rows":18}}}},
            {{"op":"assert-frame","contains":["ready"],"absent":["error"]}},
            {{"op":"assert-capture","capture":{{"name":"gh","invocation":1,"argv":["gh","pr"],"env":[{{"name":"HOME","value":"/w"}}],"cwd":"/w"}}}},
            {{"op":"assert-file","file":{{"path":"work/a.txt","content":{{"utf8":"hi"}}}}}},
            {{"op":"remove","path":"work/a.txt"}},
            {{"op":"restart"}},
            {{"op":"finish"}}
        ]"#
    );
    let scenario =
        parse(&minimal_with_steps(&steps)).unwrap_or_else(|err| panic!("should parse: {err}"));
    assert_eq!(scenario.name, "t");
    assert_eq!(scenario.platform, Platform::Linux);
    assert_eq!(scenario.terminal.cols, 100);
    assert_eq!(scenario.steps.len(), 14);
    let ops: Vec<&str> = scenario.steps.iter().map(Step::op_name).collect();
    assert_eq!(
        ops,
        [
            "mkdir",
            "write",
            "capture",
            "launch",
            "wait",
            "key",
            "text",
            "resize",
            "assert-frame",
            "assert-capture",
            "assert-file",
            "remove",
            "restart",
            "finish"
        ]
    );
    let Step::Key { modifiers, .. } = &scenario.steps[5] else {
        panic!("expected key step");
    };
    assert_eq!(modifiers, &[Modifier::Control, Modifier::Shift]);
    let Step::Wait { source, .. } = &scenario.steps[4] else {
        panic!("expected wait step");
    };
    assert_eq!(*source, WaitSource::Frame);
}

#[test]
fn missing_schema_is_rejected_before_anything_else() {
    // A pre-schema (legacy) scenario document must be rejected: there is no
    // adapter and no fallback.
    let legacy = r#"{"config":{"cols":80,"rows":24},"steps":[]}"#;
    let err = parse(legacy).err().unwrap_or_else(|| panic!("must fail"));
    assert_eq!(err.code, HarCode::E001);
    assert!(err.detail.contains("schema"), "{}", err.detail);
    assert_eq!(err.exit_code(), 2);
}

#[test]
fn wrong_schema_values_are_e001() {
    for schema in ["0", "2", "\"1\"", "true"] {
        let doc = format!(
            r#"{{"schema":{schema},"name":"t","platform":"linux",
                "terminal":{{"cols":1,"rows":1}},
                "workspace":{{"mode":448,"dirs":[],"files":[],"env":[]}},
                "steps":[{LAUNCH}],"secrets":[]}}"#
        );
        let err = parse(&doc)
            .err()
            .unwrap_or_else(|| panic!("{schema} must fail"));
        assert_eq!(err.code, HarCode::E001, "{schema}");
    }
}

#[test]
fn unknown_fields_are_rejected_everywhere() {
    let cases = [
        minimal_with_steps(&format!("[{LAUNCH}]"))
            .replace("\"secrets\":[]", "\"secrets\":[],\"extra\":1"),
        minimal_with_steps(&format!("[{LAUNCH}]")).replace(
            "\"terminal\":{\"cols\":100,\"rows\":30}",
            "\"terminal\":{\"cols\":100,\"rows\":30,\"depth\":8}",
        ),
        minimal_with_steps(
            r#"[{"op":"launch","argv":["app"],"env":[],"cwd":"work","shell":true}]"#,
        ),
    ];
    for doc in &cases {
        let err = parse(doc)
            .err()
            .unwrap_or_else(|| panic!("must fail: {doc}"));
        assert_eq!(err.code, HarCode::E001, "{doc}");
        assert!(err.detail.contains("unknown field"), "{}", err.detail);
    }
}

#[test]
fn duplicate_keys_are_e001() {
    let doc = minimal_with_steps(&format!("[{LAUNCH}]"))
        .replace("\"name\":\"t\"", "\"name\":\"t\",\"name\":\"u\"");
    let err = parse(&doc).err().unwrap_or_else(|| panic!("must fail"));
    assert_eq!(err.code, HarCode::E001);
    assert!(err.detail.contains("duplicate"), "{}", err.detail);
}

#[test]
fn terminal_bounds_at_limit_and_plus_one() {
    let valid = minimal_with_steps(&format!("[{LAUNCH}]"))
        .replace("\"cols\":100,\"rows\":30", "\"cols\":500,\"rows\":200");
    parse(&valid).unwrap_or_else(|err| panic!("at-limit should parse: {err}"));
    for (field, doc) in [
        (
            "cols",
            minimal_with_steps(&format!("[{LAUNCH}]")).replace("\"cols\":100", "\"cols\":501"),
        ),
        (
            "rows",
            minimal_with_steps(&format!("[{LAUNCH}]")).replace("\"rows\":30", "\"rows\":201"),
        ),
        (
            "rows-zero",
            minimal_with_steps(&format!("[{LAUNCH}]")).replace("\"rows\":30", "\"rows\":0"),
        ),
    ] {
        let err = parse(&doc)
            .err()
            .unwrap_or_else(|| panic!("{field} must fail"));
        assert_eq!(err.code, HarCode::E002, "{field}");
    }
}

#[test]
fn timeout_bounds_enforced() {
    let build = |timeout: &str| {
        minimal_with_steps(&format!(
            r#"[{LAUNCH},{{"op":"wait","source":"frame","literal":"x","timeout_ms":{timeout}}}]"#
        ))
    };
    parse(&build("30000")).unwrap_or_else(|err| panic!("at-limit should parse: {err}"));
    for timeout in ["0", "30001"] {
        let err = parse(&build(timeout))
            .err()
            .unwrap_or_else(|| panic!("must fail"));
        assert_eq!(err.code, HarCode::E002, "{timeout}");
    }
}

#[test]
fn workspace_mode_must_be_448() {
    let doc = minimal_with_steps(&format!("[{LAUNCH}]")).replace("\"mode\":448", "\"mode\":493");
    let err = parse(&doc).err().unwrap_or_else(|| panic!("must fail"));
    assert_eq!(err.code, HarCode::E001);
}

#[test]
fn file_modes_are_constrained() {
    let build = |mode: u32| {
        minimal_with_steps(&format!(
            r#"[{{"op":"write","file":{{"path":"a","content":{{"utf8":"x"}},"mode":{mode}}}}},{LAUNCH}]"#
        ))
    };
    for mode in [384, 420, 448, 493] {
        parse(&build(mode)).unwrap_or_else(|err| panic!("mode {mode} should parse: {err}"));
    }
    for mode in [0, 511, 449] {
        let err = parse(&build(mode))
            .err()
            .unwrap_or_else(|| panic!("must fail"));
        assert_eq!(err.code, HarCode::E001, "mode {mode}");
    }
}

#[test]
fn base64_content_decodes_and_malformed_fails() {
    let good = minimal_with_steps(&format!(
        r#"[{{"op":"write","file":{{"path":"a","content":{{"base64":"aGk="}},"mode":420}}}},{LAUNCH}]"#
    ));
    let scenario = parse(&good).unwrap_or_else(|err| panic!("should parse: {err}"));
    let Step::Write { file } = &scenario.steps[0] else {
        panic!("expected write");
    };
    assert_eq!(file.content, FileContent::Base64(b"hi".to_vec()));
    let bad = good.replace("aGk=", "aGk");
    let err = parse(&bad).err().unwrap_or_else(|| panic!("must fail"));
    assert_eq!(err.code, HarCode::E001);
}

#[test]
fn interpolation_violations_fail_at_parse_time() {
    let doc =
        minimal_with_steps(r#"[{"op":"launch","argv":["${home}/app"],"env":[],"cwd":"work"}]"#);
    let err = parse(&doc).err().unwrap_or_else(|| panic!("must fail"));
    assert_eq!(err.code, HarCode::E003);
    let env_doc = minimal_with_steps(
        r#"[{"op":"launch","argv":["app"],"env":[{"name":"X","value":"a${workspace}"}],"cwd":"work"}]"#,
    );
    let err = parse(&env_doc).err().unwrap_or_else(|| panic!("must fail"));
    assert_eq!(err.code, HarCode::E003);
}

#[test]
fn steps_must_be_non_empty_and_bounded() {
    let err = parse(&minimal_with_steps("[]"))
        .err()
        .unwrap_or_else(|| panic!("must fail"));
    assert_eq!(err.code, HarCode::E001);
    let mut steps: Vec<String> = vec![LAUNCH.to_string()];
    steps.extend(std::iter::repeat_n(
        r#"{"op":"text","text":"x"}"#.to_string(),
        1024,
    ));
    let doc = minimal_with_steps(&format!("[{}]", steps.join(",")));
    let err = parse(&doc).err().unwrap_or_else(|| panic!("must fail"));
    assert_eq!(err.code, HarCode::E002);
}

#[test]
fn semantic_rules_enforced() {
    // Terminal op before launch.
    let doc = minimal_with_steps(r#"[{"op":"text","text":"x"},{"op":"finish"}]"#);
    let err = parse(&doc).err().unwrap_or_else(|| panic!("must fail"));
    assert_eq!(err.code, HarCode::E001);
    // Duplicate capture name.
    let dup = minimal_with_steps(&format!(
        r#"[{{"op":"capture","name":"gh","path":"a","behavior":{{"stdout":"","stderr":"","exit_code":0,"stdin_limit":0,"hang":false,"spawn_child_hang":false}}}},
            {{"op":"capture","name":"gh","path":"b","behavior":{{"stdout":"","stderr":"","exit_code":0,"stdin_limit":0,"hang":false,"spawn_child_hang":false}}}},{LAUNCH}]"#
    ));
    let err = parse(&dup).err().unwrap_or_else(|| panic!("must fail"));
    assert_eq!(err.code, HarCode::E001);
    // assert-capture against unregistered name.
    let unreg = minimal_with_steps(&format!(
        r#"[{LAUNCH},{{"op":"assert-capture","capture":{{"name":"gh","invocation":1,"argv":[],"env":[],"cwd":"/w"}}}}]"#
    ));
    let err = parse(&unreg).err().unwrap_or_else(|| panic!("must fail"));
    assert_eq!(err.code, HarCode::E001);
    // Steps after finish.
    let after = minimal_with_steps(&format!(
        r#"[{LAUNCH},{{"op":"finish"}},{{"op":"text","text":"x"}}]"#
    ));
    let err = parse(&after).err().unwrap_or_else(|| panic!("must fail"));
    assert_eq!(err.code, HarCode::E001);
    // Second launch.
    let twice = minimal_with_steps(&format!("[{LAUNCH},{LAUNCH}]"));
    let err = parse(&twice).err().unwrap_or_else(|| panic!("must fail"));
    assert_eq!(err.code, HarCode::E001);
    // Duplicate workspace paths.
    let dup_paths = minimal_with_steps(&format!("[{LAUNCH}]")).replace(
        "\"dirs\":[]",
        r#""dirs":[{"path":"a","mode":448},{"path":"a","mode":493}]"#,
    );
    let err = parse(&dup_paths)
        .err()
        .unwrap_or_else(|| panic!("must fail"));
    assert_eq!(err.code, HarCode::E001);
    // Duplicate workspace env names.
    let dup_env = minimal_with_steps(&format!("[{LAUNCH}]")).replace(
        "\"env\":[]",
        r#""env":[{"name":"A","value":"1"},{"name":"A","value":"2"}]"#,
    );
    let err = parse(&dup_env).err().unwrap_or_else(|| panic!("must fail"));
    assert_eq!(err.code, HarCode::E001);

    // Capture executable and behavior paths cannot overwrite fixtures.
    for fixture_path in ["bin/gh", "bin/gh.capture.json"] {
        let collision = minimal_with_steps(&format!(
            r#"[{{"op":"capture","name":"gh","path":"bin/gh","behavior":{{"stdout":"","stderr":"","exit_code":0,"stdin_limit":0,"hang":false,"spawn_child_hang":false}}}},{LAUNCH}]"#
        ))
        .replace(
            r#""files":[]"#,
            &format!(
                r#""files":[{{"path":"{fixture_path}","content":{{"utf8":"keep"}},"mode":420}}]"#
            ),
        );
        let err = parse(&collision)
            .err()
            .unwrap_or_else(|| panic!("capture collision must fail"));
        assert_eq!(err.code, HarCode::E001, "{fixture_path}");
    }
}

#[test]
fn deterministic_plan_for_identical_input() {
    let doc = minimal_with_steps(&format!("[{LAUNCH}]"));
    let first = parse(&doc).unwrap_or_else(|err| panic!("should parse: {err}"));
    let second = parse(&doc).unwrap_or_else(|err| panic!("should parse: {err}"));
    assert_eq!(first, second);
}

#[test]
fn secret_rules_apply() {
    let doc =
        minimal_with_steps(&format!("[{LAUNCH}]")).replace("\"secrets\":[]", "\"secrets\":[\"\"]");
    let err = parse(&doc).err().unwrap_or_else(|| panic!("must fail"));
    assert_eq!(err.code, HarCode::E001);
}
