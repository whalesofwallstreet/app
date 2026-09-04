use crate::bridge::price_oracle::PriceOracle;
use crate::bridge::Chain;
use crate::config::AppConfig;
use moka::future::Cache;
use reqwest_middleware::ClientWithMiddleware;
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

const DEFAULT_ETHERSCAN_BASE_URL: &str = "https://api.etherscan.io";
const DEFAULT_ARBISCAN_BASE_URL: &str = "https://api.arbiscan.io";

/// Bounds how long a single gas-tracker request attempt may run. Applied
/// per-attempt on top of the shared resilient client's retry/backoff, so a
/// slow/hanging provider can't blow through the overall request budget.
const PROVIDER_REQUEST_TIMEOUT: Duration = Duration::from_secs(3);

/// Why a gas-fee lookup fell through to [`GasOracle::fallback_fee`], so
/// operators can tell "we never even tried to authenticate" apart from "the
/// provider is actually down" at a glance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FallbackReason {
    /// No API key is configured for this chain's provider, so requests are
    /// sent unauthenticated and are far more likely to be rate-limited.
    MissingApiKey,
    /// A key is configured but the request still failed (network error,
    /// non-2xx after retries, or an unparsable response).
    ProviderOutage,
}

impl FallbackReason {
    fn as_str(self) -> &'static str {
        match self {
            FallbackReason::MissingApiKey => "missing_api_key",
            FallbackReason::ProviderOutage => "provider_outage",
        }
    }
}

pub struct GasOracle {
    config: Arc<AppConfig>,
    cache: Cache<Chain, f64>,
    client: ClientWithMiddleware,
    etherscan_base_url: String,
    arbiscan_base_url: String,
    /// Source of truth for the ETH price used in fee-in-USD conversion —
    /// shared with `RoutePlanner` (via [`Self::with_price_oracle`]) so gas
    /// fees, route pathfinding, and DEX quotes never disagree about what
    /// ETH is worth. Callers that don't need that sharing (most existing
    /// `GasOracle::new` call sites) get their own private instance.
    price_oracle: Arc<PriceOracle>,
    /// When true, [`Self::fetch_from_api`] never makes a network call and
    /// resolves straight to the fallback table. Set whenever `MOCK_GAS_ORACLE`
    /// is present (by [`Self::new`] and [`Self::with_price_oracle`], both
    /// used in production) — never by the `#[cfg(test)]` [`Self::with_base_urls`],
    /// whose whole purpose is exercising the real fetch path against an
    /// explicit wiremock URL regardless of that ambient env var.
    skip_network: bool,
    /// Count of lookups that fell through to static fallback pricing.
    /// Sustained growth here means live gas data has stopped flowing —
    /// surface it on a dashboard/alert rather than relying on scraping logs.
    fallback_count: AtomicU64,
}

impl GasOracle {
    pub fn new(config: Arc<AppConfig>) -> Self {
        let price_oracle = Arc::new(PriceOracle::new(config.clone()));
        Self::with_price_oracle(config, price_oracle)
    }

    /// Builds a `GasOracle` backed by a caller-supplied [`PriceOracle`]
    /// instead of a private one, so the ETH price it uses for fee-in-USD
    /// conversion is literally the same cache `RoutePlanner` reads for
    /// route pathfinding and DEX quotes. Respects `MOCK_GAS_ORACLE` — set
    /// by default for every `cargo`-driven process, see `.cargo/config.toml`.
    pub fn with_price_oracle(config: Arc<AppConfig>, price_oracle: Arc<PriceOracle>) -> Self {
        let skip_network = std::env::var("MOCK_GAS_ORACLE").is_ok();
        Self::build(
            config,
            DEFAULT_ETHERSCAN_BASE_URL.to_string(),
            DEFAULT_ARBISCAN_BASE_URL.to_string(),
            price_oracle,
            skip_network,
        )
    }

