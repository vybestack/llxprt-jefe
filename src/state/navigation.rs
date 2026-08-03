//! The sole owner of route, stack, and dirty transitions (issue #386).
//!
//! Every screen change in the program is one call to [`reduce_navigation`].
//! Before this existed each mode moved the session itself — issues mode set the
//! screen on the way in and set it back on the way out, and so did pull
//! requests, actions, errors, and the terminal manager — which meant "where am
//! I" had as many authorities as there were modes, and none of them could
//! answer "where was I before".
//!
//! The reducer is pure: it takes the navigation state, the screen registry, and
//! one intent, and returns the next state plus what the caller must do about
//! the instances that entered or left. It performs no I/O, holds no handle, and
//! stages no effect of its own, so a refused navigation is indistinguishable
//! from one that never happened.
//!
//! Three rules make partial mutation impossible:
//!
//! - a target is validated and constructed **before** anything is suspended or
//!   disposed, so a refusal returns the state it was given, unchanged;
//! - an instance identity is never reused, so a completion issued by an
//!   instance that has gone can never be mistaken for one the live instance
//!   issued;
//! - suspension is a type, not a flag: the stack holds
//!   [`SuspendedInstance`], which cannot be read as the current instance by
//!   accident.

use std::fmt;

use crate::workbench::{
    ActivationError, ActivationValues, NavCode, PanelId, RouteId, ScreenId, ScreenIdentity,
    ScreenInstanceId, ScreenRegistry, route_declaration,
};

/// Maximum number of suspended instances the navigation stack may hold.
pub const MAX_NAVIGATION_STACK: usize = 32;

/// What an instance was activated with, and which instance asked for it.
///
/// The two identity fields have one job at each end of the activation's life.
/// On a request they say which snapshot the request was computed from, so a
/// request produced against a screen that has since been replaced can be
/// refused instead of acted on. On the instance that was entered they are
/// provenance: which instance asked, and at which activation generation this
/// instance's own effects are correlated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Activation {
    /// The route this activation reaches.
    pub route: RouteId,
    /// The validated field values the route accepts.
    pub values: ActivationValues,
    /// The instance the request was computed from.
    pub source_instance: ScreenInstanceId,
    /// On a request, the source's activation generation; on an entered
    /// instance, that instance's own activation generation.
    pub activation_generation: u64,
}

impl Activation {
    /// Build a request against the snapshot `source` was read from.
    #[must_use]
    pub fn from_source(route: RouteId, values: ActivationValues, source: &ScreenInstance) -> Self {
        Self {
            route,
            values,
            source_instance: source.id,
            activation_generation: source.activation.activation_generation,
        }
    }
}

/// One live screen instance.
///
/// An instance is the session's presence on a screen: which screen, what it was
/// activated with, where focus sits, and which generation its in-flight work is
/// correlated against. Two visits to the same screen are two instances, so the
/// second never inherits the first's pending answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenInstance {
    /// Process-unique identity, never reused.
    pub id: ScreenInstanceId,
    /// The screen this instance is on.
    pub screen: ScreenId,
    /// What this instance was activated with.
    pub activation: Activation,
    /// The panel that holds focus.
    pub panel_focus: PanelId,
    /// Screen generation; a completion naming an older one is stale.
    pub generation: u64,
}

/// An instance whose subscriptions are suspended while it waits on the stack.
///
/// Wrapping it keeps the distinction in the type system: a suspended instance
/// is not a candidate for "what is the session doing now", and its pending work
/// is not answered until it is restored — by which time its generation has been
/// left behind and the answers are stale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuspendedInstance(ScreenInstance);

impl SuspendedInstance {
    /// The exact instance that was suspended.
    #[must_use]
    pub const fn instance(&self) -> &ScreenInstance {
        &self.0
    }
}

/// What the session is asking navigation to do.
#[derive(Debug, Clone)]
pub enum NavIntent {
    /// Suspend the current instance and enter the activation's target.
    Push(Activation),
    /// Dispose the current instance and enter the activation's target.
    Replace(Activation),
    /// Leave the current instance and restore the one beneath it.
    Back,
}

/// Route, stack, and the instance the session is on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavState {
    current: ScreenInstance,
    stack: Vec<SuspendedInstance>,
    next_generation: u64,
    next_activation_generation: u64,
}

