use crate::anchor::tracker::{TrackerStore, Transaction};
use crate::anchor::{sep24::Sep24Client, sep38::Sep38Client, Sep24InteractiveResponse, Sep38Quote};
use crate::bridge::attestation::AttestationError;
use crate::bridge::cctp::CctpClient;
use crate::bridge::gas_oracle::GasOracle;
use crate::bridge::Chain;
use crate::cache_sync::{ClusterCache, InvalidationMessage};
use crate::config::AppConfig;
use crate::db::models::RouteExecutionInput;
use crate::db::service::{ExecuteRouteResult, RouteExecutionService};
use crate::db::Database;
use crate::error::AppError;
use crate::mempool::PoolRiskRegistry;
use crate::router::slippage::SlippageError;
use crate::router::{RouteOption, RoutePlanner};
use axum::{
    extract::{Path, Query},
    routing::{get, post},
    Extension, Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tower_http::timeout::TimeoutLayer;
use uuid::Uuid;

pub mod auth;
pub mod cors;
pub mod middleware;
pub mod rate_limit;
pub mod validation;
use auth::SignatureVerifier;
use rate_limit::RateLimiter;
use validation::{validate_anchor_domain, validate_asset_code, validate_stellar_address};

#[derive(Deserialize, Debug)]
pub struct QuoteRequest {
    pub source_chain: Chain,
    pub dest_chain: Chain,
    pub source_asset: String,
    pub dest_asset: String,
    pub amount_in: u64,
}

#[derive(Serialize)]
pub struct QuoteResponse {
    pub routes: Vec<RouteOption>,
}

#[derive(Deserialize, Debug)]
pub struct DepositRequest {
    pub anchor_domain: String,
    pub asset_code: String,
    pub account: String,
}

#[derive(Deserialize, Debug)]
pub struct WithdrawRequest {
    pub anchor_domain: String,
    pub asset_code: String,
    pub account: String,
}

#[derive(Deserialize, Debug)]
pub struct AnchorQuoteRequest {
    pub anchor_domain: String,
    pub sell_asset: String,
    pub buy_asset: String,
    pub sell_amount: f64,
}

#[derive(Deserialize, Debug)]
pub struct ExecuteRouteRequest {
    pub user_id: Uuid,
    pub source_chain: String,
    pub dest_chain: String,
    pub source_asset: String,
    pub dest_asset: String,
    pub amount_in: u64,
    pub amount_out: u64,
    pub provider: String,
    pub path: String,
    pub estimated_fee_usd: f64,
    pub anchor_domain: Option<String>,
    pub anchor_transaction_id: Option<String>,
}

#[derive(Serialize)]
pub struct ConfigResponse {
    pub chains: Vec<&'static str>,
    pub assets: Vec<&'static str>,
    pub bridges: Vec<&'static str>,
}

#[derive(Deserialize, Debug)]
pub struct InvalidateCacheRequest {
    #[serde(default)]
    pub chain: Option<Chain>,
}

#[derive(Serialize)]
pub struct InvalidateCacheResponse {
    pub invalidated: &'static str,
    pub broadcast: bool,
}

#[derive(Deserialize, Debug)]
pub struct VerifyAttestationRequest {
    pub dest_chain: Chain,
    pub message: String,
    pub attestation: String,
}

#[derive(Serialize)]
pub struct VerifyAttestationResponse {
    pub verified: bool,
    pub source_domain: u32,
    pub destination_domain: u32,
    pub nonce: u64,
}

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub service: &'static str,
    pub version: &'static str,
    pub timestamp: String,
}

#[derive(Deserialize)]
pub struct PortfolioRequest {
    pub account: String,
}

#[derive(Serialize)]
pub struct PortfolioResponse {
    pub balance: f64,
    pub transactions: Vec<PortfolioTransaction>,
}

#[derive(Serialize)]
pub struct PortfolioTransaction {
    pub id: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub name: String,
    pub username: String,
    pub avatar: String,
    pub amount: f64,
    pub note: Option<String>,
    pub date: String,
    pub status: String,
}

#[derive(Deserialize)]
struct HorizonAccount {
    balances: Vec<HorizonBalance>,
}

