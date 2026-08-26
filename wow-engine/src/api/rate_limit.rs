//! Per-IP request throttling.
//!
//! The `/api/v1/quote` handler alone runs a bounded but non-trivial
//! priority-queue search (`RoutePlanner::find_best_route`) across every
//! chain/asset combination per request. With no throttling anywhere in
//! `create_router`, it — and every other route — is open to trivial
//! volumetric abuse from any origin. [`RateLimiter`] enforces a simple
//! fixed-window per-key budget; [`rate_limit_middleware`] wires it up as
//! Axum middleware.

use crate::error::AppError;
use axum::{
    extract::{ConnectInfo, Request, State},
    http::header,
    middleware::Next,
    response::Response,
};
use moka::future::Cache;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Fixed-window per-key request counter.
///
/// Each key's counter lives for exactly `window` from the moment it's first
/// created (moka's per-entry TTL); once the entry is evicted the window
/// resets. This mirrors [`crate::bridge::gas_oracle::GasOracle`]'s
/// TTL-cache pattern rather than pulling in a dedicated rate-limiting crate.
#[derive(Clone)]
pub struct RateLimiter {
    counters: Cache<String, Arc<AtomicU32>>,
    limit_per_window: u32,
    window: Duration,
}

impl RateLimiter {
    pub fn new(limit_per_window: u32, window: Duration) -> Self {
        Self {
            counters: Cache::builder().time_to_live(window).build(),
            limit_per_window,
            window,
        }
    }

    /// Returns `Ok(())` if `key` is still within budget for its current
    /// window, or `Err(retry_after)` once the budget is exhausted.
    async fn check(&self, key: &str) -> Result<(), Duration> {
        let counter = self
            .counters
            .get_with(key.to_string(), async { Arc::new(AtomicU32::new(0)) })
            .await;

        let count = counter.fetch_add(1, Ordering::SeqCst) + 1;
        if count > self.limit_per_window {
            Err(self.window)
        } else {
            Ok(())
        }
    }
}

/// Best-effort client identity for throttling: the real peer IP when
/// available (wired up via `into_make_service_with_connect_info` in
/// `main`), otherwise the first hop of `X-Forwarded-For` (useful behind a
/// trusted proxy, and for exercising this middleware in tests without a
/// real socket), otherwise a shared fallback bucket.
fn client_key(req: &Request) -> String {
    if let Some(ConnectInfo(addr)) = req.extensions().get::<ConnectInfo<SocketAddr>>() {
        return addr.ip().to_string();
    }

    req.headers()
        .get(header::HeaderName::from_static("x-forwarded-for"))
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

pub async fn rate_limit_middleware(
    State(limiter): State<RateLimiter>,
    req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let key = client_key(&req);
    match limiter.check(&key).await {
        Ok(()) => Ok(next.run(req).await),
        Err(retry_after) => Err(AppError::TooManyRequests(retry_after.as_secs().max(1))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn allows_up_to_the_limit_then_rejects() {
        let limiter = RateLimiter::new(3, Duration::from_secs(60));
        assert!(limiter.check("1.2.3.4").await.is_ok());
        assert!(limiter.check("1.2.3.4").await.is_ok());
        assert!(limiter.check("1.2.3.4").await.is_ok());
        assert!(limiter.check("1.2.3.4").await.is_err());
    }

    #[tokio::test]
    async fn tracks_separate_budgets_per_key() {
        let limiter = RateLimiter::new(1, Duration::from_secs(60));
        assert!(limiter.check("1.2.3.4").await.is_ok());
        assert!(limiter.check("5.6.7.8").await.is_ok());
        assert!(limiter.check("1.2.3.4").await.is_err());
    }
}
