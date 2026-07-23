//! Deterministic chaos-engineering suite (issue #18).
//!
//! These tests purposefully degrade internal dependencies — 10-second network
//! hangs, tripped circuit breakers, an exhausted Postgres connection pool — and
//! assert the engine *fails fast and recovers* instead of hanging, deadlocking,
//! or panicking.
//!
//! The whole suite is deterministic and requires **no external services**. Every
//! delay is a Tokio timer, and every test runs under a paused clock
//! (`#[tokio::test(start_paused = true)]`). When the runtime goes idle it
//! auto-advances virtual time to the next timer, so a simulated 10-second outage
//! resolves instantly in wall-clock terms. That is what makes the suite fast and
//! non-flaky in CI: there is no real sleeping and therefore no timing race.
//!
//! Coverage maps directly to the issue's acceptance criteria:
//!   * timeouts fire exactly on schedule, without drift or blocking the executor
//!   * circuit breakers trip after repeated failures and fail fast while open
//!   * the system recovers cleanly once a simulated partition ends (half-open)
//!   * connection-pool exhaustion yields `503`, never an indefinite hang
//!   * no deadlocks or panics under concurrent, overlapping failures

use std::sync::Arc;
use std::time::Duration;

use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    routing::get,
    Router,
};
use tokio::sync::Semaphore;
use tokio::time::Instant;
use tower::ServiceExt; // for `oneshot`

use wow_engine::error::AppError;
use wow_engine::resilience::{CircuitBreaker, CircuitBreakerConfig, CircuitError, CircuitState};

/// A dependency that "hangs" for `delay` before it would have succeeded. Used to
/// simulate a downstream (bridge API, RPC node) that has gone unresponsive.
async fn hanging_dependency(delay: Duration) -> Result<&'static str, &'static str> {
    tokio::time::sleep(delay).await;
    Ok("recovered")
}

// ---------------------------------------------------------------------------
// Timeouts fire exactly on time, without drift or blocking the executor.
// ---------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn timeout_fires_exactly_at_configured_deadline() {
    let cb = CircuitBreaker::new(CircuitBreakerConfig {
        failure_threshold: 3,
        open_cooldown: Duration::from_secs(30),
        call_timeout: Duration::from_secs(5),
    });

    // The dependency would take 10s; our call timeout is 5s.
    let started = Instant::now();
    let result: Result<&str, CircuitError<&str>> =
        cb.call(hanging_dependency(Duration::from_secs(10))).await;
    let elapsed = started.elapsed();

    assert!(
        matches!(result, Err(CircuitError::Timeout)),
        "a 10s hang under a 5s timeout must surface as a timeout"
    );
    // No drift: the timeout fires at *exactly* the configured deadline, not the
    // dependency's 10s and not some jittered value in between.
    assert_eq!(
        elapsed,
        Duration::from_secs(5),
        "timeout must fire precisely at the 5s deadline"
    );
}

#[tokio::test(start_paused = true)]
async fn slow_dependency_does_not_block_the_executor() {
    // A 10s hang must not stall unrelated work. We race the hang against a 1s
    // timer; on a healthy cooperative runtime the 1s timer wins. If the hang
    // blocked the executor thread, this test would deadlock (and CI would catch
    // it via the harness timeout) rather than completing instantly.
    let quick = tokio::time::sleep(Duration::from_secs(1));
    tokio::select! {
        _ = hanging_dependency(Duration::from_secs(10)) => {
            panic!("the 10s hang must not resolve before the 1s timer");
        }
        _ = quick => { /* expected: the short timer fires first */ }
    }
}

// ---------------------------------------------------------------------------
// Circuit breaker trips after repeated failures and fails fast while open.
// ---------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn breaker_trips_after_repeated_timeouts() {
    let cb = CircuitBreaker::new(CircuitBreakerConfig {
        failure_threshold: 3,
        open_cooldown: Duration::from_secs(30),
        call_timeout: Duration::from_secs(5),
    });

    // Three consecutive 10s hangs, each timing out, trip the breaker open.
    for _ in 0..3 {
        let r: Result<&str, CircuitError<&str>> =
            cb.call(hanging_dependency(Duration::from_secs(10))).await;
        assert!(matches!(r, Err(CircuitError::Timeout)));
    }
    assert_eq!(cb.state(), CircuitState::Open, "breaker should be open");
}

