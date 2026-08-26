use axum_test::TestServer;
use serde_json::json;
use wow_engine::api::{create_router, create_router_with_cache};

#[tokio::test]
async fn test_health_endpoint() {
    let app = create_router(None, None);
    let server = TestServer::new(app).unwrap();

    let response = server.get("/api/v1/health").await;
    response.assert_status_ok();

    let health: serde_json::Value = response.json();
    assert_eq!(health["status"], "ok");
    assert_eq!(health["service"], "wow-engine");
}

#[tokio::test]
async fn test_quote_endpoint_bad_request() {
    let app = create_router(None, None);
    let server = TestServer::new(app).unwrap();

    // 0 amount should trigger a validation error
    let payload = json!({
        "source_chain": "Solana",
        "dest_chain": "Ethereum",
        "source_asset": "USDC",
        "dest_asset": "USDC",
        "amount_in": 0
    });

    let response = server.post("/api/v1/quote").json(&payload).await;
    response.assert_status_bad_request();

    let err_msg = response.text();
    assert!(err_msg.contains("Amount in must be greater than zero"));
}

#[tokio::test]
async fn test_verify_attestation_endpoint_rejects_invalid_hex() {
    let app = create_router(None, None);
    let server = TestServer::new(app).unwrap();

    let payload = json!({
        "dest_chain": "Arbitrum",
        "message": "not-hex",
        "attestation": "0xdeadbeef"
    });

    let response = server
        .post("/api/v1/cctp/verify-attestation")
        .json(&payload)
        .await;
    response.assert_status_bad_request();
    assert!(response.text().contains("Invalid hex in message"));
}

#[tokio::test]
async fn test_verify_attestation_endpoint_rejects_malformed_attestation() {
    let app = create_router(None, None);
    let server = TestServer::new(app).unwrap();

    // Structurally invalid: 64 bytes is not a whole number of 65-byte
    // signatures. Rejected synchronously, before any key fetch.
    let payload = json!({
        "dest_chain": "Arbitrum",
        "message": "0x00",
        "attestation": format!("0x{}", "ab".repeat(64))
    });

    let response = server
        .post("/api/v1/cctp/verify-attestation")
        .json(&payload)
        .await;
    response.assert_status_bad_request();

    let err_msg = response.text();
    assert!(err_msg.contains("Attestation rejected"));
    assert!(err_msg.contains("malformed attestation"));
}

#[tokio::test]
async fn test_quote_endpoint_exposes_dynamic_slippage() {
    let app = create_router(None, None);
    let server = TestServer::new(app).unwrap();

    let payload = json!({
        "source_chain": "Ethereum",
        "dest_chain": "Ethereum",
        "source_asset": "ETH",
        "dest_asset": "USDC",
        "amount_in": 10
    });

    let response = server.post("/api/v1/quote").json(&payload).await;
    response.assert_status_ok();

    let body: serde_json::Value = response.json();
    let route = &body["routes"][0];
    assert!(
        route["slippage_bps"].is_u64(),
        "route must expose the dynamic slippage tolerance"
    );
    assert!(
        route["price_impact_bps"].is_u64(),
        "route must expose the computed price impact"
    );
    assert!(route["slippage_bps"].as_u64().unwrap() > 0);
}

#[tokio::test]
async fn test_quote_endpoint_rejects_catastrophic_price_impact() {
    let app = create_router(None, None);
    let server = TestServer::new(app).unwrap();

    // ~$180M of ETH exceeds the 15% price-impact ceiling on every pool.
    let payload = json!({
        "source_chain": "Ethereum",
        "dest_chain": "Ethereum",
        "source_asset": "ETH",
        "dest_asset": "USDC",
        "amount_in": 60000
    });

    let response = server.post("/api/v1/quote").json(&payload).await;
    response.assert_status_bad_request();

    let err_msg = response.text();
    assert!(err_msg.contains("price impact"));
    assert!(err_msg.contains("exceeds the maximum"));
}

#[tokio::test]
async fn test_deposit_endpoint_invalid_address() {
    let app = create_router(None, None);
    let server = TestServer::new(app).unwrap();

    let payload = json!({
        "anchor_domain": "test.com",
        "asset_code": "USDC",
        "account": "INVALID_ADDRESS"
    });

    let response = server.post("/api/v1/anchor/deposit").json(&payload).await;
    response.assert_status_bad_request();

    let err_msg = response.text();
    assert!(err_msg.contains("Invalid account address"));
}

#[tokio::test]
async fn test_admin_invalidate_cache_endpoint_invalidates_specific_chain() {
    let app = create_router(None, None);
    let server = TestServer::new(app).unwrap();

    let response = server
        .post("/api/v1/admin/invalidate-cache")
        .json(&json!({ "chain": "Ethereum" }))
        .await;
    response.assert_status_ok();

    let body: serde_json::Value = response.json();
    assert_eq!(body["invalidated"], "chain");
    // No REDIS_URL configured in this router, so the broadcast must be
    // reported as skipped rather than silently pretending to succeed.
    assert_eq!(body["broadcast"], false);
}

#[tokio::test]
async fn test_admin_invalidate_cache_endpoint_invalidates_all_when_chain_omitted() {
    let app = create_router(None, None);
    let server = TestServer::new(app).unwrap();

    let response = server
        .post("/api/v1/admin/invalidate-cache")
        .json(&json!({}))
        .await;
    response.assert_status_ok();

    let body: serde_json::Value = response.json();
    assert_eq!(body["invalidated"], "all");
    assert_eq!(body["broadcast"], false);
}

