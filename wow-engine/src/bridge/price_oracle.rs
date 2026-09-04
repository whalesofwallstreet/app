//! Single source of truth for asset-to-USD pricing.
//!
//! Before this module, `router::get_usd_value`, `DexProvider::get_swap_quote`,
//! and `GasOracle`'s fee calculations each hardcoded their own private price
//! table. Any real market move meant routing decisions, DEX quotes, and
//! gas-fee-in-USD estimates all disagreed with each other and with reality,
//! since none of them shared a source or update mechanism.
//!
//! [`PriceOracle`] centralizes this behind one live feed (CoinGecko),
//! reusing the `moka` cache-with-TTL + static-fallback pattern already
//! proven in [`super::gas_oracle::GasOracle`].

use crate::config::AppConfig;
use moka::future::Cache;
use reqwest_middleware::ClientWithMiddleware;
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tracing::warn;

const DEFAULT_COINGECKO_BASE_URL: &str = "https://api.coingecko.com";

/// Bounds how long a single price-lookup attempt may run, mirroring
/// `GasOracle`'s `PROVIDER_REQUEST_TIMEOUT`.
const PROVIDER_REQUEST_TIMEOUT: Duration = Duration::from_secs(3);

/// Maps an asset symbol to CoinGecko's `simple/price` coin id. Symbols with
/// no entry here (and any symbol when the live feed is unreachable) resolve
/// through [`PriceOracle::fallback_price_usd`] instead.
fn coingecko_id(asset: &str) -> Option<&'static str> {
    match asset {
        "ETH" => Some("ethereum"),
        "SOL" => Some("solana"),
        "XLM" => Some("stellar"),
        "USDC" => Some("usd-coin"),
        _ => None,
    }
}

pub struct PriceOracle {
    cache: Cache<String, f64>,
    client: ClientWithMiddleware,
    base_url: String,
    api_key: Option<String>,
    /// When true, [`Self::fetch_from_api`] never makes a network call and
    /// resolves straight to the fallback table. Set by [`Self::new_for_tests`]
    /// unconditionally, and by [`Self::new`] when `MOCK_GAS_ORACLE` is set —
    /// never by [`Self::with_base_url`], whose whole purpose is exercising
    /// the real fetch path against an explicit (typically wiremock) URL.
    skip_network: bool,
    /// Count of lookups that fell through to static fallback pricing.
    /// Sustained growth here means live price data has stopped flowing.
    fallback_count: AtomicU64,
}

impl PriceOracle {
    /// Production constructor. Respects `MOCK_GAS_ORACLE` (set by default
    /// for every `cargo`-driven process — see `.cargo/config.toml` — so
    /// `cargo test`/`cargo bench` never depend on live network) so this can
    /// be used directly wherever a test just needs "don't make a real HTTP
    /// call", without reaching for [`Self::new_for_tests`].
    pub fn new(config: Arc<AppConfig>) -> Self {
        let skip_network = std::env::var("MOCK_GAS_ORACLE").is_ok();
        Self::build(
            config.coingecko_api_key.clone(),
            DEFAULT_COINGECKO_BASE_URL.to_string(),
            skip_network,
        )
    }

    /// Test-only: points at an explicit (typically wiremock) base URL and
    /// always attempts a real fetch against it, deliberately ignoring
    /// `MOCK_GAS_ORACLE` — the whole point of these tests is exercising the
    /// fetch/parse/retry/fallback logic itself, so short-circuiting on the
    /// same env var `cargo test` sets by default would defeat them.
    #[cfg(test)]
    fn with_base_url(api_key: Option<String>, base_url: String) -> Self {
        Self::build(api_key, base_url, false)
    }

    /// Test-only constructor that never touches the network: every lookup
    /// resolves immediately to [`Self::fallback_price_usd`], matching
    /// today's hardcoded price table exactly. Use this (not `new`) in any
    /// test that doesn't specifically exercise the live-feed/cache/fallback
    /// behavior itself, so tests stay deterministic and network-free.
    pub fn new_for_tests() -> Self {
        Self::build(None, DEFAULT_COINGECKO_BASE_URL.to_string(), true)
    }

