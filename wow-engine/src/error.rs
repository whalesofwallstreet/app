use axum::{
    http::StatusCode,
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

    #[error("Internal Server Error")]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, err_msg) = match self {
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            AppError::Unauthorized(msg) => {
                // Log at debug: signature failures are expected noise from
                // probes/misconfigured callers and should not spam error logs.
                tracing::debug!("Rejected unauthorized request: {msg}");
                (StatusCode::UNAUTHORIZED, "Unauthorized".to_string())
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
        (status, body).into_response()
    }
}
