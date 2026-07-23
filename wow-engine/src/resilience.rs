//! Resilience primitives for surviving cascading dependency failures.
//!
//! The routing engine fans out to several unreliable external systems on every
//! request (bridge quote APIs, gas oracles, the Postgres pool). When one of
//! those dependencies hangs or starts erroring, we must fail *fast and locally*
//! instead of letting the slow call pile up and exhaust our own executor and
//! connection resources.
//!
//! This module provides two composable building blocks:
//!
//! * [`CircuitBreaker`] — trips open after a configurable number of consecutive
//!   failures and short-circuits subsequent calls until a cooldown elapses, at
//!   which point it lets a single probe through (half-open) to test recovery.
//! * [`CircuitBreaker::call`] — wraps a future with both a hard timeout and the
//!   breaker's accounting, so a hanging dependency counts as a failure rather
//!   than blocking indefinitely.
//!
//! Everything here is built on [`tokio::time`], which means the behaviour is
//! fully deterministic under `tokio::time::pause`/`advance`. The chaos test
//! suite (`tests/chaos_tests.rs`) exploits that to simulate multi-second
//! outages instantly and assert timeouts fire exactly on schedule.

use std::fmt;
use std::sync::Mutex;
use std::time::Duration;
use tokio::time::Instant;

/// Runtime state of a [`CircuitBreaker`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Healthy. Calls pass through and failures are counted.
    Closed,
    /// Tripped. Calls fail fast until the cooldown elapses.
    Open,
    /// Cooldown elapsed. A single probe call is allowed through to test whether
    /// the dependency has recovered.
    HalfOpen,
}

impl fmt::Display for CircuitState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            CircuitState::Closed => "closed",
            CircuitState::Open => "open",
            CircuitState::HalfOpen => "half-open",
        };
        f.write_str(s)
    }
}

/// Error returned by [`CircuitBreaker::call`].
#[derive(Debug)]
pub enum CircuitError<E> {
    /// The breaker is open and rejected the call without invoking the future.
    Open,
    /// The wrapped future exceeded the configured call timeout.
    Timeout,
    /// The wrapped future completed but returned an error.
    Inner(E),
}

impl<E: fmt::Display> fmt::Display for CircuitError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CircuitError::Open => write!(f, "circuit breaker is open; call rejected"),
            CircuitError::Timeout => write!(f, "call timed out"),
            CircuitError::Inner(e) => write!(f, "{e}"),
        }
    }
}

impl<E: std::error::Error + 'static> std::error::Error for CircuitError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CircuitError::Inner(e) => Some(e),
            _ => None,
        }
    }
}

/// Configuration for a [`CircuitBreaker`].
#[derive(Debug, Clone, Copy)]
pub struct CircuitBreakerConfig {
    /// Number of consecutive failures (errors or timeouts) that trips the
    /// breaker from closed to open.
    pub failure_threshold: u32,
    /// How long the breaker stays open before allowing a half-open probe.
    pub open_cooldown: Duration,
    /// Maximum time a single call may run before it is treated as a failure.
    pub call_timeout: Duration,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            open_cooldown: Duration::from_secs(30),
            call_timeout: Duration::from_secs(10),
        }
    }
}

/// Internal, lock-protected accounting for the breaker.
#[derive(Debug)]
struct Inner {
    state: CircuitState,
    consecutive_failures: u32,
    /// When the breaker last transitioned to `Open`. Only meaningful while
    /// `state == Open`.
    opened_at: Option<Instant>,
}

/// A thread-safe circuit breaker built on Tokio timers.
///
/// Clone is intentionally *not* derived: share a single breaker across tasks by
/// wrapping it in an [`std::sync::Arc`]. The internal state is guarded by a
/// [`Mutex`]; the lock is never held across an `.await`, so it is safe to use
/// from any async context without risking a deadlock.
#[derive(Debug)]
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    inner: Mutex<Inner>,
}