    /// Test-only: builds a `GasOracle` pointed at mock Etherscan/Arbiscan
    /// endpoints, backed by a network-free [`PriceOracle::new_for_tests`] so
    /// the ETH-price sub-lookup inside `fetch_etherscan`/`fetch_arbiscan`
    /// never depends on live network access either. Deliberately ignores
    /// `MOCK_GAS_ORACLE` so it always exercises the real fetch/parse logic
    /// against the given (wiremock) URLs.
    #[cfg(test)]
    fn with_base_urls(
        config: Arc<AppConfig>,
        etherscan_base_url: String,
        arbiscan_base_url: String,
    ) -> Self {
        let price_oracle = Arc::new(PriceOracle::new_for_tests());
        Self::build(
            config,
            etherscan_base_url,
            arbiscan_base_url,
            price_oracle,
            false,
        )
    }

    fn build(
        config: Arc<AppConfig>,
        etherscan_base_url: String,
        arbiscan_base_url: String,
        price_oracle: Arc<PriceOracle>,
        skip_network: bool,
    ) -> Self {
        // Cache with 60 seconds TTL
        let cache = Cache::builder()
            .time_to_live(Duration::from_secs(60))
            .build();

        // Shared resilient client: retries transient failures (5xx, network
        // errors) with exponential backoff, same as CctpClient/DeBridgeClient.
        let client = crate::http_client::build_resilient_client()
            .expect("Failed to build resilient HTTP client for GasOracle");

        Self {
            config,
            cache,
            client,
            etherscan_base_url,
            arbiscan_base_url,
            price_oracle,
            skip_network,
            fallback_count: AtomicU64::new(0),
        }
    }

    pub async fn estimate_gas_fee_usd(&self, chain: Chain) -> f64 {
        // Fix #1: Use try_get_with to coalesce concurrent calls and fix Cache Stampede
        let fee = self
            .cache
            .try_get_with(chain, async { self.fetch_from_api(chain).await })
            .await;

        match fee {
            Ok(val) => val,
            Err(e) => {
                let reason = self.fallback_reason(chain);
                self.fallback_count.fetch_add(1, Ordering::Relaxed);
                warn!(
                    chain = ?chain,
                    reason = reason.as_str(),
                    error = %e,
                    "Gas oracle degraded to static fallback pricing"
                );
                Self::fallback_fee(chain)
            }
        }
    }

    /// Number of lookups that have fallen through to static fallback
    /// pricing since this oracle was created. Exposed for dashboards/alerts
    /// so sustained degradation is observable rather than silent.
    pub fn fallback_count(&self) -> u64 {
        self.fallback_count.load(Ordering::Relaxed)
    }

    /// Distinguishes "we never had a key to authenticate with" from "a key
    /// is configured but the provider call still failed" for the chains
    /// that call out to an external gas tracker. Chains with no such
    /// provider (Solana/Stellar) never fail, so this is only meaningful for
    /// the `Err` branch of [`Self::estimate_gas_fee_usd`].
    fn fallback_reason(&self, chain: Chain) -> FallbackReason {
        let key_configured = match chain {
            Chain::Ethereum => self.config.etherscan_api_key.is_some(),
            Chain::Arbitrum => self.config.arbiscan_api_key.is_some(),
            Chain::Solana | Chain::Stellar => true,
        };

        if key_configured {
            FallbackReason::ProviderOutage
        } else {
            FallbackReason::MissingApiKey
        }
    }

    /// Evicts the cached gas fee for a single chain.
    pub async fn invalidate(&self, chain: Chain) {
        self.cache.invalidate(&chain).await;
    }

    /// Evicts every cached gas fee, regardless of chain.
    pub async fn invalidate_all(&self) {
        self.cache.invalidate_all();
    }

    async fn fetch_from_api(&self, chain: Chain) -> Result<f64, std::sync::Arc<anyhow::Error>> {
        if self.skip_network {
            return Ok(Self::fallback_fee(chain));
        }

        info!(
            "Fetching real-time gas fee from external REST API for {:?}",
            chain
        );
        // Fix #3: Actual REST integrations for EVM chains
        let result = match chain {
            Chain::Ethereum => self.fetch_etherscan().await,
            Chain::Arbitrum => self.fetch_arbiscan().await,
            // For Solana/Stellar, there are no simple unauthenticated gas APIs, returning a safe fallback
            _ => Ok(Self::fallback_fee(chain)),
        };

        result.map_err(std::sync::Arc::new)
    }

