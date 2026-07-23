use std::net::SocketAddr;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

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

    // 1. Connect to the database (required for route execution/quota tracking)
    let db = match config.get_database_url() {
        Ok(url) => match wow_engine::db::Database::new(&url).await {
            Ok(db) => {
                // Apply any pending schema migrations before serving traffic.
                // This ensures the database schema always matches the binary's expectations,
                // eliminating configuration drift across environments.
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

    // 3. Initialize API router with CORS enabled for seamless frontend calls
    let app = wow_engine::api::create_router(db, verifier)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    // 4. Bind TCP listener on configured port
    let port = config.port;
    // Bind all container interfaces so the published Docker port can reach the
    // service. The container still runs as the unprivileged `nonroot` user.
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;

    tracing::info!("Wow Engine is booting up and routing pipeline conversions...");
    tracing::info!("   Listening on: http://{}", addr);
    tracing::info!("   Endpoints available:");
    tracing::info!("     - GET  /api/v1/health          (Health Check)");
    tracing::info!("     - POST /api/v1/quote           (Quoting Pathfinder)");
    tracing::info!("     - POST /api/v1/execute-route   (Atomic Route Execution)");
    tracing::info!("     - POST /api/v1/anchor/deposit  (SEP-24 Deposit Anchor / On-ramp)");
    tracing::info!("     - POST /api/v1/anchor/withdraw (SEP-24 Withdraw Anchor / Off-ramp)");
    tracing::info!("     - POST /api/v1/anchor/quote    (SEP-38 Anchor Quotes)");

    // 5. Serve incoming TCP requests through Axum pipeline
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}
