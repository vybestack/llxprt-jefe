//! Shared result/option diagnostics for the `jefe doctor` integration tests.
//!
//! Mirrors the no-unwrap/no-expect test style used by `tests/git_info/support.rs`
//! so every fallible test operation reports a descriptive context on panic
//! instead of relying on the standard library's unwrap message.

use std::fmt::Debug;

/// Extension trait that turns `Result` and `Option` into context-bearing
/// panics, keeping the test bodies free of `.unwrap()` / `.expect()`.
pub trait TestResultExt<T> {
    /// Returns the contained value or panics with `context` and the debug
    /// representation of the error/absence.
    fn test_unwrap(self, context: &str) -> T;
}

impl<T, E> TestResultExt<T> for Result<T, E>
where
    E: Debug,
{
    fn test_unwrap(self, context: &str) -> T {
        match self {
            Ok(value) => value,
            Err(error) => panic!("{context}: {error:?}"),
        }
    }
}

impl<T> TestResultExt<T> for Option<T> {
    fn test_unwrap(self, context: &str) -> T {
        match self {
            Some(value) => value,
            None => panic!("{context}: expected Some, found None"),
        }
    }
}
