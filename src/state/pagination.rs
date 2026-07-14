//! State-layer compatibility exports for the domain pagination container.
//!
//! Pagination state is defined in `domain` so transient detail domain types
//! can own a `PaginatedList` without reversing the project dependency DAG.

pub use crate::domain::{
    AcceptOutcome, BeginOutcome, LoadCorrelation, PageResult, PaginatedList, ReloadResult,
    ReloadVisibility, RequestIdExhausted,
};