impl NavState {
    /// The state a session starts in: one clean instance, no stack.
    ///
    /// # Errors
    ///
    /// Returns [`NavRefusal::MissingDescriptor`] when `screen` has no compiled
    /// descriptor, which is a malformed compiled table rather than anything a
    /// user did.
    pub fn rooted(registry: &ScreenRegistry, screen: ScreenId) -> Result<Self, NavRefusal> {
        let Some(descriptor) = registry.get(screen) else {
            return Err(NavRefusal::MissingDescriptor { screen });
        };
        let id = ScreenInstanceId::next();
        Ok(Self {
            current: ScreenInstance {
                id,
                screen,
                activation: Activation {
                    route: descriptor.route,
                    values: ActivationValues::empty(),
                    // The root instance was activated by nothing but itself.
                    source_instance: id,
                    activation_generation: 1,
                },
                panel_focus: descriptor.initial_focus,
                generation: 1,
            },
            stack: Vec::new(),
            next_generation: 2,
            next_activation_generation: 2,
        })
    }

    /// The instance the session is on.
    #[must_use]
    pub const fn current(&self) -> &ScreenInstance {
        &self.current
    }

    /// The instance the session is on, for the owner of its focus and dirtiness.
    pub const fn current_mut(&mut self) -> &mut ScreenInstance {
        &mut self.current
    }

    /// The suspended instances, oldest first.
    #[must_use]
    pub fn suspended(&self) -> &[SuspendedInstance] {
        &self.stack
    }

    /// How many instances are suspended beneath the current one.
    #[must_use]
    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    /// The screen the session is on.
    #[must_use]
    pub const fn screen(&self) -> ScreenId {
        self.current.screen
    }

    /// The generations work must name to still be answerable.
    ///
    /// This pair is what a pending effect's correlation is registered with, and
    /// what its completion is checked against.
    #[must_use]
    pub const fn live_generations(&self) -> (u64, u64) {
        (
            self.current.generation,
            self.current.activation.activation_generation,
        )
    }

    /// Whether work correlated at these generations belongs to the live instance.
    ///
    /// This is the whole staleness rule, and it needs no bookkeeping beyond the
    /// generations themselves. A suspended instance's generations are not the
    /// live ones, so its answers are ignored while it waits; restoring it makes
    /// them live again, which is what "Back restores the instance's
    /// subscriptions" means in practice. A disposed instance's generations
    /// never become live again, because generations only ever move forward and
    /// a disposed instance is never restored — so its answers are ignored
    /// permanently rather than applied to whatever took its place.
    #[must_use]
    pub const fn answers_live_work(
        &self,
        screen_generation: u64,
        activation_generation: u64,
    ) -> bool {
        let (live_screen, live_activation) = self.live_generations();
        screen_generation == live_screen && activation_generation == live_activation
    }

    /// Build the instance an activation names, without disturbing this state.
    fn construct(
        &self,
        registry: &ScreenRegistry,
        activation: &Activation,
    ) -> Result<ScreenInstance, NavRefusal> {
        if activation.source_instance != self.current.id
            || activation.activation_generation != self.current.activation.activation_generation
        {
            return Err(NavRefusal::StaleSource {
                supplied: activation.source_instance,
                live: self.current.id,
            });
        }
        let declaration = route_declaration(registry, activation.route)?;
        declaration.validate(&activation.values)?;
        let ScreenIdentity::Compiled(screen) = declaration.target_screen else {
            return Err(NavRefusal::NotRoutable {
                route: activation.route,
            });
        };
        let Some(descriptor) = registry.get(screen) else {
            return Err(NavRefusal::MissingDescriptor { screen });
        };
        if self.next_generation.checked_add(1).is_none()
            || self.next_activation_generation.checked_add(1).is_none()
        {
            return Err(NavRefusal::GenerationExhausted);
        }
        Ok(ScreenInstance {
            id: ScreenInstanceId::next(),
            screen,
            activation: Activation {
                route: activation.route,
                values: activation.values.clone(),
                source_instance: activation.source_instance,
                activation_generation: self.next_activation_generation,
            },
            panel_focus: descriptor.initial_focus,
            generation: self.next_generation,
        })
    }

    /// Advance the monotonic counters past the instance just entered.
    fn advance(&mut self) {
        self.next_generation = self.next_generation.saturating_add(1);
        self.next_activation_generation = self.next_activation_generation.saturating_add(1);
    }
}

/// What a committed navigation left for its caller to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavOutcome {
    /// `suspended` went onto the stack and `entered` became current.
    Pushed {
        /// The instance whose subscriptions are now suspended.
        suspended: ScreenInstanceId,
        /// The instance the session is now on.
        entered: ScreenInstanceId,
    },
    /// `disposed` was torn down without stacking and `entered` became current.
    Replaced {
        /// The instance to tear down.
        disposed: ScreenInstanceId,
        /// The instance the session is now on.
        entered: ScreenInstanceId,
    },
    /// `disposed` was torn down and `restored` resumed from the stack.
    Restored {
        /// The instance to tear down.
        disposed: ScreenInstanceId,
        /// The instance the session has returned to.
        restored: ScreenInstanceId,
    },
    /// There was nothing to do; nothing changed.
    Unchanged,
    /// The request was refused; nothing changed and the refusal must be shown.
    Refused(NavRefusal),
}

