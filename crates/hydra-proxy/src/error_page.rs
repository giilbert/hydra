use axum::{
    body,
    response::{IntoResponse, Response},
};
use hyper::StatusCode;

pub struct ErrorPage {
    pub status_code: StatusCode,
    pub message: String,
}

impl ErrorPage {
    pub fn not_found(message: impl Into<String>) -> Self {
        ErrorPage {
            status_code: StatusCode::NOT_FOUND,
            message: message.into(),
        }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        ErrorPage {
            status_code: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    pub fn bad_gateway(message: impl Into<String>) -> Self {
        ErrorPage {
            status_code: StatusCode::BAD_GATEWAY,
            message: message.into(),
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        ErrorPage {
            status_code: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
        }
    }
}

impl IntoResponse for ErrorPage {
    fn into_response(self) -> Response {
        let body = format!(
            r#"
            <html>
                <head>
                    <title>{status_code}</title>
                </head>
                <body>
                    <h1>{status_code}</h1>
                    <p>{message}</p>
                </body>
            </html>
            "#,
            status_code = self.status_code.as_u16(),
            message = self.message,
        );

        Response::builder()
            .status(self.status_code)
            .header("Content-Type", "text/html; charset=utf-8")
            .body(body::boxed(body))
            .unwrap()
    }
}