#[tokio::test]
async fn test_admin_invalidate_cache_endpoint_rejects_unknown_chain() {
    let app = create_router(None, None);
    let server = TestServer::new(app).unwrap();

    let response = server
        .post("/api/v1/admin/invalidate-cache")
        .json(&json!({ "chain": "Bitcoin" }))
        .await;
    // Axum's `Json` extractor rejects an undeserializable body before the
    // handler ever runs, surfacing as 422 (not our handler's own 400s).
    response.assert_status(axum::http::StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn test_anchor_quote_invalid_amount() {
    let app = create_router(None, None);
    let server = TestServer::new(app).unwrap();

    let payload = json!({
        "anchor_domain": "test.com",
        "sell_asset": "USDC",
        "buy_asset": "NGN",
        "sell_amount": -100.0
    });

    let response = server.post("/api/v1/anchor/quote").json(&payload).await;
    response.assert_status_bad_request();

    let err_msg = response.text();
    assert!(err_msg.contains("Sell amount must be greater than zero"));
}

#[tokio::test]
async fn test_anchor_quote_endpoint_success() {
    // Sep38Client is stateless (no DB dependency), so this success path
    // runs unconditionally, unlike the deposit/withdraw success tests below.
    //
    // The handler now sources its price from a live call to the anchor's
    // SEP-38 `/price` endpoint (see issue #51), so this points the request
    // at a mock anchor rather than asserting a hardcoded literal.
    let mock_anchor = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/sep38/price"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "price": "1500.0000000" })),
        )
        .mount(&mock_anchor)
        .await;

    let app = create_router_with_cache(
        None,
        None,
        None,
        std::time::Duration::from_secs(30),
        wow_engine::cache_sync::ClusterCache::local_only(),
        std::sync::Arc::new(wow_engine::config::AppConfig::default()),
        std::sync::Arc::new(wow_engine::anchor::sep38::Sep38Client::new()),
    );
    let server = TestServer::new(app).unwrap();

    let payload = json!({
        "anchor_domain": mock_anchor.uri(),
        "sell_asset": "USDC",
        "buy_asset": "NGN",
        "sell_amount": 100.0
    });

    let response = server.post("/api/v1/anchor/quote").json(&payload).await;
    response.assert_status_ok();

    let body: serde_json::Value = response.json();
    assert_eq!(body["sell_asset"], "USDC");
    assert_eq!(body["buy_asset"], "NGN");
    assert_eq!(body["price"], "1500.0000000");
    assert_eq!(body["buy_amount"], "150000.0000000");
    assert!(body["id"].as_str().unwrap().starts_with("q_sep38_"));
}

/// Success-path tests for the anchor deposit/withdraw endpoints. These
/// require a live Postgres instance to back the `TrackerStore` (see
/// `tests/transaction_atomicity_tests.rs` for the same convention) and are
/// skipped by default since CI's `cargo test` doesn't pass
/// `--include-ignored`.
mod anchor_deposit_withdraw_success {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;
    use wow_engine::anchor::sep38::Sep38Client;
    use wow_engine::anchor::tracker::TrackerStore;
    use wow_engine::cache_sync::ClusterCache;
    use wow_engine::config::AppConfig;
    use wow_engine::db::Database;

    async fn app_with_tracker() -> Option<axum::Router> {
        let database_url = std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| {
            "postgres://postgres:postgres@localhost/wow_engine_test".to_string()
        });

        let db = match Database::new(&database_url).await {
            Ok(db) => db,
            Err(e) => {
                eprintln!("Skipping test: {}", e);
                return None;
            }
        };
        db.run_migrations().await.ok();

        let tracker = Arc::new(TrackerStore::new(db.clone()));

        Some(create_router_with_cache(
            Some(db),
            None,
            Some(tracker),
            Duration::from_secs(30),
            ClusterCache::local_only(),
            Arc::new(AppConfig::default()),
            Arc::new(Sep38Client::new()),
        ))
    }

    #[tokio::test]
    #[ignore]
    async fn test_deposit_endpoint_success() {
        let Some(app) = app_with_tracker().await else {
            return;
        };
        let server = TestServer::new(app).unwrap();

        let payload = json!({
            "anchor_domain": "test.com",
            "asset_code": "USDC",
            "account": "GA5Z3IX5VQ3N6FB77T342A27RWRN7CKEZ63M3W7S5VJB3D77J6F2JAFK"
        });

        let response = server.post("/api/v1/anchor/deposit").json(&payload).await;
        response.assert_status_ok();

        let body: serde_json::Value = response.json();
        assert_eq!(body["type"], "interactive_customer_info_needed");
        assert!(body["url"]
            .as_str()
            .unwrap()
            .contains("/sep24/interactive/deposit"));
        assert!(body["id"].as_str().unwrap().starts_with("tx_sep24_"));
    }

    #[tokio::test]
    #[ignore]
    async fn test_withdraw_endpoint_success() {
        let Some(app) = app_with_tracker().await else {
            return;
        };
        let server = TestServer::new(app).unwrap();

        let payload = json!({
            "anchor_domain": "test.com",
            "asset_code": "USDC",
            "account": "GA5Z3IX5VQ3N6FB77T342A27RWRN7CKEZ63M3W7S5VJB3D77J6F2JAFK"
        });

        let response = server.post("/api/v1/anchor/withdraw").json(&payload).await;
        response.assert_status_ok();

        let body: serde_json::Value = response.json();
        assert_eq!(body["type"], "interactive_customer_info_needed");
        assert!(body["url"]
            .as_str()
            .unwrap()
            .contains("/sep24/interactive/withdraw"));
    }
}