    fn build(api_key: Option<String>, base_url: String, skip_network: bool) -> Self {
        let cache = Cache::builder()
            .time_to_live(Duration::from_secs(60))
            .build();
        let client = crate::http_client::build_resilient_client()
            .expect("Failed to build resilient HTTP client for PriceOracle");

        Self {
            cache,
            client,
            base_url,
            api_key,
            skip_network,
            fallback_count: AtomicU64::new(0),
        }
    }

    /// Returns `asset`'s current USD price. Never fails: a cache miss that
    /// can't reach the live feed (or an asset the feed doesn't cover) falls
    /// through to a static default instead of propagating an error, so a
    /// pricing hiccup degrades routing accuracy rather than availability.
    ///
    /// Route pathfinding calls this many times per request (once per asset
    /// per candidate edge explored), so unlike `GasOracle` — called once or
    /// twice per bridge quote — the overwhelmingly common case here is a
    /// cache *hit*. A plain lock-free [`Cache::get`] serves that case
    /// without paying to construct the fetch-coordination future
    /// `try_get_with` needs for the miss path; that slower, stampede-safe
    /// path only runs on an actual miss.
    pub async fn get_price_usd(&self, asset: &str) -> f64 {
        let asset = asset.to_uppercase();

        if let Some(price) = self.cache.get(&asset).await {
            return price;
        }

        let price = self
            .cache
            .try_get_with(asset.clone(), async { self.fetch_from_api(&asset).await })
            .await;

        match price {
            Ok(val) => val,
            Err(err) => {
                self.fallback_count.fetch_add(1, Ordering::Relaxed);
                warn!(
                    asset = %asset,
                    error = %err,
                    "Price oracle degraded to static fallback pricing"
                );
                Self::fallback_price_usd(&asset)
            }
        }
    }

    /// Number of lookups that have fallen through to static fallback
    /// pricing since this oracle was created.
    pub fn fallback_count(&self) -> u64 {
        self.fallback_count.load(Ordering::Relaxed)
    }

    /// Evicts the cached price for a single asset.
    pub async fn invalidate(&self, asset: &str) {
        self.cache.invalidate(&asset.to_uppercase()).await;
    }

    /// Evicts every cached price, regardless of asset.
    pub async fn invalidate_all(&self) {
        self.cache.invalidate_all();
    }

    async fn fetch_from_api(&self, asset: &str) -> Result<f64, Arc<anyhow::Error>> {
        self.fetch_from_api_inner(asset).await.map_err(Arc::new)
    }

    async fn fetch_from_api_inner(&self, asset: &str) -> Result<f64, anyhow::Error> {
        if self.skip_network {
            return Ok(Self::fallback_price_usd(asset));
        }

        let Some(coin_id) = coingecko_id(asset) else {
            // Nothing to look up a live price for (e.g. an unmapped
            // symbol) — the static default is the only sensible answer,
            // not a fetch failure.
            return Ok(Self::fallback_price_usd(asset));
        };

        let url = format!(
            "{}/api/v3/simple/price?ids={coin_id}&vs_currencies=usd",
            self.base_url
        );
        let mut request = self.client.get(&url).timeout(PROVIDER_REQUEST_TIMEOUT);
        if let Some(key) = &self.api_key {
            request = request.header("x-cg-demo-api-key", key);
        }

        let resp: Value = request.send().await?.error_for_status()?.json().await?;

        resp.get(coin_id)
            .and_then(|entry| entry.get("usd"))
            .and_then(|price| price.as_f64())
            .ok_or_else(|| anyhow::anyhow!("Invalid CoinGecko response for {coin_id}"))
    }

