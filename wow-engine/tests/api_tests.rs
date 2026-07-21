use axum_test::TestServer;
use serde_json::json;
use wow_engine::api::create_router;

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
