use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::trace::TraceLayer;
use wow_engine::api::cors::build_cors_layer;

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("Shutdown signal received, starting graceful shutdown");
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing subscriber
    tracing_subscriber::fmt::init();

    // Load environment variables from .env file
    dotenvy::dotenv().ok();

    // Load strongly-typed configuration
    let config = wow_engine::config::AppConfig::load()?;
    let config = Arc::new(config);

    tracing::info!("Configuration loaded successfully");

    // 1. Connect to the database (required for route execution/quota tracking)
    let db = match config.get_database_url() {
        Ok(url) => match wow_engine::db::Database::new(&url).await {
            Ok(db) => {
                // Apply any pending schema migrations before serving traffic.
                tracing::info!("Running pending database migrations...");
                if let Err(err) = db.run_migrations().await {
                    tracing::error!("Fatal: failed to apply database migrations: {err}");
                    return Err(err.into());
                }
                tracing::info!("Database migrations applied successfully.");
                Some(db)
            }
            Err(err) => {
                tracing::warn!("Failed to connect to database: {err}. /api/v1/execute-route will be unavailable.");
                None
            }
        },
        Err(err) => {
            tracing::warn!("{err}. /api/v1/execute-route will be unavailable.");
            None
        }
    };

    let tracker = db
        .clone()
        .map(|d| Arc::new(wow_engine::anchor::tracker::TrackerStore::new(d)));

    // 2. Build the Ed25519 signature verifier for internal service-to-service
    //    calls. When no key is configured we run with verification DISABLED and
    //    warn loudly — acceptable for local dev, never for production.
    let verifier = match config.signing_public_key.as_deref() {
        Some(key) => {
            let verifier = wow_engine::api::auth::SignatureVerifier::from_hex_public_key(key)?;
            tracing::info!("Ed25519 request-signature verification ENABLED for internal endpoints");
            Some(verifier)
        }
        None => {
            tracing::warn!(
                "SIGNING_PUBLIC_KEY not set: internal request-signature verification is DISABLED. \
                 Protected endpoints are unauthenticated. Do NOT run this way in production."
            );
            None
        }
    };

    // 3. Build the shared, cluster-aware cache. A single long-lived
    //    `GasOracle` is shared across every request on this node (instead of
    //    each request building its own throwaway one), and — if REDIS_URL is
    //    set — a broadcaster that publishes invalidation events to every
    //    other node.
    let gas_oracle = std::sync::Arc::new(wow_engine::bridge::gas_oracle::GasOracle::new(
        config.clone(),
    ));
    let redis_broadcaster = match &config.redis_url {
        Some(url) => match redis::Client::open(url.as_str()) {
            Ok(client) => match client.get_connection_manager().await {
                Ok(manager) => {
                    tracing::info!("Connected to Redis for cluster-wide cache invalidation");
                    Some(std::sync::Arc::new(
                        wow_engine::cache_sync::CacheInvalidationBroadcaster::new(
                            Some(manager),
                            wow_engine::cache_sync::CACHE_INVALIDATION_CHANNEL,
                        ),
                    ))
                }
                Err(err) => {
                    tracing::warn!(
                        "Failed to connect to Redis ({err}); running with local TTL-only caching"
                    );
                    None
                }
            },
            Err(err) => {
                tracing::warn!("Invalid REDIS_URL ({err}); running with local TTL-only caching");
                None
            }
        },
        None => {
            tracing::info!(
                "REDIS_URL not set; running with local TTL-only caching (single-node behavior)"
            );
            None
        }
    };
    let cluster_cache = wow_engine::cache_sync::ClusterCache {
        gas_oracle: gas_oracle.clone(),
        broadcaster: redis_broadcaster,
    };

    // Background task: keeps this node's cache in sync with cluster-wide
    // invalidation events.
    tokio::spawn(wow_engine::cache_sync::run_redis_subscriber(
        config.redis_url.clone(),
        gas_oracle,
    ));

    // Background task: prunes stale `historical_routes` entries so the
    // archive table doesn't grow unbounded. No-op (and no-DB) deployments
    // simply don't get a GC worker.
    if let Some(db) = db.clone() {
        tokio::spawn(wow_engine::db::gc::run_historical_routes_gc(db));
    }

    // Background task: periodically scans the liquidity graph for
    // arbitrage (negative-weight cycle) opportunities. Entirely read-only
    // and isolated from the request-serving path, so it never adds latency
    // to a live quote.
    let arbitrage_planner =
        std::sync::Arc::new(wow_engine::router::RoutePlanner::new(config.clone()));
    tokio::spawn(wow_engine::router::arbitrage::run_arbitrage_scanner(
        arbitrage_planner,
    ));

    // 4. Initialize API router with CORS and configuration.
    let request_timeout = std::time::Duration::from_secs(config.request_timeout_secs);
    let cors_layer = build_cors_layer(&config);
    let sep38_client = std::sync::Arc::new(wow_engine::anchor::sep38::Sep38Client::new());
    let app = wow_engine::api::create_router_with_cache(
        db,
        verifier,
        tracker,
        request_timeout,
        cluster_cache,
        config.clone(),
        sep38_client,
    )
    .layer(cors_layer)
    .layer(TraceLayer::new_for_http());

    // 5. Bind TCP listener on configured port
    let port = config.port;
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;

    tracing::info!("Wow Engine is booting up and routing pipeline conversions...");
    tracing::info!("   Listening on: http://{}", addr);
    tracing::info!("   Endpoints available:");
    tracing::info!("     - GET  /api/v1/health              (Health Check)");
    tracing::info!("     - POST /api/v1/quote               (Quoting Pathfinder)");
    tracing::info!("     - POST /api/v1/execute-route       (Atomic Route Execution)");
    tracing::info!("     - POST /api/v1/anchor/deposit      (SEP-24 Deposit Anchor / On-ramp)");
    tracing::info!("     - POST /api/v1/anchor/withdraw     (SEP-24 Withdraw Anchor / Off-ramp)");
    tracing::info!("     - POST /api/v1/anchor/quote        (SEP-38 Anchor Quotes)");
    tracing::info!("     - POST /api/v1/admin/invalidate-cache (Cluster Cache Invalidation)");

    // 6. Serve incoming TCP requests through Axum pipeline. Connect-info is
    //    threaded through so rate limiting can key on the real peer IP.
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;
    Ok(())
}
