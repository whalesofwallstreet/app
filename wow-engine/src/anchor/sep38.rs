use crate::anchor::Sep38Quote;
use chrono::{DateTime, Utc};
use moka::future::Cache;
use reqwest_middleware::ClientWithMiddleware;
use serde::Deserialize;
use std::collections::HashMap;
use std::time::Duration;

/// How long an indicative rate stays valid once fetched, mirroring
/// [`crate::bridge::gas_oracle::GasOracle`]'s TTL-cache pattern.
const INDICATIVE_TTL_SECS: i64 = 60;
/// Firm quotes are a commitment the engine may have to honor, so they're
/// re-validated against the live source far more often than indicative ones.
const FIRM_TTL_SECS: i64 = 15;

const DEFAULT_FX_FALLBACK_BASE_URL: &str = "https://api.frankfurter.app";

/// (anchor_domain, sell_asset, buy_asset)
type RateKey = (String, String, String);

#[derive(Debug, Clone, Copy)]
struct CachedRate {
    price: f64,
    fetched_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct AnchorPriceResponse {
    price: String,
}

#[derive(Debug, Deserialize)]
struct FallbackFxResponse {
    rates: HashMap<String, f64>,
}

pub struct Sep38Client {
    client: ClientWithMiddleware,
    fx_fallback_base_url: String,
    indicative_cache: Cache<RateKey, CachedRate>,
    firm_cache: Cache<RateKey, CachedRate>,
}

impl Default for Sep38Client {
    fn default() -> Self {
        Self::new()
    }
}

impl Sep38Client {
    pub fn new() -> Self {
        Self::with_fx_fallback_base_url(DEFAULT_FX_FALLBACK_BASE_URL.to_string())
    }

