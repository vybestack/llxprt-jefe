//! Shared panic diagnostics for unit tests that exercise fallible contracts.

use std::fmt::Debug;

pub trait Must<T> {
    fn must(self, context: &str) -> T;
}

impl<T, E: Debug> Must<T> for Result<T, E> {
    fn must(self, context: &str) -> T {
        self.unwrap_or_else(|error| panic!("{context}: {error:?}"))
    }
}

impl<T> Must<T> for Option<T> {
    fn must(self, context: &str) -> T {
        self.unwrap_or_else(|| panic!("{context}"))
    }
}

pub trait MustErr<E> {
    fn must_err(self, context: &str) -> E;
}

impl<T: Debug, E> MustErr<E> for Result<T, E> {
    fn must_err(self, context: &str) -> E {
        match self {
            Ok(value) => panic!("{context}: unexpectedly succeeded with {value:?}"),
            Err(error) => error,
        }
    }
}
