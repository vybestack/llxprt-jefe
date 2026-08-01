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

/// How many times a fallible probe may be asked, and how long to wait between.
///
/// Bounded by construction. A retry loop that can run forever converts a
/// permanent failure into a hang, which is a worse outcome than the wrong
/// answer it was added to prevent, so `attempts` is a hard ceiling and the
/// caller is handed the last uncertainty rather than being made to wait.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    attempts: u32,
    initial_backoff: std::time::Duration,
}

impl RetryPolicy {
    /// Build a policy, clamping `attempts` to at least one.
    ///
    /// Zero attempts would mean never asking, which is not a retry policy but
    /// a way to guarantee an unknown answer.
    #[must_use]
    pub fn new(attempts: u32, initial_backoff: std::time::Duration) -> Self {
        Self {
            attempts: attempts.max(1),
            initial_backoff,
        }
    }

    #[must_use]
    pub const fn attempts(&self) -> u32 {
        self.attempts
    }

    #[must_use]
    pub const fn initial_backoff(&self) -> std::time::Duration {
        self.initial_backoff
    }
}

impl Default for RetryPolicy {
    /// Three attempts with a 50ms doubling backoff: enough to ride out a
    /// transient subprocess failure at cold start without making startup feel
    /// stalled when the multiplexer really is gone.
    fn default() -> Self {
        Self::new(3, std::time::Duration::from_millis(50))
    }
}

/// Ask a fallible probe until it answers, or until the policy is exhausted.
///
/// Returns the *last* uncertainty rather than the first: if the reason for
/// failing changed between attempts, the most recent one describes the state
/// the caller is actually in.
///
/// `sleep` is injected so the backoff schedule is observable in a test without
/// spending the wall-clock time it describes.
pub fn retry_observation<T>(
    policy: RetryPolicy,
    mut probe: impl FnMut() -> Observed<T>,
    mut sleep: impl FnMut(std::time::Duration),
) -> Observed<T> {
    let mut backoff = policy.initial_backoff();
    let mut last = None;
    for attempt in 0..policy.attempts() {
        match probe() {
            Observed::Known(value) => return Observed::Known(value),
            Observed::Unknown(uncertainty) => last = Some(uncertainty),
        }
        // No trailing sleep: waiting after the final attempt delays the caller
        // without buying another observation.
        if attempt + 1 < policy.attempts() {
            sleep(backoff);
            backoff = backoff.saturating_mul(2);
        }
    }
    last.map_or_else(
        || {
            Observed::unknown(
                ProbeBoundary::SessionExists,
                "probe was never asked".to_owned(),
            )
        },
        Observed::Unknown,
    )
}
#[cfg(test)]
mod tests {
    use super::{Observed, ProbeBoundary, Resolution, RetryPolicy, Uncertainty, retry_observation};
    use std::time::Duration;

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

    /// A transient failure is exactly what #537 hit: one subprocess failure at
    /// cold start stranded a live agent. Retrying must let the answer arrive.
    #[test]
    fn a_probe_that_answers_on_a_later_attempt_is_not_held() {
        let mut calls = 0_u32;
        let mut slept = Vec::new();

        let observed = retry_observation(
            RetryPolicy::new(3, Duration::from_millis(10)),
            || {
                calls += 1;
                if calls < 3 {
                    Observed::unknown(ProbeBoundary::SessionExists, "transient")
                } else {
                    Observed::Known(7)
                }
            },
            |delay| slept.push(delay),
        );

        assert_eq!(observed.known(), Some(&7));
        assert_eq!(calls, 3, "it must stop asking once answered");
        assert_eq!(
            slept,
            vec![Duration::from_millis(10), Duration::from_millis(20)],
            "backoff doubles, and there is no wait after the answering attempt"
        );
    }

    /// The V4 mirror hazard for retries: bounded means bounded. A retry loop
    /// that never gives up turns a permanent failure into a hang.
    #[test]
    fn a_probe_that_never_answers_stops_asking() {
        let mut calls = 0_u32;
        let mut slept = 0_usize;

        let observed: Observed<u32> = retry_observation(
            RetryPolicy::new(4, Duration::from_millis(5)),
            || {
                calls += 1;
                Observed::unknown(ProbeBoundary::PaneList, format!("attempt {calls}"))
            },
            |_| slept += 1,
        );

        assert_eq!(calls, 4, "exactly the permitted number of attempts");
        assert_eq!(slept, 3, "no wait after the final attempt");
        assert_eq!(
            observed.uncertainty().map(Uncertainty::diagnostic),
            Some("attempt 4"),
            "the most recent reason describes the state the caller is in"
        );
    }

    /// An answer on the first attempt must cost nothing.
    #[test]
    fn an_immediate_answer_never_sleeps() {
        let mut slept = 0_usize;

        let observed = retry_observation(
            RetryPolicy::default(),
            || Observed::Known("ready"),
            |_| slept += 1,
        );

        assert!(observed.is_known());
        assert_eq!(slept, 0);
    }

    /// Zero attempts is not a policy, it is a guaranteed unknown.
    #[test]
    fn a_policy_always_asks_at_least_once() {
        let mut calls = 0_u32;

        let _observed: Observed<u32> = retry_observation(
            RetryPolicy::new(0, Duration::from_millis(1)),
            || {
                calls += 1;
                Observed::unknown(ProbeBoundary::DurableRead, "no")
            },
            |_| {},
        );

        assert_eq!(calls, 1, "the probe is always asked at least once");
    }
}