    /// Test/deployment hook that overrides the fallback FX provider's base
    /// URL (e.g. to point at a mock server in unit tests).
    pub fn with_fx_fallback_base_url(fx_fallback_base_url: String) -> Self {
        Self {
            client: crate::http_client::build_resilient_client()
                .expect("Failed to build resilient HTTP client"),
            fx_fallback_base_url,
            indicative_cache: Cache::builder()
                .time_to_live(Duration::from_secs(INDICATIVE_TTL_SECS as u64))
                .build(),
            firm_cache: Cache::builder()
                .time_to_live(Duration::from_secs(FIRM_TTL_SECS as u64))
                .build(),
        }
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn get_indicative_quote(
        &self,
        anchor_domain: &str,
        sell_asset: &str,
        buy_asset: &str,
        sell_amount: f64,
    ) -> Result<Sep38Quote, anyhow::Error> {
        self.generate_quote(
            anchor_domain,
            sell_asset,
            buy_asset,
            sell_amount,
            &self.indicative_cache,
            INDICATIVE_TTL_SECS,
        )
        .await
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn get_firm_quote(
        &self,
        anchor_domain: &str,
        sell_asset: &str,
        buy_asset: &str,
        sell_amount: f64,
    ) -> Result<Sep38Quote, anyhow::Error> {
        // Firm quotes are re-fetched/re-validated on a tighter TTL than
        // indicative ones — see [`FIRM_TTL_SECS`].
        self.generate_quote(
            anchor_domain,
            sell_asset,
            buy_asset,
            sell_amount,
            &self.firm_cache,
            FIRM_TTL_SECS,
        )
        .await
    }

    async fn generate_quote(
        &self,
        anchor_domain: &str,
        sell_asset: &str,
        buy_asset: &str,
        sell_amount: f64,
        cache: &Cache<RateKey, CachedRate>,
        ttl_secs: i64,
    ) -> Result<Sep38Quote, anyhow::Error> {
        let key: RateKey = (
            anchor_domain.to_string(),
            sell_asset.to_string(),
            buy_asset.to_string(),
        );

        // `try_get_with` coalesces concurrent fetches for the same key (no
        // cache-stampede) and transparently returns the cached rate while
        // it's still within its TTL.
        let cached = cache
            .try_get_with(key, self.fetch_rate(anchor_domain, sell_asset, buy_asset))
            .await
            .map_err(|err| anyhow::anyhow!("{err}"))?;

        let quote_id = format!("q_sep38_{}", super::generate_uuid());
        let buy_amount = sell_amount * cached.price;
        let expires_at = (cached.fetched_at + chrono::Duration::seconds(ttl_secs))
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

        Ok(Sep38Quote {
            id: quote_id,
            expires_at,
            price: format!("{:.7}", cached.price),
            sell_asset: sell_asset.to_string(),
            sell_amount: format!("{:.7}", sell_amount),
            buy_asset: buy_asset.to_string(),
            buy_amount: format!("{:.7}", buy_amount),
        })
    }

    /// Sources a live rate for `sell_asset` -> `buy_asset`, preferring the
    /// anchor's own SEP-38 `/price` endpoint and falling back to a
    /// general-purpose FX provider when the anchor doesn't expose one.
    async fn fetch_rate(
        &self,
        anchor_domain: &str,
        sell_asset: &str,
        buy_asset: &str,
    ) -> Result<CachedRate, anyhow::Error> {
        let fetched_at = Utc::now();

        match self
            .fetch_anchor_price(anchor_domain, sell_asset, buy_asset)
            .await
        {
            Ok(price) => {
                tracing::info!(
                    source = "anchor",
                    anchor_domain,
                    sell_asset,
                    buy_asset,
                    price,
                    "Sourced SEP-38 price from anchor"
                );
                Ok(CachedRate { price, fetched_at })
            }
            Err(anchor_err) => {
                tracing::warn!(
                    anchor_domain,
                    "Anchor SEP-38 pricing unavailable ({anchor_err}); trying fallback FX provider"
                );

                match self.fetch_fallback_fx_rate(sell_asset, buy_asset).await {
                    Ok(price) => {
                        tracing::info!(
                            source = "fallback_fx",
                            sell_asset,
                            buy_asset,
                            price,
                            "Sourced SEP-38 price from fallback FX provider"
                        );
                        Ok(CachedRate { price, fetched_at })
                    }
                    Err(fallback_err) => Err(anyhow::anyhow!(
                        "Unable to source a live SEP-38 price for {sell_asset}->{buy_asset}: \
                         anchor error: {anchor_err}; fallback FX provider error: {fallback_err}"
                    )),
                }
            }
        }
    }

    async fn fetch_anchor_price(
        &self,
        anchor_domain: &str,
        sell_asset: &str,
        buy_asset: &str,
    ) -> Result<f64, anyhow::Error> {
        let base = if anchor_domain.starts_with("http://") || anchor_domain.starts_with("https://")
        {
            anchor_domain.trim_end_matches('/').to_string()
        } else {
            format!("https://{anchor_domain}")
        };
        let url = format!("{base}/sep38/price");

        let response = self
            .client
            .get(&url)
            .query(&[("sell_asset", sell_asset), ("buy_asset", buy_asset)])
            .send()
            .await?
            .error_for_status()?;

        let parsed: AnchorPriceResponse = response.json().await?;
        let price: f64 = parsed.price.parse().map_err(|_| {
            anyhow::anyhow!("Anchor returned a non-numeric price: {}", parsed.price)
        })?;

        if !price.is_finite() || price <= 0.0 {
            return Err(anyhow::anyhow!("Anchor returned an invalid price: {price}"));
        }

        Ok(price)
    }

    async fn fetch_fallback_fx_rate(
        &self,
        sell_asset: &str,
        buy_asset: &str,
    ) -> Result<f64, anyhow::Error> {
        let from = normalize_currency_code(sell_asset);
        let to = normalize_currency_code(buy_asset);

        if from == to {
            return Ok(1.0);
        }

        let url = format!("{}/latest", self.fx_fallback_base_url);
        let response = self
            .client
            .get(&url)
            .query(&[("from", from.as_str()), ("to", to.as_str())])
            .send()
            .await?
            .error_for_status()?;

        let parsed: FallbackFxResponse = response.json().await?;
        parsed
            .rates
            .get(&to)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("Fallback FX provider did not return a rate for {to}"))
    }
}

/// Best-effort mapping from a SEP-38 asset identifier (which may carry a
/// `stellar:CODE:ISSUER`/`iso4217:CODE` scheme prefix or be a fiat-pegged
/// stablecoin ticker) down to the plain ISO-4217 code the fallback FX
/// provider expects.
fn normalize_currency_code(asset: &str) -> String {
    let parts: Vec<&str> = asset.split(':').collect();
    let code = match parts.as_slice() {
        // "stellar:CODE:ISSUER" -> CODE is the middle segment, not the
        // trailing issuer address.
        ["stellar", code, ..] => *code,
        ["iso4217", code] => *code,
        [code] => *code,
        _ => asset,
    }
    .to_uppercase();

    match code.as_str() {
        "USDC" | "USDT" | "USD" => "USD".to_string(),
        "EURT" | "EURC" | "EUR" => "EUR".to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn test_normalize_currency_code_strips_scheme_and_maps_stablecoins() {
        assert_eq!(normalize_currency_code("iso4217:NGN"), "NGN");
        assert_eq!(
            normalize_currency_code(
                "stellar:USDC:GA5Z3IX5VQ3N6FB77T342A27RWRN7CKEZ63M3W7S5VJB3D77J6F2JAFK"
            ),
            "USD"
        );
        assert_eq!(normalize_currency_code("EURT"), "EUR");
        assert_eq!(normalize_currency_code("XLM"), "XLM");
    }

    #[tokio::test]
    async fn test_get_indicative_quote_sources_price_from_anchor() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/sep38/price"))
            .and(query_param("sell_asset", "USDC"))
            .and(query_param("buy_asset", "NGN"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "price": "1500.0000000"
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = Sep38Client::new();
        let quote = client
            .get_indicative_quote(&mock_server.uri(), "USDC", "NGN", 100.0)
            .await
            .unwrap();

        assert_eq!(quote.price, "1500.0000000");
        assert_eq!(quote.buy_amount, "150000.0000000");
        assert_eq!(quote.sell_asset, "USDC");
        assert_eq!(quote.buy_asset, "NGN");
        assert!(quote.id.starts_with("q_sep38_"));
    }

    #[tokio::test]
    async fn test_repeated_calls_within_ttl_hit_cache_not_anchor() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/sep38/price"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "price": "1.2300000"
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = Sep38Client::new();
        let first = client
            .get_indicative_quote(&mock_server.uri(), "USDC", "EURT", 10.0)
            .await
            .unwrap();
        let second = client
            .get_indicative_quote(&mock_server.uri(), "USDC", "EURT", 10.0)
            .await
            .unwrap();

        // wiremock's `.expect(1)` (checked on drop) confirms only one
        // outbound anchor request was made across both calls.
        assert_eq!(first.price, second.price);
    }

    #[tokio::test]
    async fn test_falls_back_to_fx_provider_when_anchor_unreachable() {
        let anchor_mock = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/sep38/price"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&anchor_mock)
            .await;

        let fallback_mock = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/latest"))
            .and(query_param("from", "USD"))
            .and(query_param("to", "EUR"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "rates": { "EUR": 0.9000000 }
            })))
            .expect(1)
            .mount(&fallback_mock)
            .await;

        let client = Sep38Client::with_fx_fallback_base_url(fallback_mock.uri());
        let quote = client
            .get_indicative_quote(&anchor_mock.uri(), "USDC", "EURT", 200.0)
            .await
            .unwrap();

        assert_eq!(quote.price, "0.9000000");
        assert_eq!(quote.buy_amount, "180.0000000");
    }

