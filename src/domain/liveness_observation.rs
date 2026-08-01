//! Fail-closed observation values for Jefe-owned liveness (issue #541).
//!
//! Every historical violation of the fail-closed invariant took the same shape:
//! a probe boundary that could fail, and a call site that resolved the failure
//! into whichever state was convenient there. `#305` fixed it for process
//! identity, `#527` marked twenty live panes stopped on a signature change,
//! `#445` failed open to an empty durable state, and `#537` stranded a live
//! agent on a single transient subprocess failure at cold start.
//!
//! The cause is not those four call sites. It is that nothing in the type
//! system distinguished "the probe says no" from "the probe did not answer",
//! so the distinction had to be remembered rather than enforced.
//!
//! [`Observed`] restores the distinction. It deliberately has **no**
//! `unwrap_or`, **no** `Default`, and **no** `From<Option<T>>`: each of those
//! is a way to supply a value where none was observed, which is the defect.
//! The only route from an observation to a state transition is
//! [`Observed::resolve`], whose decision closure never receives the uncertain
//! case.
//!
//! This mirrors the closed availability algebra the JSP/1 snapshot contract
//! already uses (`domain::observation::Availability`), but stays a separate
//! type on purpose: that module owns *producer-declared* field state, and its
//! documentation reserves process liveness to the Jefe runtime.

use std::fmt;

/// Why a probe could not answer.
///
/// Carried rather than discarded so the user gets an actionable state instead
/// of a silent hold. Deliverable 7 of issue #541 requires that an agent whose
/// status is unknown says so visibly, and says why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Uncertainty {
    boundary: ProbeBoundary,
    diagnostic: String,
}

impl Uncertainty {
    /// Record that `boundary` could not answer, with an operator-facing reason.
    #[must_use]
    pub fn new(boundary: ProbeBoundary, diagnostic: impl Into<String>) -> Self {
        Self {
            boundary,
            diagnostic: diagnostic.into(),
        }
    }

    /// Which probe boundary failed to answer.
    #[must_use]
    pub const fn boundary(&self) -> ProbeBoundary {
        self.boundary
    }

    /// The operator-facing reason this observation is uncertain.
    #[must_use]
    pub fn diagnostic(&self) -> &str {
        &self.diagnostic
    }
}

impl fmt::Display for Uncertainty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} did not answer: {}", self.boundary, self.diagnostic)
    }
}

/// The probe boundaries that can fail transiently.
///
/// Enumerated so a fault-injection harness can name the boundary it is driving
/// to failure, and so the verification matrix for V1 can assert coverage of
/// each one rather than of an unspecified "probe".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProbeBoundary {
    /// `has-session`.
    SessionExists,
    /// `list-sessions`.
    SessionList,
    /// `list-panes`.
    PaneList,
    /// `display-message`.
    ServerIdentity,
    /// Reading the durable state document.
    DurableRead,
    /// Querying an OS process identity.
    ProcessIdentity,
}

impl fmt::Display for ProbeBoundary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::SessionExists => "has-session",
            Self::SessionList => "list-sessions",
            Self::PaneList => "list-panes",
            Self::ServerIdentity => "display-message",
            Self::DurableRead => "durable state read",
            Self::ProcessIdentity => "process identity query",
        };
        f.write_str(name)
    }
}

/// What a fallible probe learned.
///
/// There is deliberately no method that produces a `T` from the uncertain case.
/// Reaching a value requires matching, and reaching a *transition* requires
/// [`resolve`](Observed::resolve).
///
/// A default would reintroduce exactly the defect this type exists to prevent,
/// so `Observed` does not implement [`Default`]:
///
/// ```compile_fail,E0277
/// use jefe::domain::liveness_observation::Observed;
/// let fallback: Observed<bool> = Default::default();
/// ```
///
/// Nor can an uncertain observation be unwrapped into a value the way an
/// `Option` can:
///
/// ```compile_fail,E0599
/// use jefe::domain::liveness_observation::Observed;
/// let observed: Observed<bool> = Observed::Known(true);
/// let value = observed.unwrap_or(false);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Observed<T> {
    /// The probe answered, and the answer is authoritative.
    Known(T),
    /// The probe did not answer. This is terminal: it produces no transition.
    Unknown(Uncertainty),
}

impl<T> Observed<T> {
    /// Record that `boundary` failed to answer.
    #[must_use]
    pub fn unknown(boundary: ProbeBoundary, diagnostic: impl Into<String>) -> Self {
        Self::Unknown(Uncertainty::new(boundary, diagnostic))
    }

    /// The observed value, if the probe answered.
    ///
    /// Returns `None` for the uncertain case rather than a substitute, so a
    /// caller that wants a value has to say what it intends to do without one.
    #[must_use]
    pub const fn known(&self) -> Option<&T> {
        match self {
            Self::Known(value) => Some(value),
            Self::Unknown(_) => None,
        }
    }

    /// Why this observation is uncertain, if it is.
    #[must_use]
    pub const fn uncertainty(&self) -> Option<&Uncertainty> {
        match self {
            Self::Known(_) => None,
            Self::Unknown(reason) => Some(reason),
        }
    }

