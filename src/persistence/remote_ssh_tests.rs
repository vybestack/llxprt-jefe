//! Persistence compatibility tests for remote OpenSSH settings.

use crate::domain::RemoteRepositorySettings;

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

#[test]
fn remote_ssh_settings_round_trip_and_legacy_defaults_are_compatible() {
    let configured = RemoteRepositorySettings {
        enabled: true,
        login_user: "ubuntu".to_owned(),
        host: "linux.example".to_owned(),
        port: Some(2222),
        identity_file: std::path::PathBuf::from(r"C:\Keys Ω\agent key"),
        options: vec!["Compression=yes".to_owned()],
        ..RemoteRepositorySettings::default()
    };
    let encoded = serde_json::to_string(&configured).value_or_panic("serialize SSH settings");
    let decoded: RemoteRepositorySettings =
        serde_json::from_str(&encoded).value_or_panic("deserialize SSH settings");
    assert_eq!(decoded, configured);

    let legacy = r#"{"enabled":true,"login_user":"ubuntu","host":"linux.example","run_as_user":"","setup_env_default":false}"#;
    let decoded: RemoteRepositorySettings =
        serde_json::from_str(legacy).value_or_panic("deserialize legacy SSH settings");
    assert_eq!(decoded.port, None);
    assert!(decoded.identity_file.as_os_str().is_empty());
    assert!(decoded.options.is_empty());
}
