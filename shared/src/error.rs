use axum::{http::StatusCode, response::IntoResponse};
use serde_json::json;

pub struct ErrorResponse {
    pub status_code: StatusCode,
    pub message: String,
}

impl ErrorResponse {
    pub fn new(status_code: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status_code,
            message: message.into(),
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, message)
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }

    pub fn bad_gateway(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_GATEWAY, message)
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, message)
    }
}

impl<T> From<ErrorResponse> for Result<T, ErrorResponse> {
    fn from(value: ErrorResponse) -> Self {
        Err(value)
    }
}

impl IntoResponse for ErrorResponse {
    fn into_response(self) -> axum::response::Response {
        let body = axum::response::Json(json!({
            "type": "error",
            "code": self.status_code.as_str(),
            "status": self.status_code.as_u16(),
            "message": self.message,
        }));

        (self.status_code, body).into_response()
    }
}