    /// Static last-resort pricing — the same table every call site
    /// hardcoded independently before this oracle existed.
    fn fallback_price_usd(asset: &str) -> f64 {
        match asset {
            "ETH" => 3000.0,
            "SOL" => 150.0,
            "XLM" => 0.10,
            "USDC" => 1.0,
            _ => 1.0,
        }
    }
}

impl Default for PriceOracle {
    fn default() -> Self {
        Self::new(Arc::new(AppConfig::default()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn oracle_for_test(api_key: Option<String>, base_url: String) -> PriceOracle {
        PriceOracle::with_base_url(api_key, base_url)
    }

    #[tokio::test]
    async fn fetches_and_caches_the_live_price() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v3/simple/price"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ethereum": { "usd": 3123.45 }
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        let oracle = oracle_for_test(None, mock_server.uri());
        let price = oracle.get_price_usd("ETH").await;
        assert!((price - 3123.45).abs() < f64::EPSILON);
        assert_eq!(oracle.fallback_count(), 0);

        // Second lookup within the TTL must be served from cache, not a
        // second request (wiremock's `.expect(1)` above enforces this).
        let cached = oracle.get_price_usd("eth").await;
        assert!((cached - 3123.45).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn request_includes_the_configured_api_key() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v3/simple/price"))
            .and(wiremock::matchers::header("x-cg-demo-api-key", "test-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ethereum": { "usd": 3000.0 }
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        let oracle = oracle_for_test(Some("test-key".to_string()), mock_server.uri());
        oracle.get_price_usd("ETH").await;
    }

    #[tokio::test]
    async fn provider_outage_falls_back_to_the_static_table() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v3/simple/price"))
            .respond_with(ResponseTemplate::new(503))
            .expect(4) // the resilient client retries 3x on a persistent 5xx
            .mount(&mock_server)
            .await;

        let oracle = oracle_for_test(None, mock_server.uri());
        let price = oracle.get_price_usd("ETH").await;
        assert_eq!(price, 3000.0);
        assert_eq!(oracle.fallback_count(), 1);
    }

    #[tokio::test]
    async fn unmapped_asset_falls_back_without_a_network_call() {
        let mock_server = MockServer::start().await;
        // No mock registered at all — any request would fail the test via
        // wiremock's default "no matching mock" 500 response, proving this
        // path never calls out.
        let oracle = oracle_for_test(None, mock_server.uri());
        let price = oracle.get_price_usd("DOGE").await;
        assert_eq!(price, 1.0);
        assert_eq!(oracle.fallback_count(), 0);
    }

    #[tokio::test]
    async fn new_for_tests_never_touches_the_network() {
        let oracle = PriceOracle::new_for_tests();
        assert_eq!(oracle.get_price_usd("ETH").await, 3000.0);
        assert_eq!(oracle.get_price_usd("SOL").await, 150.0);
        assert_eq!(oracle.get_price_usd("XLM").await, 0.10);
        assert_eq!(oracle.get_price_usd("USDC").await, 1.0);
        assert_eq!(oracle.fallback_count(), 0);
    }

    #[tokio::test]
    async fn invalidate_evicts_only_the_targeted_asset() {
        let oracle = PriceOracle::new_for_tests();
        oracle.cache.insert("ETH".to_string(), 42.0).await;
        oracle.cache.insert("SOL".to_string(), 1.0).await;

        oracle.invalidate("eth").await;

        assert_eq!(oracle.cache.get("ETH").await, None);
        assert_eq!(oracle.cache.get("SOL").await, Some(1.0));
    }

    #[tokio::test]
    async fn invalidate_all_evicts_every_asset() {
        let oracle = PriceOracle::new_for_tests();
        oracle.cache.insert("ETH".to_string(), 42.0).await;
        oracle.cache.insert("SOL".to_string(), 1.0).await;

        oracle.invalidate_all().await;

        assert_eq!(oracle.cache.get("ETH").await, None);
        assert_eq!(oracle.cache.get("SOL").await, None);
    }
}
