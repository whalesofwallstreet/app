use crate::anchor::{sep24::Sep24Client, sep38::Sep38Client, Sep24InteractiveResponse, Sep38Quote};
use crate::db::models::RouteExecutionInput;
use crate::db::service::{ExecuteRouteResult, RouteExecutionService};
use crate::db::Database;
use crate::error::AppError;
use crate::router::{RouteOption, RoutePlanner};
use axum::{
    routing::{get, post},
    Extension, Json, Router,
};
use serde::Serialize;

pub mod validated_request;
pub mod validation;

use validated_request::{
    ValidatedAnchorQuoteRequest, ValidatedDepositRequest, ValidatedExecuteRouteRequest,
    ValidatedQuoteRequest, ValidatedWithdrawRequest,
};

#[derive(Serialize)]
pub struct QuoteResponse {
    pub routes: Vec<RouteOption>,
}

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub service: &'static str,
    pub version: &'static str,
    pub timestamp: String,
}

pub fn create_router(db: Option<Database>) -> Router {
    Router::new()
        .route("/api/v1/health", get(health_handler))
        .route("/api/v1/quote", post(quote_handler))
        .route("/api/v1/execute-route", post(execute_route_handler))
        .route("/api/v1/anchor/deposit", post(deposit_handler))
        .route("/api/v1/anchor/withdraw", post(withdraw_handler))
        .route("/api/v1/anchor/quote", post(anchor_quote_handler))
        .layer(Extension(db))
}

async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "wow-engine",
        version: "0.1.0",
        timestamp: chrono::Utc::now().to_rfc3339(),
    })
}

#[tracing::instrument(err)]
async fn quote_handler(
    ValidatedQuoteRequest {
        source_chain,
        dest_chain,
        source_asset,
        dest_asset,
        amount_in,
    }: ValidatedQuoteRequest,
) -> Result<Json<QuoteResponse>, AppError> {
    let planner = RoutePlanner::new();
    let routes = planner
        .find_best_route(
            source_chain,
            dest_chain,
            &source_asset,
            &dest_asset,
            amount_in,
        )
        .await?;
    Ok(Json(QuoteResponse { routes }))
}

#[tracing::instrument(err)]
async fn deposit_handler(
    ValidatedDepositRequest {
        anchor_domain,
        asset_code,
        account,
    }: ValidatedDepositRequest,
) -> Result<Json<Sep24InteractiveResponse>, AppError> {
    let client = Sep24Client::new();
    let tx = client
        .initiate_deposit(&anchor_domain, &asset_code, &account)
        .await?;
    Ok(Json(tx))
}

#[tracing::instrument(err)]
async fn withdraw_handler(
    ValidatedWithdrawRequest {
        anchor_domain,
        asset_code,
        account,
    }: ValidatedWithdrawRequest,
) -> Result<Json<Sep24InteractiveResponse>, AppError> {
    let client = Sep24Client::new();
    let tx = client
        .initiate_withdrawal(&anchor_domain, &asset_code, &account)
        .await?;
    Ok(Json(tx))
}

#[tracing::instrument(err)]
async fn anchor_quote_handler(
    ValidatedAnchorQuoteRequest {
        anchor_domain,
        sell_asset,
        buy_asset,
        sell_amount,
    }: ValidatedAnchorQuoteRequest,
) -> Result<Json<Sep38Quote>, AppError> {
    let client = Sep38Client::new();
    let quote = client
        .get_indicative_quote(&anchor_domain, &sell_asset, &buy_asset, sell_amount)
        .await?;
    Ok(Json(quote))
}

#[tracing::instrument(skip(db), err)]
async fn execute_route_handler(
    Extension(db): Extension<Option<Database>>,
    ValidatedExecuteRouteRequest {
        user_id,
        source_chain,
        dest_chain,
        source_asset,
        dest_asset,
        amount_in,
        amount_out,
        provider,
        path,
        estimated_fee_usd,
        anchor_domain,
        anchor_transaction_id,
    }: ValidatedExecuteRouteRequest,
) -> Result<Json<ExecuteRouteResult>, AppError> {
    let db = db.ok_or_else(|| {
        AppError::Internal(anyhow::anyhow!(
            "Database not configured for this server instance"
        ))
    })?;

    let route_input = RouteExecutionInput {
        user_id,
        source_chain: source_chain.to_string(),
        dest_chain: dest_chain.to_string(),
        source_asset,
        dest_asset,
        amount_in: amount_in as i64,
        amount_out: amount_out as i64,
        provider,
        path,
        estimated_fee_usd,
    };

    let result = RouteExecutionService::execute_route_with_quota(
        &db,
        route_input,
        anchor_domain.as_deref(),
        anchor_transaction_id.as_deref(),
    )
    .await
    .map_err(|e| AppError::BadRequest(format!("Route execution failed: {}", e)))?;

    Ok(Json(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::validation::{validate_asset_code, validate_stellar_address};

    #[test]
    fn test_validate_stellar_address() {
        assert!(validate_stellar_address(
            "GA5Z3IX5VQ3N6FB77T342A27RWRN7CKEZ63M3W7S5VJB3D77J6F2JAFK"
        )
        .is_ok());

        assert!(validate_stellar_address(
            "SA5Z3IX5VQ3N6FB77T342A27RWRN7CKEZ63M3W7S5VJB3D77J6F2JAFK"
        )
        .is_err());

        assert!(validate_stellar_address("GA5Z3IX5").is_err());

        assert!(validate_stellar_address(
            "GA5Z3IX5VQ3N6FB77T342A27RWRN7CKEZ63M3W7S5VJB3D77J6F2JA0K"
        )
        .is_err());
    }

    #[test]
    fn test_validate_asset_code() {
        assert!(validate_asset_code("USDC").is_ok());
        assert!(validate_asset_code("XLM").is_ok());
        assert!(validate_asset_code("EURT").is_ok());

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

        assert!(validate_asset_code("iso4217:USD").is_ok());
        assert!(validate_asset_code("iso4217:NGN").is_ok());
        assert!(validate_asset_code("iso4217:US").is_err());

        assert!(validate_asset_code("").is_err());
        assert!(validate_asset_code("VERYLONGASSETCODE").is_err());
    }
}
