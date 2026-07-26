//! Focused tests for provider-free recovery source decisions.

use std::fs;

use super::inspect_import_decision;
use crate::persistence::paths::{InspectedSource, SourceValidity, physical_identity};

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
fn path_rejects_distinct_source_when_target_exists_regardless_of_bytes() {
    let directory = tempfile::tempdir().value_or_panic("create temporary directory");
    let target_path = directory.path().join("state.json");
    let source_path = directory.path().join("historical-state.json");
    fs::write(&target_path, b"same").value_or_panic("write target");

    for bytes in [b"same".as_slice(), b"different".as_slice()] {
        fs::write(&source_path, bytes).value_or_panic("write source");
        let target = physical_identity(&target_path).value_or_panic("identify target");
        let source = InspectedSource::new(
            source_path.clone(),
            physical_identity(&source_path).value_or_panic("identify source"),
            SourceValidity::Valid,
        );
        let Err(error) = inspect_import_decision(Some(&target), &[source]) else {
            panic!("distinct source must be ambiguous");
        };

        assert_eq!(error.exit_code, 3);
        assert_eq!(error.diagnostic.code.as_str(), "CFG-E001");
    }
}
