//! Test-only panic helpers for clippy-clean assertions.

use std::fmt::Debug;

pub trait TestOptionExt<T> {
    fn test_unwrap(self, context: &str) -> T;
}

impl<T> TestOptionExt<T> for Option<T> {
    fn test_unwrap(self, context: &str) -> T {
        match self {
            Some(value) => value,
            None => panic!("{context}"),
        }
    }
}

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
