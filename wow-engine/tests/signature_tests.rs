//! End-to-end tests for the Ed25519 request-signature middleware.
//!
//! These drive a real Axum server (via `axum-test`) with verification enabled
//! and assert the acceptance criteria from issue #20:
//!   * missing / tampered / stale signatures -> 401 Unauthorized
//!   * a valid signature reaches the handler (here proven by the handler's own
//!     400 validation error, i.e. it got past auth)
//!   * public endpoints (health, quote) bypass verification entirely

use axum::http::{HeaderName, HeaderValue, StatusCode};
use axum_test::TestServer;
use ed25519_dalek::{Signer, SigningKey};
use serde_json::json;
use wow_engine::api::auth::{canonical_message, sha256_hex, SignatureVerifier};
use wow_engine::api::auth::{SIGNATURE_HEADER, TIMESTAMP_HEADER};
use wow_engine::api::create_router;

/// Deterministic keypair + matching verifier for hermetic tests.
fn keypair() -> (SigningKey, SignatureVerifier) {
    let signing_key = SigningKey::from_bytes(&[42u8; 32]);
    let public_key_hex = hex::encode(signing_key.verifying_key().to_bytes());
    let verifier = SignatureVerifier::from_hex_public_key(&public_key_hex).unwrap();
    (signing_key, verifier)
}

fn server_with_verification() -> (SigningKey, TestServer) {
    let (signing_key, verifier) = keypair();
    let app = create_router(None, Some(verifier));
    (signing_key, TestServer::new(app).unwrap())
}

/// Signs a request and returns the (X-Signature, X-Timestamp) header pair.
fn sign(
    signing_key: &SigningKey,
    method: &str,
    path: &str,
    timestamp: i64,
    body: &[u8],
) -> (HeaderName, HeaderValue, HeaderName, HeaderValue) {
    let ts = timestamp.to_string();
    let message = canonical_message(method, path, &ts, &sha256_hex(body));
    let signature = hex::encode(signing_key.sign(message.as_bytes()).to_bytes());
    (
        HeaderName::from_static(SIGNATURE_HEADER),
        HeaderValue::from_str(&signature).unwrap(),
        HeaderName::from_static(TIMESTAMP_HEADER),
        HeaderValue::from_str(&ts).unwrap(),
    )
}

// A payload that passes JSON parsing but fails handler validation, so a
// successful auth is observable as a 400 (not a 401).
fn anchor_quote_payload() -> serde_json::Value {
    json!({
        "anchor_domain": "test.com",
        "sell_asset": "USDC",
        "buy_asset": "NGN",
        "sell_amount": -100.0
    })
}

const PROTECTED_PATH: &str = "/api/v1/anchor/quote";

#[tokio::test]
async fn valid_signature_passes_the_middleware() {
    let (signing_key, server) = server_with_verification();
    let payload = anchor_quote_payload();
    let body = serde_json::to_vec(&payload).unwrap();
    let ts = chrono::Utc::now().timestamp();
    let (sn, sv, tn, tv) = sign(&signing_key, "POST", PROTECTED_PATH, ts, &body);

    let response = server
        .post(PROTECTED_PATH)
        .json(&payload)
        .add_header(sn, sv)
        .add_header(tn, tv)
        .await;

    // Got past auth and into the handler, which rejects the negative amount.
    response.assert_status_bad_request();
    assert!(response
        .text()
        .contains("Sell amount must be greater than zero"));
}

#[tokio::test]
async fn missing_signature_is_rejected() {
    let (_signing_key, server) = server_with_verification();

    let response = server
        .post(PROTECTED_PATH)
        .json(&anchor_quote_payload())
        .await;

    response.assert_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn tampered_body_is_rejected() {
    let (signing_key, server) = server_with_verification();

    // Sign over the original body...
    let original = anchor_quote_payload();
    let signed_body = serde_json::to_vec(&original).unwrap();
    let ts = chrono::Utc::now().timestamp();
    let (sn, sv, tn, tv) = sign(&signing_key, "POST", PROTECTED_PATH, ts, &signed_body);

    // ...but send a different body under the same signature.
    let tampered = json!({
        "anchor_domain": "attacker.com",
        "sell_asset": "USDC",
        "buy_asset": "NGN",
        "sell_amount": -100.0
    });

    let response = server
        .post(PROTECTED_PATH)
        .json(&tampered)
        .add_header(sn, sv)
        .add_header(tn, tv)
        .await;

    response.assert_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn stale_timestamp_is_rejected_as_replay() {
    let (signing_key, server) = server_with_verification();
    let payload = anchor_quote_payload();
    let body = serde_json::to_vec(&payload).unwrap();

    // Properly signed, but the timestamp is an hour old -> outside the window.
    let stale = chrono::Utc::now().timestamp() - 3600;
    let (sn, sv, tn, tv) = sign(&signing_key, "POST", PROTECTED_PATH, stale, &body);

    let response = server
        .post(PROTECTED_PATH)
        .json(&payload)
        .add_header(sn, sv)
        .add_header(tn, tv)
        .await;

    response.assert_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn wrong_key_signature_is_rejected() {
    let (_trusted_key, server) = server_with_verification();
    // An attacker signs with a key the engine does not trust.
    let attacker_key = SigningKey::from_bytes(&[9u8; 32]);
    let payload = anchor_quote_payload();
    let body = serde_json::to_vec(&payload).unwrap();
    let ts = chrono::Utc::now().timestamp();
    let (sn, sv, tn, tv) = sign(&attacker_key, "POST", PROTECTED_PATH, ts, &body);

    let response = server
        .post(PROTECTED_PATH)
        .json(&payload)
        .add_header(sn, sv)
        .add_header(tn, tv)
        .await;

    response.assert_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn public_endpoints_bypass_verification() {
    let (_signing_key, server) = server_with_verification();

    // Health: no signature headers at all, still succeeds.
    server.get("/api/v1/health").await.assert_status_ok();

    // Quote: unsigned, reaches the handler which rejects the zero amount (400,
    // not 401) — proving the signature layer was bypassed.
    let response = server
        .post("/api/v1/quote")
        .json(&json!({
            "source_chain": "Solana",
            "dest_chain": "Ethereum",
            "source_asset": "USDC",
            "dest_asset": "USDC",
            "amount_in": 0
        }))
        .await;
    response.assert_status_bad_request();
}
