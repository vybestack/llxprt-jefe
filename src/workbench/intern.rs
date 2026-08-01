//! Bounded identifier interning for lowered custom screens (issue #385).
//!
//! Every descriptor identifier is a [`Copy`] newtype over a `&'static str`, so
//! a resolved layout can be rebuilt every frame without cloning strings. Screens
//! compiled into the executable satisfy that trivially. A screen lowered from a
//! user file does not: its identifier text arrives at runtime.
//!
//! Composition happens exactly once, into a registry that is published for the
//! remainder of the process, so a lowered identifier genuinely does live for the
//! whole program. This module makes that lifetime explicit and gives it a
//! ceiling: identical text is interned once, and the table refuses to grow past
//! [`MAX_INTERNED_IDENTIFIERS`], so a hostile or accidental definitions
//! directory cannot turn a bounded parse into unbounded resident memory.
//!
//! Nothing here parses or validates. Callers intern text that has already passed
//! the identifier grammar, which is why the only failure is exhaustion.

use std::collections::BTreeSet;
use std::sync::{Mutex, OnceLock, PoisonError};

/// Ceiling on distinct interned identifier strings.
///
/// The worst legal definitions directory declares [`MAX_SCREENS`] screens, each
/// with an identity and a route, [`MAX_PANELS_PER_SCREEN`] panels, and for each
/// panel an identity, a panel type, and [`MAX_PORTS_PER_PANEL`] ports with a
/// port identity and a versioned type. That is 64 * (2 + 16 * (2 + 32 * 2)) =
/// 67,712 strings before deduplication, so this ceiling admits every legal
/// directory and rejects anything past it.
///
/// [`MAX_SCREENS`]: super::ids::MAX_SCREENS
/// [`MAX_PANELS_PER_SCREEN`]: super::ids::MAX_PANELS_PER_SCREEN
/// [`MAX_PORTS_PER_PANEL`]: super::ids::MAX_PORTS_PER_PANEL
pub const MAX_INTERNED_IDENTIFIERS: usize = 67_712;

/// The interning table refused to admit another distinct string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InternExhausted {
    /// How many distinct strings were already resident.
    pub resident: usize,
}

impl std::fmt::Display for InternExhausted {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "identifier table holds {} entries (max {MAX_INTERNED_IDENTIFIERS})",
            self.resident
        )
    }
}

impl std::error::Error for InternExhausted {}

static TABLE: OnceLock<Mutex<BTreeSet<&'static str>>> = OnceLock::new();

fn table() -> &'static Mutex<BTreeSet<&'static str>> {
    TABLE.get_or_init(|| Mutex::new(BTreeSet::new()))
}

/// Return the process-lifetime copy of `value`, interning it if it is new.
///
/// Interning the same text twice returns the same pointer, so identifiers
/// lowered from two files that name the same panel type compare equal by value
/// and share one allocation.
///
/// # Errors
///
/// Returns [`InternExhausted`] when admitting a new distinct string would push
/// the table past [`MAX_INTERNED_IDENTIFIERS`].
pub fn intern(value: &str) -> Result<&'static str, InternExhausted> {
    // A poisoned table is still a correct set of live `'static` strings: the
    // only mutation is an insert, and a panic between the read and the insert
    // cannot leave a torn entry. Recovering the guard keeps one failed
    // composition from making every later one fail too.
    let mut resident = table().lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(existing) = resident.get(value) {
        return Ok(existing);
    }
    if resident.len() >= MAX_INTERNED_IDENTIFIERS {
        return Err(InternExhausted {
            resident: resident.len(),
        });
    }
    let leaked: &'static str = Box::leak(value.to_owned().into_boxed_str());
    resident.insert(leaked);
    drop(resident);
    Ok(leaked)
}

/// How many distinct strings the table currently holds.
#[must_use]
pub fn resident_count() -> usize {
    table().lock().unwrap_or_else(PoisonError::into_inner).len()
}