#[tokio::test(start_paused = true)]
async fn open_breaker_fails_fast_without_waiting() {
    let cb = CircuitBreaker::new(CircuitBreakerConfig {
        failure_threshold: 2,
        open_cooldown: Duration::from_secs(30),
        call_timeout: Duration::from_secs(5),
    });

    for _ in 0..2 {
        let _: Result<&str, CircuitError<&str>> =
            cb.call(hanging_dependency(Duration::from_secs(10))).await;
    }
    assert_eq!(cb.state(), CircuitState::Open);

    // Once open, a call must be rejected *instantly* — it must not run the
    // (still-hanging) dependency, and it must not consume the timeout budget.
    let started = Instant::now();
    let r: Result<&str, CircuitError<&str>> =
        cb.call(hanging_dependency(Duration::from_secs(10))).await;
    assert!(matches!(r, Err(CircuitError::Open)));
    assert_eq!(
        started.elapsed(),
        Duration::ZERO,
        "an open breaker must reject with zero delay"
    );
}

// ---------------------------------------------------------------------------
// Clean recovery from a simulated network partition (half-open probe).
// ---------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn recovers_cleanly_after_partition_ends() {
    let cb = CircuitBreaker::new(CircuitBreakerConfig {
        failure_threshold: 3,
        open_cooldown: Duration::from_secs(30),
        call_timeout: Duration::from_secs(5),
    });

    // Partition begins: every call hangs and trips the breaker.
    for _ in 0..3 {
        let _: Result<&str, CircuitError<&str>> =
            cb.call(hanging_dependency(Duration::from_secs(10))).await;
    }
    assert_eq!(cb.state(), CircuitState::Open);

    // Still open: calls are rejected during the cooldown window.
    let r: Result<&str, CircuitError<&str>> = cb.call(async { Ok::<_, &str>("nope") }).await;
    assert!(matches!(r, Err(CircuitError::Open)));

    // Partition heals. After the cooldown the breaker allows one half-open probe.
    tokio::time::advance(Duration::from_secs(31)).await;
    assert_eq!(cb.state(), CircuitState::HalfOpen);

    // The probe succeeds (dependency is fast again) and the breaker fully closes,
    // with no manual intervention.
    let r: Result<&str, CircuitError<&str>> =
        cb.call(hanging_dependency(Duration::from_millis(10))).await;
    assert!(matches!(r, Ok("recovered")));
    assert_eq!(cb.state(), CircuitState::Closed);

    // And normal traffic flows again.
    let r: Result<&str, CircuitError<&str>> = cb.call(async { Ok::<_, &str>("ok") }).await;
    assert!(matches!(r, Ok("ok")));
}

#[tokio::test(start_paused = true)]
async fn half_open_probe_failure_reopens_breaker() {
    let cb = CircuitBreaker::new(CircuitBreakerConfig {
        failure_threshold: 3,
        open_cooldown: Duration::from_secs(30),
        call_timeout: Duration::from_secs(5),
    });

    for _ in 0..3 {
        let _: Result<&str, CircuitError<&str>> =
            cb.call(hanging_dependency(Duration::from_secs(10))).await;
    }
    tokio::time::advance(Duration::from_secs(31)).await;
    assert_eq!(cb.state(), CircuitState::HalfOpen);

    // The single probe also fails: the outage is not over, so the breaker must
    // snap straight back to open on this one failure (not wait for the threshold).
    let r: Result<&str, CircuitError<&str>> =
        cb.call(hanging_dependency(Duration::from_secs(10))).await;
    assert!(matches!(r, Err(CircuitError::Timeout)));
    assert_eq!(cb.state(), CircuitState::Open);
}