#[derive(Deserialize)]
struct HorizonBalance {
    asset_type: String,
    asset_code: Option<String>,
    balance: String,
}

#[derive(Deserialize)]
struct HorizonPayments {
    _embedded: HorizonPaymentEmbedded,
}

#[derive(Deserialize)]
struct HorizonPaymentEmbedded {
    records: Vec<HorizonPayment>,
}

#[derive(Deserialize)]
struct HorizonPayment {
    id: String,
    from: Option<String>,
    to: Option<String>,
    amount: String,
    created_at: String,
    transaction_hash: Option<String>,
    asset_type: String,
    asset_code: Option<String>,
}

/// Shared service handles the router injects as `Extension`s — every route
/// handler reaches these the same way, so they're bundled here instead of
/// as separate positional parameters to [`create_router_with_cache`].
/// `verifier` and `request_timeout` stay separate parameters since they
/// drive router construction itself (which middleware gets layered on)
/// rather than being handed to handlers untouched.
pub struct RouterDeps {
    pub db: Option<Database>,
    pub tracker: Option<Arc<TrackerStore>>,
    pub cache: ClusterCache,
    pub config: Arc<AppConfig>,
    pub sep38_client: Arc<Sep38Client>,
    pub mempool_risk_registry: Arc<PoolRiskRegistry>,
}

/// Builds the application router.
///
/// `db` injects the (optional) database used by `/execute-route`. `verifier`
/// injects the Ed25519 request-signature enforcement: when `Some`, every route
/// except the public allowlist ([`auth::PUBLIC_PATHS`]) requires a valid
/// signature; when `None`, verification is disabled entirely (intended only for
/// local development — see `main`, which warns loudly in that case).
pub fn create_router(db: Option<Database>, verifier: Option<SignatureVerifier>) -> Router {
    create_router_with_cache(
        verifier,
        Duration::from_secs(30),
        RouterDeps {
            db,
            tracker: None,
            cache: ClusterCache::local_only(),
            config: Arc::new(AppConfig::default()),
            sep38_client: Arc::new(Sep38Client::new()),
            mempool_risk_registry: Arc::new(PoolRiskRegistry::new()),
        },
    )
}

pub fn create_router_with_cache(
    verifier: Option<SignatureVerifier>,
    request_timeout: Duration,
    deps: RouterDeps,
) -> Router {
    let RouterDeps {
        db,
        tracker,
        cache,
        config,
        sep38_client,
        mempool_risk_registry,
    } = deps;

    // Every route shares a global per-IP budget; `/api/v1/quote` additionally
    // gets its own, stricter budget since it runs a non-trivial pathfinding
    // search per request.
    let quote_limiter = RateLimiter::new(
        config.rate_limit_quote_per_minute,
        Duration::from_secs(60),
        config.trust_proxy_headers,
    );
    let global_limiter = RateLimiter::new(
        config.rate_limit_global_per_minute,
        Duration::from_secs(60),
        config.trust_proxy_headers,
    );

    let router = Router::new()
        .route("/api/v1/health", get(health_handler))
        .route("/api/v1/wallet/portfolio", get(portfolio_handler))
        .route(
            "/api/v1/config",
            get(config_handler).layer(axum::middleware::from_fn(middleware::etag_middleware)),
        )
        .route(
            "/api/v1/quote",
            post(quote_handler).layer(axum::middleware::from_fn_with_state(
                quote_limiter,
                rate_limit::rate_limit_middleware,
            )),
        )
        .route("/api/v1/execute-route", post(execute_route_handler))
        .route("/api/v1/anchor/deposit", post(deposit_handler))
        .route("/api/v1/anchor/withdraw", post(withdraw_handler))
        .route("/api/v1/anchor/quote", post(anchor_quote_handler))
        .route("/api/v1/anchor/transaction/:id", get(transaction_handler))
        .route(
            "/api/v1/admin/invalidate-cache",
            post(admin_invalidate_cache_handler),
        )
        .route(
            "/api/v1/cctp/verify-attestation",
            post(verify_attestation_handler),
        )
        .layer(Extension(db))
        .layer(Extension(cache))
        .layer(Extension(config))
        .layer(Extension(tracker))
        .layer(Extension(sep38_client))
        .layer(Extension(mempool_risk_registry))
        .layer(axum::middleware::from_fn_with_state(
            global_limiter,
            rate_limit::rate_limit_middleware,
        ));

    // The signature layer is added last so it runs *first* — verification
    // happens before any handler (or its body extractor) sees the request.
    let router = match verifier {
        Some(verifier) => router.layer(axum::middleware::from_fn_with_state(
            verifier,
            auth::verify_signature,
        )),
        None => router,
    };

    router.layer(TimeoutLayer::new(request_timeout))
}