    /// Whether the probe answered.
    #[must_use]
    pub const fn is_known(&self) -> bool {
        matches!(self, Self::Known(_))
    }

    /// Resolve this observation into a state transition.
    ///
    /// `decide` is only invoked for an answered probe, so an unknown
    /// observation cannot reach a transition by any path through this type.
    /// That is the type-level form of the invariant: *an unknown observation
    /// must never cause a state transition.*
    pub fn resolve<S>(self, decide: impl FnOnce(T) -> S) -> Resolution<S> {
        match self {
            Self::Known(value) => Resolution::Transition(decide(value)),
            Self::Unknown(reason) => Resolution::Hold(reason),
        }
    }

    /// Transform an answered observation, preserving the uncertain case.
    pub fn map<U>(self, transform: impl FnOnce(T) -> U) -> Observed<U> {
        match self {
            Self::Known(value) => Observed::Known(transform(value)),
            Self::Unknown(reason) => Observed::Unknown(reason),
        }
    }
}

/// The outcome of resolving an observation.
///
/// `Hold` carries the reason rather than being a bare "do nothing", so a caller
/// can surface why the agent's state was left alone and offer a re-probe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution<S> {
    /// The observation was conclusive; apply this transition.
    Transition(S),
    /// The observation was inconclusive. Preserve the current state and every
    /// binding, and tell the user why.
    Hold(Uncertainty),
}

impl<S> Resolution<S> {
    /// The transition to apply, if there is one.
    #[must_use]
    pub const fn transition(&self) -> Option<&S> {
        match self {
            Self::Transition(state) => Some(state),
            Self::Hold(_) => None,
        }
    }

    /// Why no transition was produced, if none was.
    #[must_use]
    pub const fn held(&self) -> Option<&Uncertainty> {
        match self {
            Self::Transition(_) => None,
            Self::Hold(reason) => Some(reason),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Observed, ProbeBoundary, Resolution, Uncertainty};

    /// The whole point of the type: the decision closure never runs for an
    /// observation that did not answer, so no call site can transition on one.
    #[test]
    fn an_unknown_observation_never_reaches_the_decision() {
        let observed: Observed<bool> =
            Observed::unknown(ProbeBoundary::SessionList, "psmux exited with status 1");

        let resolution = observed.resolve(|_| panic!("a transition was decided without evidence"));

        assert!(
            matches!(resolution, Resolution::Hold(_)),
            "an unanswered probe must hold, not transition"
        );
    }

    #[test]
    fn an_answered_observation_reaches_the_decision() {
        let observed = Observed::Known(true);

        let resolution = observed.resolve(|alive| if alive { "running" } else { "stopped" });

        assert_eq!(resolution.transition(), Some(&"running"));
        assert_eq!(resolution.held(), None);
    }

    /// Deliverable 7: the user must be able to see *why* jefe does not know.
    #[test]
    fn holding_carries_an_operator_facing_reason() {
        let observed: Observed<bool> = Observed::unknown(
            ProbeBoundary::PaneList,
            "timed out after 2s waiting for list-panes",
        );

        let resolution = observed.resolve(|_| "unreachable");

        let held = resolution
            .held()
            .unwrap_or_else(|| panic!("an unanswered probe must explain itself"));
        assert_eq!(held.boundary(), ProbeBoundary::PaneList);
        assert!(
            held.to_string().contains("list-panes"),
            "the reason must name the boundary that failed: {held}"
        );
        assert!(
            held.to_string().contains("timed out"),
            "the reason must carry the diagnostic: {held}"
        );
    }

    /// `known()` must not invent a value for the uncertain case. This is the
    /// property that `unwrap_or_default()` violated at every historical site.
    #[test]
    fn an_unknown_observation_yields_no_value() {
        let observed: Observed<u32> =
            Observed::unknown(ProbeBoundary::ProcessIdentity, "access denied");

        assert_eq!(observed.known(), None);
        assert!(!observed.is_known());
    }

    /// Mapping must not quietly resolve uncertainty either.
    #[test]
    fn mapping_preserves_the_uncertain_case() {
        let observed: Observed<u32> =
            Observed::unknown(ProbeBoundary::DurableRead, "state.json was mid-write");

        let mapped = observed.map(|pid| pid > 0);

        assert_eq!(
            mapped.uncertainty().map(Uncertainty::boundary),
            Some(ProbeBoundary::DurableRead),
            "mapping an unanswered probe must stay unanswered"
        );
    }

    /// Every boundary the fault-injection harness must be able to drive (V1)
    /// names itself, so a test failure says which probe was being exercised.
    #[test]
    fn every_probe_boundary_names_itself() {
        for boundary in [
            ProbeBoundary::SessionExists,
            ProbeBoundary::SessionList,
            ProbeBoundary::PaneList,
            ProbeBoundary::ServerIdentity,
            ProbeBoundary::DurableRead,
            ProbeBoundary::ProcessIdentity,
        ] {
            assert!(
                !boundary.to_string().is_empty(),
                "{boundary:?} must name itself for fault-injection diagnostics"
            );
        }
    }
}
