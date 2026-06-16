//! Clippy allow policy contract tests.

#[test]
fn clippy_allow_policy_script_passes() {
    let output = std::process::Command::new("bash")
        .arg("scripts/check-clippy-allows.sh")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output();
    let Ok(output) = output else {
        panic!("clippy allow policy script should run");
    };

    assert!(
        output.status.success(),
        "clippy allow policy script failed
stdout:
{}
stderr:
{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
