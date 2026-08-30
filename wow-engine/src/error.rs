use axum::{
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use thiserror::Error;

#[derive(Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Bad Request: {0}")]
    BadRequest(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Not Found: {0}")]
    NotFound(String),

    /// A downstream dependency is unavailable or overloaded (e.g. the database
    /// connection pool is exhausted, or a circuit breaker has tripped open).
    ///
    /// Surfaced as `503 Service Unavailable` so clients back off and retry
    /// rather than treating it as a permanent failure. Crucially, the request
    /// fails *fast* instead of hanging until an upstream timeout.
    #[error("Service Unavailable: {0}")]
    ServiceUnavailable(String),

    /// The caller has exceeded its per-IP request budget. Carries the number
    /// of seconds until the caller may retry, surfaced as a `Retry-After`
    /// header rather than a generic Axum default.
    #[error("Too Many Requests")]
    TooManyRequests(u64),

    #[error("Internal Server Error")]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let mut retry_after_secs = None;

        let (status, err_msg) = match self {
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            AppError::Unauthorized(msg) => {
                // Log at debug: signature failures are expected noise from
                // probes/misconfigured callers and should not spam error logs.
                tracing::debug!("Rejected unauthorized request: {msg}");
                (StatusCode::UNAUTHORIZED, "Unauthorized".to_string())
            }
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            AppError::ServiceUnavailable(msg) => {
                tracing::warn!("Service unavailable: {msg}");
                (StatusCode::SERVICE_UNAVAILABLE, msg)
            }
            AppError::TooManyRequests(secs) => {
                retry_after_secs = Some(secs);
                (
                    StatusCode::TOO_MANY_REQUESTS,
                    "Too many requests".to_string(),
                )
            }
            AppError::Internal(err) => {
                tracing::error!("Internal error: {:?}", err);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
        };

        let body = Json(ErrorResponse { error: err_msg });
        let mut response = (status, body).into_response();

        if let Some(secs) = retry_after_secs {
            if let Ok(value) = header::HeaderValue::from_str(&secs.to_string()) {
                response.headers_mut().insert(header::RETRY_AFTER, value);
            }
        }

        response
    }
}