    async fn fetch_etherscan(&self) -> Result<f64, anyhow::Error> {
        let base = format!(
            "{}/api?module=gastracker&action=gasoracle",
            self.etherscan_base_url
        );
        let url = match &self.config.etherscan_api_key {
            Some(key) => format!("{base}&apikey={key}"),
            None => base,
        };
        let resp: Value = self
            .client
            .get(&url)
            .timeout(PROVIDER_REQUEST_TIMEOUT)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        // Etherscan returns "ProposeGasPrice" in Gwei
        if let Some(price_str) = resp
            .get("result")
            .and_then(|r| r.get("ProposeGasPrice"))
            .and_then(|p| p.as_str())
        {
            let gwei: f64 = price_str.parse()?;
            let eth_price_usd = self.price_oracle.get_price_usd("ETH").await;
            // Assume 150,000 gas limit for a bridge tx.
            // Fee (USD) = gas_limit * gas_price_gwei * 10^-9 * eth_price_usd
            let fee_usd = 150_000.0 * gwei * 1e-9 * eth_price_usd;
            return Ok(fee_usd);
        }
        Err(anyhow::anyhow!("Invalid Etherscan response"))
    }

    async fn fetch_arbiscan(&self) -> Result<f64, anyhow::Error> {
        let base = format!(
            "{}/api?module=gastracker&action=gasoracle",
            self.arbiscan_base_url
        );
        let url = match &self.config.arbiscan_api_key {
            Some(key) => format!("{base}&apikey={key}"),
            None => base,
        };
        let resp: Value = self
            .client
            .get(&url)
            .timeout(PROVIDER_REQUEST_TIMEOUT)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        if let Some(price_str) = resp
            .get("result")
            .and_then(|r| r.get("ProposeGasPrice"))
            .and_then(|p| p.as_str())
        {
            let gwei: f64 = price_str.parse()?;
            let eth_price_usd = self.price_oracle.get_price_usd("ETH").await;
            let fee_usd = 1_000_000.0 * gwei * 1e-9 * eth_price_usd; // L2 gas limit is higher, but gwei is tiny
            return Ok(fee_usd);
        }
        Err(anyhow::anyhow!("Invalid Arbiscan response"))
    }

    fn fallback_fee(chain: Chain) -> f64 {
        match chain {
            Chain::Ethereum => 15.00,
            Chain::Arbitrum => 0.50,
            Chain::Solana => 0.05,
            Chain::Stellar => 0.01,
        }
    }
}

impl Default for GasOracle {
    fn default() -> Self {
        Self::new(Arc::new(AppConfig::default()))
    }
}

/// Test-only direct cache accessors, used by [`crate::cache_sync`]'s tests to
/// seed/observe cache state without going through a real (network-bound) fetch.
#[cfg(test)]
impl GasOracle {
    pub(crate) async fn cache_insert_for_test(&self, chain: Chain, fee: f64) {
        self.cache.insert(chain, fee).await;
    }