async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "wow-engine",
        version: "0.1.0",
        timestamp: chrono::Utc::now().to_rfc3339(),
    })
}

async fn portfolio_handler(
    Extension(config): Extension<Arc<AppConfig>>,
    Query(payload): Query<PortfolioRequest>,
) -> Result<Json<PortfolioResponse>, AppError> {
    validate_stellar_address(&payload.account)
        .map_err(|error| AppError::BadRequest(format!("Invalid account address: {error}")))?;

    let client = crate::http_client::build_resilient_client()
        .map_err(|error| AppError::Internal(error.into()))?;
    let horizon_url = config.stellar_horizon_url.trim_end_matches('/');
    let account_url = format!("{horizon_url}/accounts/{}", payload.account);
    let account = client
        .get(&account_url)
        .send()
        .await
        .map_err(|error| {
            AppError::ServiceUnavailable(format!("Stellar account lookup failed: {error}"))
        })?
        .error_for_status()
        .map_err(|error| {
            AppError::ServiceUnavailable(format!("Stellar account lookup failed: {error}"))
        })?
        .json::<HorizonAccount>()
        .await
        .map_err(|error| {
            AppError::ServiceUnavailable(format!("Invalid Stellar account response: {error}"))
        })?;

    let balance = account
        .balances
        .iter()
        .filter(|balance| {
            balance.asset_type == "native" || balance.asset_code.as_deref() == Some("USDC")
        })
        .filter_map(|balance| balance.balance.parse::<f64>().ok())
        .sum();

    let payments_url = format!(
        "{horizon_url}/accounts/{}/payments?limit=50&order=desc",
        payload.account
    );
    let payments = client
        .get(&payments_url)
        .send()
        .await
        .map_err(|error| {
            AppError::ServiceUnavailable(format!("Stellar payments lookup failed: {error}"))
        })?
        .error_for_status()
        .map_err(|error| {
            AppError::ServiceUnavailable(format!("Stellar payments lookup failed: {error}"))
        })?
        .json::<HorizonPayments>()
        .await
        .map_err(|error| {
            AppError::ServiceUnavailable(format!("Invalid Stellar payments response: {error}"))
        })?;

    let transactions = payments
        ._embedded
        .records
        .into_iter()
        .filter_map(|payment| {
            if payment.asset_type != "native" && payment.asset_code.as_deref() != Some("USDC") {
                return None;
            }
            let received = payment.to.as_deref() == Some(payload.account.as_str());
            let counterparty = if received { payment.from? } else { payment.to? };
            Some(PortfolioTransaction {
                id: payment.transaction_hash.unwrap_or(payment.id),
                type_: if received { "received" } else { "sent" }.to_string(),
                name: counterparty.clone(),
                username: counterparty,
                avatar: String::new(),
                amount: payment.amount.parse().ok()?,
                note: None,
                date: payment.created_at,
                status: "completed".to_string(),
            })
        })
        .collect();

    Ok(Json(PortfolioResponse {
        balance,
        transactions,
    }))
}

