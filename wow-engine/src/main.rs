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

    // 1. Initialize API router with CORS enabled for seamless frontend calls
    let app = wow_engine::api::create_router()
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    // 2. Bind TCP listener on configured port
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
    tracing::info!("     - POST /api/v1/anchor/deposit  (SEP-24 Deposit Anchor / On-ramp)");
    tracing::info!("     - POST /api/v1/anchor/withdraw (SEP-24 Withdraw Anchor / Off-ramp)");
    tracing::info!("     - POST /api/v1/anchor/quote    (SEP-38 Anchor Quotes)");

    // 3. Serve incoming TCP requests through Axum pipeline
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}
