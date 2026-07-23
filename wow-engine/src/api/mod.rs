use crate::anchor::{sep24::Sep24Client, sep38::Sep38Client, Sep24InteractiveResponse, Sep38Quote};
use crate::bridge::Chain;
use crate::db::models::RouteExecutionInput;
use crate::db::service::{ExecuteRouteResult, RouteExecutionService};
use crate::db::Database;
use crate::error::AppError;
use crate::router::slippage::SlippageError;
use crate::router::{RouteOption, RoutePlanner};
use axum::{
    routing::{get, post},
    Extension, Json, Router,
};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tower_http::timeout::TimeoutLayer;
use uuid::Uuid;

/// Default per-request timeout applied when [`create_router`] is used without an
/// explicit value. Kept in sync with [`crate::config::AppConfig`]'s default.
pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

pub mod auth;
pub mod middleware;
pub mod validation;
use auth::SignatureVerifier;
use validation::{validate_asset_code, validate_stellar_address};

#[derive(Serialize)]
pub struct ConfigResponse {
    pub chains: Vec<&'static str>,
    pub assets: Vec<&'static str>,
    pub bridges: Vec<&'static str>,
}

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
pub struct HealthResponse {
    pub status: &'static str,
    pub service: &'static str,
    pub version: &'static str,
    pub timestamp: String,
}

/// Builds the application router.
///
/// `db` injects the (optional) database used by `/execute-route`. `verifier`
/// injects the Ed25519 request-signature enforcement: when `Some`, every route
/// except the public allowlist ([`auth::PUBLIC_PATHS`]) requires a valid
/// signature; when `None`, verification is disabled entirely (intended only for
/// local development — see `main`, which warns loudly in that case).
pub fn create_router(db: Option<Database>, verifier: Option<SignatureVerifier>) -> Router {
    create_router_with_timeout(db, verifier, DEFAULT_REQUEST_TIMEOUT)
}

/// Like [`create_router`], but with an explicit per-request timeout.
///
/// The [`TimeoutLayer`] is the outermost application layer (added last, so it
/// wraps everything): if any handler — or a downstream dependency it is waiting
/// on — fails to produce a response within `request_timeout`, the request is
/// aborted and the client receives `408 Request Timeout` instead of hanging.
/// This is what prevents a single stalled bridge/database call from tying up a
/// connection forever.
pub fn create_router_with_timeout(
    db: Option<Database>,
    verifier: Option<SignatureVerifier>,
    request_timeout: Duration,
) -> Router {
    let router = Router::new()
        .route("/api/v1/health", get(health_handler))
        .route(
            "/api/v1/config",
            get(config_handler).layer(axum::middleware::from_fn(middleware::etag_middleware)),
        )
        .route("/api/v1/quote", post(quote_handler))
        .route("/api/v1/execute-route", post(execute_route_handler))
        .route("/api/v1/anchor/deposit", post(deposit_handler))
        .route("/api/v1/anchor/withdraw", post(withdraw_handler))
        .route("/api/v1/anchor/quote", post(anchor_quote_handler))
        .layer(Extension(db));

    // The signature layer is added last so it runs *first* — verification
    // happens before any handler (or its body extractor) sees the request.
    let router = match verifier {
        Some(verifier) => router.layer(axum::middleware::from_fn_with_state(
            verifier,
            auth::verify_signature,
        )),
        None => router,
    };

    // Timeout is the outermost layer so it also bounds the auth middleware and
    // body extraction, not just the leaf handler.
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

async fn config_handler() -> Json<ConfigResponse> {
    Json(ConfigResponse {
        chains: vec!["Ethereum", "Arbitrum", "Solana", "Stellar"],
        assets: vec!["ETH", "USDC", "SOL", "XLM"],
        bridges: vec!["deBridge", "CCTP"],
    })
}

#[tracing::instrument(err)]
async fn quote_handler(Json(payload): Json<QuoteRequest>) -> Result<Json<QuoteResponse>, AppError> {
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

    let planner = RoutePlanner::new();
    let routes = planner
        .find_best_route(
            payload.source_chain,
            payload.dest_chain,
            &payload.source_asset,
            &payload.dest_asset,
            payload.amount_in,
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

#[tracing::instrument(err)]
async fn deposit_handler(
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

    let client = Sep24Client::new();
    let tx = client
        .initiate_deposit(
            &payload.anchor_domain,
            &payload.asset_code,
            &payload.account,
        )
        .await?;
    Ok(Json(tx))
}

#[tracing::instrument(err)]
async fn withdraw_handler(
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

    let client = Sep24Client::new();
    let tx = client
        .initiate_withdrawal(
            &payload.anchor_domain,
            &payload.asset_code,
            &payload.account,
        )
        .await?;
    Ok(Json(tx))
}

#[tracing::instrument(err)]
async fn anchor_quote_handler(
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
    if payload.anchor_domain.trim().is_empty() {
        return Err(AppError::BadRequest(
            "Anchor domain cannot be empty".to_string(),
        ));
    }

    let client = Sep38Client::new();
    let quote = client
        .get_indicative_quote(
            &payload.anchor_domain,
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
    .map_err(map_route_execution_error)?;

    Ok(Json(result))
}

/// Classifies an error from [`RouteExecutionService::execute_route_with_quota`]
/// into the correct HTTP-facing [`AppError`].
///
/// Connection-pool starvation (`PoolTimedOut`) and a closed pool
/// (`PoolClosed`) are infrastructure problems, not client mistakes: under a
/// Postgres outage or a connection storm the pool's `acquire_timeout` fires and
/// we must surface `503 Service Unavailable` so the request fails fast and the
/// caller retries, instead of masquerading as a `400` or blocking the client.
/// Everything else (quota exceeded, bad references, etc.) remains a `400`.
pub(crate) fn map_route_execution_error(err: Box<dyn std::error::Error>) -> AppError {
    if let Some(sqlx_err) = err.downcast_ref::<sqlx::Error>() {
        if matches!(
            sqlx_err,
            sqlx::Error::PoolTimedOut | sqlx::Error::PoolClosed
        ) {
            return AppError::ServiceUnavailable(
                "Database connection pool exhausted; please retry shortly".to_string(),
            );
        }
    }
    AppError::BadRequest(format!("Route execution failed: {}", err))
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