    #[tokio::test]
    async fn test_returns_clear_error_when_anchor_and_fallback_both_unreachable() {
        let anchor_mock = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/sep38/price"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&anchor_mock)
            .await;

        let fallback_mock = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/latest"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&fallback_mock)
            .await;

        let client = Sep38Client::with_fx_fallback_base_url(fallback_mock.uri());
        let result = client
            .get_indicative_quote(&anchor_mock.uri(), "USDC", "EURT", 200.0)
            .await;

        assert!(result.is_err());
        let message = result.unwrap_err().to_string();
        assert!(message.contains("Unable to source a live SEP-38 price"));
    }

    #[tokio::test]
    async fn test_get_indicative_quote_uses_longer_expiry_than_firm_quote() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/sep38/price"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "price": "1.0000000"
            })))
            .mount(&mock_server)
            .await;

        let client = Sep38Client::new();

        let indicative = client
            .get_indicative_quote(&mock_server.uri(), "USDC", "NGN", 50.0)
            .await
            .unwrap();
        let firm = client
            .get_firm_quote(&mock_server.uri(), "USDC", "NGN", 50.0)
            .await
            .unwrap();

        let indicative_expiry = chrono::DateTime::parse_from_rfc3339(&indicative.expires_at)
            .unwrap()
            .timestamp();
        let firm_expiry = chrono::DateTime::parse_from_rfc3339(&firm.expires_at)
            .unwrap()
            .timestamp();

        // Indicative quotes (60s TTL) must expire later than firm quotes
        // (15s TTL, re-validated against the live source more often).
        assert!(indicative_expiry > firm_expiry);
    }
}