#[tracing::instrument(err)]
async fn quote_handler(
    Extension(config): Extension<Arc<AppConfig>>,
    Extension(risk_registry): Extension<Arc<PoolRiskRegistry>>,
    Json(payload): Json<QuoteRequest>,
) -> Result<Json<QuoteResponse>, AppError> {
    if payload.source_asset.trim().is_empty() {
        return Err(AppError::BadRequest(
            "Source asset cannot be empty".to_string(),
        ));
    }
    if payload.dest_asset.trim().is_empty() {
        return Err(AppError::BadRequest(
            "Destination asset cannot be empty".to_string(),
        ));
    }
    if payload.amount_in == 0 {
        return Err(AppError::BadRequest(
            "Amount in must be greater than zero".to_string(),
        ));
    }

    let planner = RoutePlanner::with_risk_registry(config, risk_registry);
    let routes = planner
        .find_best_route(
            payload.source_chain,
            payload.dest_chain,
            &payload.source_asset,
            &payload.dest_asset,
            payload.amount_in,
            false,
        )
        .await
        .map_err(|err| {
            // A catastrophic price-impact rejection is a property of the
            // requested trade, not an engine failure: report it as a 400
            // with the explanatory message.
            if err.downcast_ref::<SlippageError>().is_some() {
                AppError::BadRequest(err.to_string())
            } else {
                AppError::Internal(err)
            }
        })?;
    Ok(Json(QuoteResponse { routes }))
}

#[tracing::instrument(skip(tracker), err)]
async fn deposit_handler(
    tracker: Extension<Option<Arc<TrackerStore>>>,
    Extension(config): Extension<Arc<AppConfig>>,
    Json(payload): Json<DepositRequest>,
) -> Result<Json<Sep24InteractiveResponse>, AppError> {
    if let Err(err) = validate_stellar_address(&payload.account) {
        return Err(AppError::BadRequest(format!(
            "Invalid account address: {}",
            err
        )));
    }
    if let Err(err) = validate_asset_code(&payload.asset_code) {
        return Err(AppError::BadRequest(format!("Invalid asset code: {}", err)));
    }
    if payload.anchor_domain.trim().is_empty() {
        return Err(AppError::BadRequest(
            "Anchor domain cannot be empty".to_string(),
        ));
    }

    let tracker = tracker.0.ok_or_else(|| {
        AppError::Internal(anyhow::anyhow!(
            "Database not configured for anchor tracker"
        ))
    })?;

    let client = Sep24Client::new(
        tracker,
        &config.sep10_signing_keys,
        config.sep10_challenge_max_age_secs,
        config.sep10_challenge_max_future_skew_secs,
    )
    .map_err(AppError::Internal)?;
    let tx = client
        .initiate_deposit(
            &payload.anchor_domain,
            &payload.asset_code,
            &payload.account,
        )
        .await?;
    Ok(Json(tx))
}

#[tracing::instrument(skip(tracker), err)]
async fn withdraw_handler(
    tracker: Extension<Option<Arc<TrackerStore>>>,
    Extension(config): Extension<Arc<AppConfig>>,
    Json(payload): Json<WithdrawRequest>,
) -> Result<Json<Sep24InteractiveResponse>, AppError> {
    if let Err(err) = validate_stellar_address(&payload.account) {
        return Err(AppError::BadRequest(format!(
            "Invalid account address: {}",
            err
        )));
    }
    if let Err(err) = validate_asset_code(&payload.asset_code) {
        return Err(AppError::BadRequest(format!("Invalid asset code: {}", err)));
    }
    if payload.anchor_domain.trim().is_empty() {
        return Err(AppError::BadRequest(
            "Anchor domain cannot be empty".to_string(),
        ));
    }

    let tracker = tracker.0.ok_or_else(|| {
        AppError::Internal(anyhow::anyhow!(
            "Database not configured for anchor tracker"
        ))
    })?;

    let client = Sep24Client::new(
        tracker,
        &config.sep10_signing_keys,
        config.sep10_challenge_max_age_secs,
        config.sep10_challenge_max_future_skew_secs,
    )
    .map_err(AppError::Internal)?;
    let tx = client
        .initiate_withdrawal(
            &payload.anchor_domain,
            &payload.asset_code,
            &payload.account,
        )
        .await?;
    Ok(Json(tx))
}