impl CircuitBreaker {
    /// Creates a breaker with the given configuration, starting closed.
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            config,
            inner: Mutex::new(Inner {
                state: CircuitState::Closed,
                consecutive_failures: 0,
                opened_at: None,
            }),
        }
    }

    /// Returns the breaker's current state, resolving any elapsed cooldown.
    ///
    /// If the breaker is open and the cooldown has elapsed, this reports
    /// [`CircuitState::HalfOpen`] (and records the transition) so callers see a
    /// consistent view.
    pub fn state(&self) -> CircuitState {
        let mut inner = self.inner.lock().expect("circuit breaker mutex poisoned");
        self.refresh_state(&mut inner);
        inner.state
    }

    /// Convenience predicate: is the breaker currently rejecting calls?
    pub fn is_open(&self) -> bool {
        self.state() == CircuitState::Open
    }

    /// Promote an open breaker to half-open once its cooldown has elapsed.
    fn refresh_state(&self, inner: &mut Inner) {
        if inner.state == CircuitState::Open {
            if let Some(opened_at) = inner.opened_at {
                if opened_at.elapsed() >= self.config.open_cooldown {
                    inner.state = CircuitState::HalfOpen;
                }
            }
        }
    }

    /// Records a successful call, closing the breaker and clearing failures.
    fn record_success(&self) {
        let mut inner = self.inner.lock().expect("circuit breaker mutex poisoned");
        inner.state = CircuitState::Closed;
        inner.consecutive_failures = 0;
        inner.opened_at = None;
    }

    /// Records a failed call. Trips the breaker open if the failure threshold is
    /// reached, or immediately if we were probing in the half-open state.
    fn record_failure(&self) {
        let mut inner = self.inner.lock().expect("circuit breaker mutex poisoned");
        inner.consecutive_failures = inner.consecutive_failures.saturating_add(1);

        let should_open = inner.state == CircuitState::HalfOpen
            || inner.consecutive_failures >= self.config.failure_threshold;

        if should_open {
            inner.state = CircuitState::Open;
            inner.opened_at = Some(Instant::now());
        }
    }

    /// Executes `fut` under the breaker's protection.
    ///
    /// * If the breaker is open (and still cooling down), the future is never
    ///   polled and [`CircuitError::Open`] is returned immediately.
    /// * Otherwise the future runs with a [`tokio::time::timeout`]. A timeout or
    ///   an `Err` result is recorded as a failure; an `Ok` result closes the
    ///   breaker.
    ///
    /// Because the timeout uses Tokio's timer, a paused/advanced test clock
    /// drives it deterministically with no wall-clock waiting.
    pub async fn call<F, T, E>(&self, fut: F) -> Result<T, CircuitError<E>>
    where
        F: std::future::Future<Output = Result<T, E>>,
    {
        // Admission control. We take the lock, decide, and drop it *before*
        // awaiting so we never hold a std Mutex across an await point.
        {
            let mut inner = self.inner.lock().expect("circuit breaker mutex poisoned");
            self.refresh_state(&mut inner);
            if inner.state == CircuitState::Open {
                return Err(CircuitError::Open);
            }
        }

        match tokio::time::timeout(self.config.call_timeout, fut).await {
            Err(_elapsed) => {
                self.record_failure();
                Err(CircuitError::Timeout)
            }
            Ok(Ok(value)) => {
                self.record_success();
                Ok(value)
            }
            Ok(Err(err)) => {
                self.record_failure();
                Err(CircuitError::Inner(err))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fast_config() -> CircuitBreakerConfig {
        CircuitBreakerConfig {
            failure_threshold: 3,
            open_cooldown: Duration::from_secs(30),
            call_timeout: Duration::from_secs(10),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn success_keeps_breaker_closed() {
        let cb = CircuitBreaker::new(fast_config());
        let out: Result<u32, CircuitError<()>> = cb.call(async { Ok(7) }).await;
        assert!(matches!(out, Ok(7)));
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[tokio::test(start_paused = true)]
    async fn trips_open_after_threshold_failures() {
        let cb = CircuitBreaker::new(fast_config());
        for _ in 0..3 {
            let out: Result<(), CircuitError<&str>> = cb.call(async { Err("boom") }).await;
            assert!(matches!(out, Err(CircuitError::Inner("boom"))));
        }
        assert_eq!(cb.state(), CircuitState::Open);

        // While open, calls are rejected without ever polling the future.
        let mut polled = false;
        let out: Result<(), CircuitError<&str>> = cb
            .call(async {
                polled = true;
                Err("should not run")
            })
            .await;
        assert!(matches!(out, Err(CircuitError::Open)));
        assert!(!polled, "open breaker must not poll the guarded future");
    }

    #[tokio::test(start_paused = true)]
    async fn half_open_recovers_on_success() {
        let cb = CircuitBreaker::new(fast_config());
        for _ in 0..3 {
            let _: Result<(), CircuitError<&str>> = cb.call(async { Err("boom") }).await;
        }
        assert_eq!(cb.state(), CircuitState::Open);

        // Advance past the cooldown: the breaker should offer a half-open probe.
        tokio::time::advance(Duration::from_secs(31)).await;
        assert_eq!(cb.state(), CircuitState::HalfOpen);

        let out: Result<u32, CircuitError<()>> = cb.call(async { Ok(1) }).await;
        assert!(matches!(out, Ok(1)));
        assert_eq!(cb.state(), CircuitState::Closed);
    }
}
