//! Shared result diagnostics for git-info integration tests.

use std::fmt::Debug;

pub trait TestResultExt<T> {
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
