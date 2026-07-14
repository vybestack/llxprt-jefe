//! Guarded behavioral coverage for a disposable remote Linux SSH fixture.
//!
//! The test is inert unless `JEFE_REAL_SSH_HOST` and `JEFE_REAL_SSH_USER` are
//! configured. It creates one uniquely named upstream tmux session and one
//! uniquely named remote path, attaches through Jefe's shell-free plan, and
//! cleans only those run-owned resources.

use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use jefe::domain::RemoteRepositorySettings;
use jefe::ssh::{SSH_OPERATION_TIMEOUT, SshCancellation, SshMode, SshPlan};

struct RemoteFixtureGuard {
    settings: RemoteRepositorySettings,
    session: String,
    path: String,
}

impl RemoteFixtureGuard {
    fn execute(&self, command: &str) -> Result<std::process::Output, jefe::ssh::SshError> {
        SshPlan::new(&self.settings, command, SshMode::NonInteractive)?.execute(
            None,
            SSH_OPERATION_TIMEOUT,
            None,
        )
    }
}

impl Drop for RemoteFixtureGuard {
    fn drop(&mut self) {
        let command = format!(
            "tmux kill-session -t '{}' >/dev/null 2>&1 || :; rm -rf -- '{}'",
            self.session, self.path
        );
        if let Err(error) = self.execute(&command) {
            tracing::error!(%error, "failed to clean the owned real-SSH fixture");
        }
    }
}

fn configured_fixture() -> Result<Option<RemoteRepositorySettings>, String> {
    let Ok(host) = std::env::var("JEFE_REAL_SSH_HOST") else {
        return Ok(None);
    };
    let Ok(login_user) = std::env::var("JEFE_REAL_SSH_USER") else {
        return Ok(None);
    };
    let port = match std::env::var("JEFE_REAL_SSH_PORT") {
        Ok(value) => Some(
            value
                .parse::<u16>()
                .map_err(|error| format!("invalid JEFE_REAL_SSH_PORT: {error}"))?,
        ),
        Err(std::env::VarError::NotPresent) => None,
        Err(error) => return Err(format!("invalid JEFE_REAL_SSH_PORT: {error}")),
    };
    let identity_file = std::env::var_os("JEFE_REAL_SSH_IDENTITY")
        .map(PathBuf::from)
        .unwrap_or_default();
    Ok(Some(RemoteRepositorySettings {
        enabled: true,
        login_user,
        host,
        port,
        identity_file,
        ..RemoteRepositorySettings::default()
    }))
}

fn wait_for_attached_client(guard: &RemoteFixtureGuard, session: &str) -> Result<(), String> {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let command = format!("tmux list-clients -t '{session}' -F '#{{client_name}}'");
        match guard.execute(&command) {
            Ok(output) if output.status.success() && !output.stdout.is_empty() => return Ok(()),
            Ok(_) | Err(_) if std::time::Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Ok(_) | Err(_) => {
                return Err("remote tmux client did not attach before deadline".to_owned());
            }
        }
    }
}

#[test]
fn guarded_windows_real_ssh_launches_attaches_and_cleans_owned_resources() {
    if !cfg!(windows) {
        tracing::info!("skipping Windows real-SSH fixture test on a non-Windows host");
        return;
    }
    let configured =
        configured_fixture().unwrap_or_else(|error| panic!("configure real-SSH fixture: {error}"));
    let Some(settings) = configured else {
        tracing::info!(
            "skipping real-SSH fixture test; configure JEFE_REAL_SSH_HOST and JEFE_REAL_SSH_USER"
        );
        return;
    };
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let session = format!("jefe-ssh-test-{}-{nonce}", std::process::id());
    let path = format!("/tmp/{session}");
    let guard = RemoteFixtureGuard {
        settings,
        session: session.clone(),
        path: path.clone(),
    };

    let launch = format!(
        "set -e; mkdir -p '{path}'; tmux new-session -d -s '{session}' \"printf 'JEFE_REMOTE_READY\\n'; exec sleep 120\""
    );
    let output = guard
        .execute(&launch)
        .unwrap_or_else(|error| panic!("launch deterministic remote agent: {error}"));
    assert!(output.status.success());

    let attach = jefe::runtime::build_remote_attach_plan(&guard.settings, &session)
        .unwrap_or_else(|error| panic!("plan remote attach: {error}"));
    let cancellation = SshCancellation::default();
    let attach_cancellation = cancellation.clone();
    let attach_thread = std::thread::spawn(move || {
        attach.execute(None, Duration::from_secs(10), Some(&attach_cancellation))
    });
    wait_for_attached_client(&guard, &session)
        .unwrap_or_else(|error| panic!("observe remote tmux attachment: {error}"));
    cancellation.cancel();
    let attached = attach_thread
        .join()
        .unwrap_or_else(|_| panic!("join remote attach thread"));
    assert!(matches!(attached, Err(jefe::ssh::SshError::Cancelled)));

    let verify = guard
        .execute(&format!("tmux capture-pane -pt '{session}'"))
        .unwrap_or_else(|error| panic!("capture remote fixture pane: {error}"));
    assert!(verify.status.success());
    assert!(String::from_utf8_lossy(&verify.stdout).contains("JEFE_REMOTE_READY"));
}
