use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt; // for `oneshot`
use wow_engine::anchor::sep38::Sep38Client;
use wow_engine::api::cors::build_cors_layer;
use wow_engine::api::{create_router, create_router_with_cache};
use wow_engine::cache_sync::ClusterCache;
use wow_engine::config::AppConfig;

fn health_request() -> Request<Body> {
    Request::builder()
        .uri("/api/v1/health")
        .body(Body::empty())
        .unwrap()
}

fn quote_request() -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/v1/quote")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            r#"{"source_chain":"Solana","dest_chain":"Ethereum","source_asset":"USDC","dest_asset":"USDC","amount_in":1}"#,
        ))
        .unwrap()
}

fn router_with_config(config: AppConfig) -> axum::Router {
    create_router_with_cache(
        None,
        None,
        None,
        Duration::from_secs(30),
        ClusterCache::local_only(),
        Arc::new(config),
        Arc::new(Sep38Client::new()),
    )
}

#[tokio::test]
async fn test_disallowed_origin_does_not_receive_cors_approval() {
    let config = AppConfig {
        allowed_cors_origins: vec!["https://app.example.com".to_string()],
        ..AppConfig::default()
    };
    let app = create_router(None, None).layer(build_cors_layer(&config));

    let mut request = health_request();
    request
        .headers_mut()
        .insert(header::ORIGIN, "https://evil.example.com".parse().unwrap());

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_none(),
        "a disallowed Origin must not receive Access-Control-Allow-Origin"
    );
}

#[tokio::test]
async fn test_allowed_origin_receives_cors_approval() {
    let config = AppConfig {
        allowed_cors_origins: vec!["https://app.example.com".to_string()],
        ..AppConfig::default()
    };
    let app = create_router(None, None).layer(build_cors_layer(&config));

    let mut request = health_request();
    request
        .headers_mut()
        .insert(header::ORIGIN, "https://app.example.com".parse().unwrap());

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .unwrap(),
        "https://app.example.com"
    );
}

#[tokio::test]
async fn test_no_allowlist_configured_stays_permissive_for_local_dev() {
    // Mirrors production defaults today: an empty allowlist means
    // unrestricted CORS, which is the intended local-development fallback.
    let config = AppConfig::default();
    let app = create_router(None, None).layer(build_cors_layer(&config));

    let mut request = health_request();
    request.headers_mut().insert(
        header::ORIGIN,
        "https://anything.example.com".parse().unwrap(),
    );

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response
        .headers()
        .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
        .is_some());
}

#[tokio::test]
async fn test_quote_endpoint_returns_429_with_retry_after_once_over_budget() {
    let config = AppConfig {
        rate_limit_quote_per_minute: 2,
        rate_limit_global_per_minute: 1_000,
        ..AppConfig::default()
    };
    let app = router_with_config(config);

    for _ in 0..2 {
        let response = app.clone().oneshot(quote_request()).await.unwrap();
        assert_ne!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    let response = app.oneshot(quote_request()).await.unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(
        response.headers().get(header::RETRY_AFTER).is_some(),
        "a 429 must carry a Retry-After header"
    );
}

#[tokio::test]
async fn test_global_rate_limit_covers_routes_without_their_own_budget() {
    let config = AppConfig {
        rate_limit_global_per_minute: 2,
        rate_limit_quote_per_minute: 1_000,
        ..AppConfig::default()
    };
    let app = router_with_config(config);

    for _ in 0..2 {
        let response = app.clone().oneshot(health_request()).await.unwrap();
        assert_ne!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    let response = app.oneshot(health_request()).await.unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn test_requests_under_budget_are_never_rate_limited() {
    let config = AppConfig {
        rate_limit_quote_per_minute: 5,
        rate_limit_global_per_minute: 1_000,
        ..AppConfig::default()
    };
    let app = router_with_config(config);

    for _ in 0..5 {
        let response = app.clone().oneshot(quote_request()).await.unwrap();
        assert_ne!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    }
}