#[tracing::instrument(skip(tracker), err)]
async fn transaction_handler(
    tracker: Extension<Option<Arc<TrackerStore>>>,
    Path(id): Path<String>,
) -> Result<Json<Transaction>, AppError> {
    let tracker = tracker.0.ok_or_else(|| {
        AppError::Internal(anyhow::anyhow!(
            "Database not configured for anchor tracker"
        ))
    })?;

    let tx = tracker
        .get_transaction(&id)
        .await
        .map_err(|err| AppError::Internal(err.into()))?
        .ok_or_else(|| AppError::NotFound(format!("Unknown transaction id: {id}")))?;

    Ok(Json(tx))
}

#[tracing::instrument(skip(client, config), err)]
async fn anchor_quote_handler(
    Extension(client): Extension<Arc<Sep38Client>>,
    Extension(config): Extension<Arc<AppConfig>>,
    Json(payload): Json<AnchorQuoteRequest>,
) -> Result<Json<Sep38Quote>, AppError> {
    if let Err(err) = validate_asset_code(&payload.sell_asset) {
        return Err(AppError::BadRequest(format!("Invalid sell asset: {}", err)));
    }
    if let Err(err) = validate_asset_code(&payload.buy_asset) {
        return Err(AppError::BadRequest(format!("Invalid buy asset: {}", err)));
    }
    if payload.sell_amount <= 0.0 {
        return Err(AppError::BadRequest(
            "Sell amount must be greater than zero".to_string(),
        ));
    }
    // `fetch_anchor_price` (inside `get_indicative_quote`) makes a live
    // outbound HTTP request to this domain, and `/api/v1/anchor/quote` is
    // unauthenticated (PUBLIC_PATHS) — without this check, any caller could
    // point anchor_domain at an internal host or a cloud metadata address
    // and have us fetch it for them. Same allowlist + SSRF guard already
    // enforced by deposit/withdraw's anchor-domain handling.
    let anchor_domain =
        validate_anchor_domain(&payload.anchor_domain, &config.allowed_anchor_domains)
            .map_err(AppError::BadRequest)?;

    let quote = client
        .get_indicative_quote(
            &anchor_domain,
            &payload.sell_asset,
            &payload.buy_asset,
            payload.sell_amount,
        )
        .await?;
    Ok(Json(quote))
}

#[tracing::instrument(skip(db), err)]
async fn execute_route_handler(
    Extension(db): Extension<Option<Database>>,
    Json(payload): Json<ExecuteRouteRequest>,
) -> Result<Json<ExecuteRouteResult>, AppError> {
    let db = db.ok_or_else(|| {
        AppError::Internal(anyhow::anyhow!(
            "Database not configured for this server instance"
        ))
    })?;

    if payload.amount_in == 0 {
        return Err(AppError::BadRequest(
            "Amount in must be greater than zero".to_string(),
        ));
    }
    if payload.amount_out == 0 {
        return Err(AppError::BadRequest(
            "Amount out must be greater than zero".to_string(),
        ));
    }
    if payload.estimated_fee_usd < 0.0 {
        return Err(AppError::BadRequest(
            "Estimated fee cannot be negative".to_string(),
        ));
    }

    let route_input = RouteExecutionInput {
        user_id: payload.user_id,
        source_chain: payload.source_chain,
        dest_chain: payload.dest_chain,
        source_asset: payload.source_asset,
        dest_asset: payload.dest_asset,
        amount_in: payload.amount_in as i64,
        amount_out: payload.amount_out as i64,
        provider: payload.provider,
        path: payload.path,
        estimated_fee_usd: payload.estimated_fee_usd,
    };

    let result = RouteExecutionService::execute_route_with_quota(
        &db,
        route_input,
        payload.anchor_domain.as_deref(),
        payload.anchor_transaction_id.as_deref(),
    )
    .await
    .map_err(|e| AppError::BadRequest(format!("Route execution failed: {}", e)))?;

    Ok(Json(result))
}

async fn config_handler() -> Json<ConfigResponse> {
    Json(ConfigResponse {
        chains: vec!["Ethereum", "Arbitrum", "Solana", "Stellar"],
        assets: vec!["ETH", "USDC", "SOL", "XLM"],
        bridges: vec!["deBridge", "CCTP"],
    })
}