/// One committed navigation step.
#[derive(Debug, Clone)]
pub struct NavTransition {
    /// The state after the step, which equals the state before it on a refusal.
    pub state: NavState,
    /// What the step did, or why it did nothing.
    pub outcome: NavOutcome,
}

/// Apply one navigation intent.
///
/// Returns the state unchanged, paired with a refusal, whenever the request
/// cannot be satisfied — there is no partially applied navigation.
#[must_use]
pub fn reduce_navigation(
    state: NavState,
    registry: &ScreenRegistry,
    intent: NavIntent,
) -> NavTransition {
    match intent {
        NavIntent::Push(activation) => push(state, registry, &activation),
        NavIntent::Replace(activation) => replace(state, registry, &activation),
        NavIntent::Back => back(state),
    }
}

fn push(mut state: NavState, registry: &ScreenRegistry, activation: &Activation) -> NavTransition {
    if state.stack.len() >= MAX_NAVIGATION_STACK {
        return refused(
            state,
            NavRefusal::StackDepth {
                limit: MAX_NAVIGATION_STACK,
            },
        );
    }
    let entered = match state.construct(registry, activation) {
        Ok(instance) => instance,
        Err(refusal) => return refused(state, refusal),
    };
    let outcome = NavOutcome::Pushed {
        suspended: state.current.id,
        entered: entered.id,
    };
    let suspended = std::mem::replace(&mut state.current, entered);
    state.stack.push(SuspendedInstance(suspended));
    state.advance();
    NavTransition { state, outcome }
}

fn replace(
    mut state: NavState,
    registry: &ScreenRegistry,
    activation: &Activation,
) -> NavTransition {
    let entered = match state.construct(registry, activation) {
        Ok(instance) => instance,
        Err(refusal) => return refused(state, refusal),
    };
    let outcome = NavOutcome::Replaced {
        disposed: state.current.id,
        entered: entered.id,
    };
    state.current = entered;
    state.advance();
    NavTransition { state, outcome }
}

fn back(mut state: NavState) -> NavTransition {
    let Some(SuspendedInstance(restored)) = state.stack.pop() else {
        return NavTransition {
            state,
            outcome: NavOutcome::Unchanged,
        };
    };
    let outcome = NavOutcome::Restored {
        disposed: state.current.id,
        restored: restored.id,
    };
    state.current = restored;
    NavTransition { state, outcome }
}

fn refused(state: NavState, refusal: NavRefusal) -> NavTransition {
    NavTransition {
        state,
        outcome: NavOutcome::Refused(refusal),
    }
}

/// Why a navigation request was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavRefusal {
    /// The activation does not satisfy its route's declared schema.
    Activation(ActivationError),
    /// The request was computed from an instance that is no longer current.
    StaleSource {
        /// The instance the request named.
        supplied: ScreenInstanceId,
        /// The instance that is actually current.
        live: ScreenInstanceId,
    },
    /// The route reaches a screen the session cannot be routed to.
    NotRoutable {
        /// The route whose target has no renderer.
        route: RouteId,
    },
    /// The screen has no compiled descriptor.
    MissingDescriptor {
        /// The screen with no descriptor.
        screen: ScreenId,
    },
    /// The stack already holds its maximum number of suspended instances.
    StackDepth {
        /// The maximum the stack holds.
        limit: usize,
    },
    /// The session ran out of distinct generations.
    GenerationExhausted,
}

impl NavRefusal {
    /// The coded diagnostic this refusal reports.
    #[must_use]
    pub const fn code(&self) -> NavCode {
        NavCode::E001
    }
}

impl From<ActivationError> for NavRefusal {
    fn from(error: ActivationError) -> Self {
        Self::Activation(error)
    }
}

impl fmt::Display for NavRefusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // The activation error already renders its own code.
            Self::Activation(error) => write!(formatter, "{error}"),
            Self::StaleSource { .. } => write!(
                formatter,
                "{}: the screen this request came from is no longer open",
                self.code()
            ),
            Self::NotRoutable { route } => write!(
                formatter,
                "{}: route '{route}' does not reach a screen this session can open",
                self.code()
            ),
            Self::MissingDescriptor { screen } => write!(
                formatter,
                "{}: screen '{screen}' has no descriptor",
                self.code()
            ),
            Self::StackDepth { limit } => write!(
                formatter,
                "{}: {limit} screens are already open behind this one",
                self.code()
            ),
            Self::GenerationExhausted => write!(
                formatter,
                "{}: this session cannot open another screen",
                self.code()
            ),
        }
    }
}

impl std::error::Error for NavRefusal {}
