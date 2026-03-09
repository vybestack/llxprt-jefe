//! Runtime preflight checks for sandbox launch ergonomics.

use std::path::Path;
use std::process::Command;

/// Return a user-facing warning when sandbox SSH forwarding is likely to fail.
///
/// This check is intentionally conservative and non-fatal: it surfaces common
/// host-side issues early (missing/empty SSH agent identities) while still
/// allowing session launch to proceed.
#[must_use]
pub fn sandbox_ssh_agent_warning() -> Option<String> {
    let ssh_auth_sock = std::env::var("SSH_AUTH_SOCK").ok()?;

    if ssh_auth_sock.trim().is_empty() {
        return Some("SSH_AUTH_SOCK is set but empty; SSH auth may fail in sandbox.".to_owned());
    }

    if !Path::new(&ssh_auth_sock).exists() {
        return Some(format!(
            "SSH_AUTH_SOCK points to missing path ({ssh_auth_sock}); SSH auth may fail in sandbox."
        ));
    }

    let output = Command::new("ssh-add").arg("-l").output().ok()?;
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
    .to_ascii_lowercase();

    if combined.contains("the agent has no identities") {
        return Some(
            "SSH agent socket is present but no identities are loaded. Run `ssh-add ~/.ssh/id_ed25519` (or your key) and relaunch the sandbox session.".to_owned(),
        );
    }

    None
}
