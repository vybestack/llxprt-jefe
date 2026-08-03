//! Tests for the hosted-PTY terminal-model event sink.

use super::forward_clipboard_store;

/// Embedded OSC 52 events must use the project clipboard boundary.
#[test]
fn clipboard_store_uses_injected_project_boundary() {
    let mut observed = String::new();
    let result = forward_clipboard_store("Ω clipboard", |text| {
        observed.push_str(text);
        Ok(())
    });

    assert!(
        result.is_ok(),
        "clipboard boundary should succeed: {result:?}"
    );
    assert_eq!(observed, "Ω clipboard");
}

#[test]
fn clipboard_store_provider_failure_is_recoverable() {
    let result = forward_clipboard_store("copy", |_| {
        Err(std::io::Error::new(
            std::io::ErrorKind::NotConnected,
            "provider unavailable",
        ))
    });

    assert!(
        result.is_err(),
        "provider failure must be surfaced without panicking"
    );
}

#[test]
fn clipboard_store_ignores_empty_payload() {
    let mut called = false;
    let result = forward_clipboard_store("", |_| {
        called = true;
        Ok(())
    });

    assert!(result.is_ok());
    assert!(!called, "empty OSC 52 stores must not invoke the provider");
}
