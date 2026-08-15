use super::*;

#[test]
fn inventory_mutations_are_rejected() {
    let manifest = load_manifest();

    let mut missing = manifest.clone();
    missing.scenarios.pop();
    assert_validation_contains(
        &missing,
        "manifest paths differ from shipped scenario paths",
    );

    let mut duplicate = manifest.clone();
    duplicate.scenarios.push(duplicate.scenarios[0].clone());
    assert_validation_contains(
        &duplicate,
        "scenario paths must be strictly sorted and unique",
    );

    let mut unsorted = manifest.clone();
    unsorted.scenarios.swap(0, 1);
    assert_validation_contains(
        &unsorted,
        "scenario paths must be strictly sorted and unique",
    );
}

#[test]
fn exact_failure_and_capture_mutations_are_rejected() {
    let manifest = load_manifest();

    let mut failure = manifest.clone();
    let containment = failure
        .scenarios
        .iter_mut()
        .find(|entry| entry.path.ends_with("harness-containment.json"))
        .unwrap_or_else(|| panic!("containment scenario must be classified"));
    containment
        .expect
        .failed_step
        .as_mut()
        .unwrap_or_else(|| panic!("containment failure must be projected"))
        .index = 5;
    assert_validation_contains(&failure, "failure expectation differs from scenario");

    let mut captures = manifest.clone();
    let timeout = captures
        .scenarios
        .iter_mut()
        .find(|entry| entry.path.ends_with("harness-timeout.json"))
        .unwrap_or_else(|| panic!("timeout scenario must be classified"));
    timeout.expect.capture_names[0] = "wrong-capture".to_string();
    assert_validation_contains(&captures, "capture inventory differs from scenario");
}

#[test]
fn classification_mutations_are_rejected() {
    let manifest = load_manifest();

    let mut reasonless = manifest.clone();
    reasonless.scenarios[0]
        .platforms
        .get_mut("windows")
        .unwrap_or_else(|| panic!("windows disposition must exist"))
        .reason = None;
    assert_validation_contains(
        &reasonless,
        "unsupported windows disposition needs a reason",
    );

    let mut old_runner = manifest.clone();
    old_runner.scenarios[0].command.binary = concat!("jefe-tmux", "-harness").to_string();
    assert_validation_contains(&old_runner, "command binary must be tmux_scenario");

    let mut unsafe_install = manifest.clone();
    let installed = unsafe_install
        .scenarios
        .iter_mut()
        .find(|entry| !entry.command.installs.is_empty())
        .unwrap_or_else(|| panic!("at least one scenario must install a binary"));
    installed.command.installs[0].name = "../escape".to_string();
    assert_validation_contains(&unsafe_install, "has invalid name");

    let mut stale_assertions = manifest.clone();
    stale_assertions.scenarios[0].expect.captures += 1;
    assert_validation_contains(&stale_assertions, "capture inventory differs from scenario");

    let issue493_index = manifest
        .scenarios
        .iter()
        .position(|entry| entry.path == ISSUE493_PATH)
        .unwrap_or_else(|| panic!("{ISSUE493_PATH} must be classified"));
    let mut wrong_issue493_reason = manifest.clone();
    wrong_issue493_reason.scenarios[issue493_index]
        .platforms
        .get_mut("macos")
        .unwrap_or_else(|| panic!("macos disposition must exist"))
        .reason = Some("generic unsupported scenario".to_string());
    assert_validation_contains(
        &wrong_issue493_reason,
        "must be excluded from Unix execution and owned by native Windows psmux evidence",
    );

    let mut wrong_issue493_owner = manifest.clone();
    wrong_issue493_owner.scenarios[issue493_index].ci_job = "tui_scenarios_macos".to_string();
    assert_validation_contains(
        &wrong_issue493_owner,
        "must be excluded from Unix execution and owned by native Windows psmux evidence",
    );

    let mut generic_skip = manifest.clone();
    for disposition in generic_skip.scenarios[0].platforms.values_mut() {
        disposition.disposition = "unsupported".to_string();
        disposition.reason = Some("not exercised".to_string());
    }
    assert_validation_contains(
        &generic_skip,
        "required platform differs from scenario declaration",
    );
}
