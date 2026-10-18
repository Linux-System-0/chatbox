//! HTTP error translation.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use chatbox_core::error::Error;
use serde_json::json;

/// A domain error rendered as an HTTP response.
pub struct ApiError(pub Error);

impl From<Error> for ApiError {
    fn from(error: Error) -> Self {
        ApiError(error)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match self.0 {
            Error::NotFound => StatusCode::NOT_FOUND,
            Error::Conflict(_) => StatusCode::CONFLICT,
            Error::Invalid(_) => StatusCode::BAD_REQUEST,
            Error::Unauthorized => StatusCode::UNAUTHORIZED,
            Error::Forbidden(_) => StatusCode::FORBIDDEN,
            Error::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Error::PayloadTooLarge(_) => StatusCode::PAYLOAD_TOO_LARGE,
            Error::Storage(_) | Error::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        if status == StatusCode::INTERNAL_SERVER_ERROR {
            tracing::error!(error = %self.0, "request failed");
        }
        let body = json!({
            "error": { "code": self.0.code(), "message": self.0.to_string() }
        });
        (status, Json(body)).into_response()
    }
}

/// Convenient result alias for handlers.
pub type ApiResult<T> = std::result::Result<T, ApiError>;
