use axum::{
    routing::{get, post},
    Router, Json, http::StatusCode,
};
use serde::{Deserialize, Serialize};
use crate::bridge::Chain;
use crate::router::{RoutePlanner, RouteOption};
use crate::anchor::{sep24::Sep24Client, sep38::Sep38Client, Sep24InteractiveResponse, Sep38Quote};

pub mod validation;
use validation::{validate_stellar_address, validate_asset_code};


#[derive(Deserialize)]
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

#[derive(Deserialize)]
pub struct DepositRequest {
    pub anchor_domain: String,
    pub asset_code: String,
    pub account: String,
}

#[derive(Deserialize)]
pub struct WithdrawRequest {
    pub anchor_domain: String,
    pub asset_code: String,
    pub account: String,
}

#[derive(Deserialize)]
pub struct AnchorQuoteRequest {
    pub anchor_domain: String,
    pub sell_asset: String,
    pub buy_asset: String,
    pub sell_amount: f64,
}

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub service: &'static str,
    pub version: &'static str,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

pub enum AppError {
    BadRequest(String),
    Internal(String),
}

impl axum::response::IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let (status, err_msg) = match self {
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            AppError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };
        let body = Json(ErrorResponse { error: err_msg });
        (status, body).into_response()
    }
}

pub fn create_router() -> Router {
    Router::new()
        .route("/api/v1/health", get(health_handler))
        .route("/api/v1/quote", post(quote_handler))
        .route("/api/v1/anchor/deposit", post(deposit_handler))
        .route("/api/v1/anchor/withdraw", post(withdraw_handler))
        .route("/api/v1/anchor/quote", post(anchor_quote_handler))
}


async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "wow-engine",
        version: "0.1.0",
    })
}

async fn quote_handler(
    Json(payload): Json<QuoteRequest>,
) -> Result<Json<QuoteResponse>, AppError> {
    if payload.source_asset.trim().is_empty() {
        return Err(AppError::BadRequest("Source asset cannot be empty".to_string()));
    }
    if payload.dest_asset.trim().is_empty() {
        return Err(AppError::BadRequest("Destination asset cannot be empty".to_string()));
    }
    if payload.amount_in == 0 {
        return Err(AppError::BadRequest("Amount in must be greater than zero".to_string()));
    }

    let planner = RoutePlanner::new();
    match planner.find_best_route(
        payload.source_chain,
        payload.dest_chain,
        &payload.source_asset,
        &payload.dest_asset,
        payload.amount_in,
    ).await {
        Ok(routes) => Ok(Json(QuoteResponse { routes })),
        Err(e) => Err(AppError::Internal(e.to_string())),
    }
}

async fn deposit_handler(
    Json(payload): Json<DepositRequest>,
) -> Result<Json<Sep24InteractiveResponse>, AppError> {
    if let Err(err) = validate_stellar_address(&payload.account) {
        return Err(AppError::BadRequest(format!("Invalid account address: {}", err)));
    }
    if let Err(err) = validate_asset_code(&payload.asset_code) {
        return Err(AppError::BadRequest(format!("Invalid asset code: {}", err)));
    }
    if payload.anchor_domain.trim().is_empty() {
        return Err(AppError::BadRequest("Anchor domain cannot be empty".to_string()));
    }

    let client = Sep24Client::new();
    match client.initiate_deposit(
        &payload.anchor_domain,
        &payload.asset_code,
        &payload.account,
    ).await {
        Ok(tx) => Ok(Json(tx)),
        Err(e) => Err(AppError::Internal(e.to_string())),
    }
}

async fn withdraw_handler(
    Json(payload): Json<WithdrawRequest>,
) -> Result<Json<Sep24InteractiveResponse>, AppError> {
    if let Err(err) = validate_stellar_address(&payload.account) {
        return Err(AppError::BadRequest(format!("Invalid account address: {}", err)));
    }
    if let Err(err) = validate_asset_code(&payload.asset_code) {
        return Err(AppError::BadRequest(format!("Invalid asset code: {}", err)));
    }
    if payload.anchor_domain.trim().is_empty() {
        return Err(AppError::BadRequest("Anchor domain cannot be empty".to_string()));
    }

    let client = Sep24Client::new();
    match client.initiate_withdrawal(
        &payload.anchor_domain,
        &payload.asset_code,
        &payload.account,
    ).await {
        Ok(tx) => Ok(Json(tx)),
        Err(e) => Err(AppError::Internal(e.to_string())),
    }
}

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
        return Err(AppError::BadRequest("Sell amount must be greater than zero".to_string()));
    }
    if payload.anchor_domain.trim().is_empty() {
        return Err(AppError::BadRequest("Anchor domain cannot be empty".to_string()));
    }

    let client = Sep38Client::new();
    match client.get_indicative_quote(
        &payload.anchor_domain,
        &payload.sell_asset,
        &payload.buy_asset,
        payload.sell_amount,
    ).await {
        Ok(quote) => Ok(Json(quote)),
        Err(e) => Err(AppError::Internal(e.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_stellar_address() {
        // Valid address (only A-Z and 2-7, length 56, starts with G)
        assert!(validate_stellar_address("GA5Z3IX5VQ3N6FB77T342A27RWRN7CKEZ63M3W7S5VJB3D77J6F2JAFK").is_ok());
        
        // Invalid starting char
        assert!(validate_stellar_address("SA5Z3IX5VQ3N6FB77T342A27RWRN7CKEZ63M3W7S5VJB3D77J6F2JAFK").is_err());
        
        // Invalid length
        assert!(validate_stellar_address("GA5Z3IX5").is_err());
        
        // Invalid characters (e.g. contains 0, 1, 8, 9)
        assert!(validate_stellar_address("GA5Z3IX5VQ3N6FB77T342A27RWRN7CKEZ63M3W7S5VJB3D77J6F2JA0K").is_err());
    }

    #[test]
    fn test_validate_asset_code() {
        // Alphanumeric standard
        assert!(validate_asset_code("USDC").is_ok());
        assert!(validate_asset_code("XLM").is_ok());
        assert!(validate_asset_code("EURT").is_ok());

        // Fully qualified
        assert!(validate_asset_code("stellar:USDC:GA5Z3IX5VQ3N6FB77T342A27RWRN7CKEZ63M3W7S5VJB3D77J6F2JAFK").is_ok());
        assert!(validate_asset_code("stellar:USDC:SA5Z3IX5VQ3N6FB77T342A27RWRN7CKEZ63M3W7S5VJB3D77J6F2JAFK").is_err());
        assert!(validate_asset_code("stellar::GA5Z3IX5VQ3N6FB77T342A27RWRN7CKEZ63M3W7S5VJB3D77J6F2JAFK").is_err());

        // ISO-4217 format
        assert!(validate_asset_code("iso4217:USD").is_ok());
        assert!(validate_asset_code("iso4217:NGN").is_ok());
        assert!(validate_asset_code("iso4217:US").is_err());

        // Empty & too long
        assert!(validate_asset_code("").is_err());
        assert!(validate_asset_code("VERYLONGASSETCODE").is_err());
    }
}