// ---------------------------------------------------------------------------
// No deadlocks / panics under concurrent, overlapping failures.
// ---------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn concurrent_hangs_all_time_out_without_deadlock() {
    // A single breaker shared by reference across many in-flight calls. `call`
    // takes `&self`, so concurrent access is safe without extra synchronization.
    let cb = CircuitBreaker::new(CircuitBreakerConfig {
        failure_threshold: 100, // high, so the breaker stays closed for this test
        open_cooldown: Duration::from_secs(30),
        call_timeout: Duration::from_secs(5),
    });

    // Fire eight simultaneous 10s hangs and drive them concurrently with `join!`.
    let started = Instant::now();
    let dep = || cb.call(hanging_dependency(Duration::from_secs(10)));
    let results: [Result<&str, CircuitError<&str>>; 8] = {
        let (a, b, c, d, e, f, g, h) =
            tokio::join!(dep(), dep(), dep(), dep(), dep(), dep(), dep(), dep());
        [a, b, c, d, e, f, g, h]
    };

    // All eight time out, and — because they ran concurrently, not serially —
    // the whole batch completes in a single 5s timeout window, not 8 * 5s.
    assert!(results
        .iter()
        .all(|r| matches!(r, Err(CircuitError::Timeout))));
    assert_eq!(
        started.elapsed(),
        Duration::from_secs(5),
        "concurrent calls must share one timeout window, proving no serialization"
    );
}

// ---------------------------------------------------------------------------
// Axum request timeout: a hung handler yields 408 exactly on the deadline.
// ---------------------------------------------------------------------------

async fn never_responds() -> &'static str {
    // Simulate a handler wedged on an unresponsive dependency forever.
    std::future::pending::<()>().await;
    unreachable!()
}

#[tokio::test(start_paused = true)]
async fn axum_timeout_layer_returns_408_for_hung_handler() {
    use tower_http::timeout::TimeoutLayer;

    let app = Router::new()
        .route("/hang", get(never_responds))
        .layer(TimeoutLayer::new(Duration::from_secs(3)));

    let started = Instant::now();
    let response = app
        .oneshot(Request::builder().uri("/hang").body(Body::empty()).unwrap())
        .await
        .expect("router service call should not error");

    assert_eq!(
        response.status(),
        StatusCode::REQUEST_TIMEOUT,
        "a hung request must be aborted with 408, not left hanging"
    );
    assert_eq!(
        started.elapsed(),
        Duration::from_secs(3),
        "the timeout must fire precisely at the configured deadline"
    );
}

// ---------------------------------------------------------------------------
// Postgres pool exhaustion: the API returns 503 fast instead of hanging.
// ---------------------------------------------------------------------------

/// A tiny, deterministic stand-in for a Postgres connection pool. A bounded
/// [`Semaphore`] models `max_connections`, and the handler bounds its wait with
/// `tokio::time::timeout` exactly as `sqlx`'s `acquire_timeout` does. This lets
/// us reproduce pool starvation — and prove the `503`/no-hang behaviour — with
/// zero external dependencies and no wall-clock waiting.
#[derive(Clone)]
struct FakePool {
    permits: Arc<Semaphore>,
    acquire_timeout: Duration,
}

async fn pool_guarded_handler(State(pool): State<FakePool>) -> Result<&'static str, AppError> {
    match tokio::time::timeout(pool.acquire_timeout, pool.permits.acquire()).await {
        Ok(Ok(_permit)) => Ok("ok"),
        // Acquire timed out: the pool is starved. Fail fast with 503 so the
        // caller retries, rather than blocking the request indefinitely.
        Ok(Err(_)) | Err(_) => Err(AppError::ServiceUnavailable(
            "Database connection pool exhausted".to_string(),
        )),
    }
}

#[tokio::test(start_paused = true)]
async fn pool_exhaustion_returns_503_then_recovers() {
    let permits = Arc::new(Semaphore::new(2));
    let pool = FakePool {
        permits: Arc::clone(&permits),
        acquire_timeout: Duration::from_secs(5),
    };

    // Exhaust the pool by holding both permits for the duration of the request.
    let held: Vec<_> = vec![
        permits.clone().try_acquire_owned().unwrap(),
        permits.clone().try_acquire_owned().unwrap(),
    ];

    let app = Router::new()
        .route("/execute", get(pool_guarded_handler))
        .with_state(pool.clone());

    // With every connection checked out, the request must give up after
    // `acquire_timeout` and return 503 — not hang forever.
    let started = Instant::now();
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/execute")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        started.elapsed(),
        Duration::from_secs(5),
        "the request must fail fast at acquire_timeout, not hang indefinitely"
    );

    // The outage ends: connections are returned to the pool.
    drop(held);

    // Subsequent traffic is served normally, with no manual intervention.
    let response = app
        .oneshot(
            Request::builder()
                .uri("/execute")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}
