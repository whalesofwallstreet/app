//! CORS origin allowlisting.
//!
//! `create_router`/`main` must only grant CORS approval to known frontend
//! origins (web-app, native-app in dev/staging/prod), never reflect
//! `Access-Control-Allow-Origin` for an arbitrary requesting origin.

use crate::config::AppConfig;
use tower_http::cors::{Any, CorsLayer};

/// Builds the CORS layer from [`AppConfig::allowed_cors_origins`].
///
/// When the allowlist is empty (local development with no explicit
/// configuration), CORS is fully permissive. Otherwise only the configured
/// origins receive `Access-Control-Allow-Origin` approval.
pub fn build_cors_layer(config: &AppConfig) -> CorsLayer {
    if config.allowed_cors_origins.is_empty() {
        return CorsLayer::permissive();
    }

    let origins: Vec<_> = config
        .allowed_cors_origins
        .iter()
        .filter_map(|o| o.parse().ok())
        .collect();

    CorsLayer::new()
        .allow_origin(origins)
        .allow_methods(Any)
        .allow_headers(Any)
}