    pub(crate) async fn cached_value_for_test(&self, chain: Chain) -> Option<f64> {
        self.cache.get(&chain).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn oracle_for_test(
        config: AppConfig,
        etherscan_base_url: String,
        arbiscan_base_url: String,
    ) -> GasOracle {
        GasOracle::with_base_urls(Arc::new(config), etherscan_base_url, arbiscan_base_url)
    }

    #[tokio::test]
    async fn invalidate_evicts_only_the_targeted_chain() {
        let oracle = GasOracle::new(Arc::new(AppConfig::default()));
        oracle.cache.insert(Chain::Ethereum, 42.0).await;
        oracle.cache.insert(Chain::Solana, 1.0).await;

        oracle.invalidate(Chain::Ethereum).await;

        assert_eq!(oracle.cache.get(&Chain::Ethereum).await, None);
        assert_eq!(oracle.cache.get(&Chain::Solana).await, Some(1.0));
    }

    #[tokio::test]
    async fn invalidate_all_evicts_every_chain() {
        let oracle = GasOracle::new(Arc::new(AppConfig::default()));
        oracle.cache.insert(Chain::Ethereum, 42.0).await;
        oracle.cache.insert(Chain::Solana, 1.0).await;

        oracle.invalidate_all().await;

        assert_eq!(oracle.cache.get(&Chain::Ethereum).await, None);
        assert_eq!(oracle.cache.get(&Chain::Solana).await, None);
    }

    #[tokio::test]
    async fn etherscan_request_includes_the_configured_api_key() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api"))
            .and(wiremock::matchers::query_param(
                "apikey",
                "test-etherscan-key",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "result": { "ProposeGasPrice": "20" }
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        let config = AppConfig {
            etherscan_api_key: Some("test-etherscan-key".to_string()),
            ..AppConfig::default()
        };
        let oracle = oracle_for_test(
            config,
            mock_server.uri(),
            "http://unused.invalid".to_string(),
        );

        let fee = oracle.estimate_gas_fee_usd(Chain::Ethereum).await;
        assert!((fee - (150_000.0 * 20.0 * 1e-9 * 3000.0)).abs() < f64::EPSILON);
        assert_eq!(oracle.fallback_count(), 0);
    }

    #[tokio::test]
    async fn missing_api_key_still_calls_the_provider_unauthenticated() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "result": { "ProposeGasPrice": "10" }
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        // No etherscan_api_key configured.
        let oracle = oracle_for_test(
            AppConfig::default(),
            mock_server.uri(),
            "http://unused.invalid".to_string(),
        );

        let fee = oracle.estimate_gas_fee_usd(Chain::Ethereum).await;
        assert!((fee - (150_000.0 * 10.0 * 1e-9 * 3000.0)).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn provider_outage_retries_via_the_resilient_client_then_falls_back() {
        let mock_server = MockServer::start().await;
        // The shared resilient client retries up to 3 times (4 attempts total)
        // on a persistent 5xx, mirroring http_client.rs's own retry test.
        Mock::given(method("GET"))
            .and(path("/api"))
            .respond_with(ResponseTemplate::new(503))
            .expect(4)
            .mount(&mock_server)
            .await;

        let config = AppConfig {
            etherscan_api_key: Some("configured-key".to_string()),
            ..AppConfig::default()
        };
        let oracle = oracle_for_test(
            config,
            mock_server.uri(),
            "http://unused.invalid".to_string(),
        );

        let fee = oracle.estimate_gas_fee_usd(Chain::Ethereum).await;

        // A key *is* configured, so the fallback is a genuine provider
        // outage, not a missing-key degradation.
        assert_eq!(fee, 15.00);
        assert_eq!(oracle.fallback_count(), 1);
    }

    #[tokio::test]
    async fn fallback_reason_distinguishes_missing_key_from_provider_outage() {
        let with_key = oracle_for_test(
            AppConfig {
                etherscan_api_key: Some("k".to_string()),
                ..AppConfig::default()
            },
            "http://unused.invalid".to_string(),
            "http://unused.invalid".to_string(),
        );
        assert_eq!(
            with_key.fallback_reason(Chain::Ethereum),
            FallbackReason::ProviderOutage
        );

        let without_key = oracle_for_test(
            AppConfig::default(),
            "http://unused.invalid".to_string(),
            "http://unused.invalid".to_string(),
        );
        assert_eq!(
            without_key.fallback_reason(Chain::Ethereum),
            FallbackReason::MissingApiKey
        );
        assert_eq!(
            without_key.fallback_reason(Chain::Arbitrum),
            FallbackReason::MissingApiKey
        );
        // No provider is ever called for these chains, so there's nothing
        // to authenticate — never reported as a missing-key degradation.
        assert_eq!(
            without_key.fallback_reason(Chain::Solana),
            FallbackReason::ProviderOutage
        );
    }
}