async fn admin_invalidate_cache_handler(
    Extension(cache): Extension<ClusterCache>,
    Json(payload): Json<InvalidateCacheRequest>,
) -> Result<Json<InvalidateCacheResponse>, AppError> {
    let (message, invalidated) = match payload.chain {
        Some(chain) => (InvalidationMessage::InvalidateChain { chain }, "chain"),
        None => (InvalidationMessage::InvalidateAll, "all"),
    };
    let broadcast = cache.broadcaster.is_some();

    cache.invalidate(message).await;

    Ok(Json(InvalidateCacheResponse {
        invalidated,
        broadcast,
    }))
}

fn decode_hex_field(value: &str, field: &str) -> Result<Vec<u8>, AppError> {
    hex::decode(value.trim_start_matches("0x"))
        .map_err(|err| AppError::BadRequest(format!("Invalid hex in {field}: {err}")))
}

async fn verify_attestation_handler(
    Extension(config): Extension<Arc<AppConfig>>,
    Json(payload): Json<VerifyAttestationRequest>,
) -> Result<Json<VerifyAttestationResponse>, AppError> {
    let message = decode_hex_field(&payload.message, "message")?;
    let attestation = decode_hex_field(&payload.attestation, "attestation")?;

    let parsed = CctpClient::new(Arc::new(GasOracle::new(config.clone())), config)
        .verify_attestation(payload.dest_chain, &message, &attestation)
        .await
        .map_err(|err| match err {
            AttestationError::KeySourceUnavailable | AttestationError::NonceStoreUnavailable(_) => {
                AppError::Internal(anyhow::Error::new(err))
            }
            other => AppError::BadRequest(format!("Attestation rejected: {other}")),
        })?;

    Ok(Json(VerifyAttestationResponse {
        verified: true,
        source_domain: parsed.source_domain,
        destination_domain: parsed.destination_domain,
        nonce: parsed.nonce,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_stellar_address() {
        // Valid address (only A-Z and 2-7, length 56, starts with G)
        assert!(validate_stellar_address(
            "GA5Z3IX5VQ3N6FB77T342A27RWRN7CKEZ63M3W7S5VJB3D77J6F2JAFK"
        )
        .is_ok());

        // Invalid starting char
        assert!(validate_stellar_address(
            "SA5Z3IX5VQ3N6FB77T342A27RWRN7CKEZ63M3W7S5VJB3D77J6F2JAFK"
        )
        .is_err());

        // Invalid length
        assert!(validate_stellar_address("GA5Z3IX5").is_err());

        // Invalid characters (e.g. contains 0, 1, 8, 9)
        assert!(validate_stellar_address(
            "GA5Z3IX5VQ3N6FB77T342A27RWRN7CKEZ63M3W7S5VJB3D77J6F2JA0K"
        )
        .is_err());
    }

    #[test]
    fn test_validate_asset_code() {
        // Alphanumeric standard
        assert!(validate_asset_code("USDC").is_ok());
        assert!(validate_asset_code("XLM").is_ok());
        assert!(validate_asset_code("EURT").is_ok());

        // Fully qualified
        assert!(validate_asset_code(
            "stellar:USDC:GA5Z3IX5VQ3N6FB77T342A27RWRN7CKEZ63M3W7S5VJB3D77J6F2JAFK"
        )
        .is_ok());
        assert!(validate_asset_code(
            "stellar:USDC:SA5Z3IX5VQ3N6FB77T342A27RWRN7CKEZ63M3W7S5VJB3D77J6F2JAFK"
        )
        .is_err());
        assert!(validate_asset_code(
            "stellar::GA5Z3IX5VQ3N6FB77T342A27RWRN7CKEZ63M3W7S5VJB3D77J6F2JAFK"
        )
        .is_err());

        // ISO-4217 format
        assert!(validate_asset_code("iso4217:USD").is_ok());
        assert!(validate_asset_code("iso4217:NGN").is_ok());
        assert!(validate_asset_code("iso4217:US").is_err());

        // Empty & too long
        assert!(validate_asset_code("").is_err());
        assert!(validate_asset_code("VERYLONGASSETCODE").is_err());
    }
}
