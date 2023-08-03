use axum::{
    body,
    response::{IntoResponse, Response},
};
use http::StatusCode;

pub struct ErrorPage {
    pub status_code: StatusCode,
    pub message: String,
}

impl ErrorPage {
    // pub fn not_found(message: impl Into<String>) -> Self {
    //     ErrorPage {
    //         status_code: StatusCode::NOT_FOUND,
    //         message: message.into(),
    //     }
    // }

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

fn create_html(status_code: StatusCode, message: &str) -> String {
    format!(
        r#"
<!DOCTYPE html>
<html>
    <head>
        <title>{status_code_name}</title>
        <style>
            body {{
                font-family: sans-serif;
                display: flex;
                justify-content: center;
                align-items: center;
                height: 100vh;
                margin: 0;
            }}
            div {{
                width: 100vw;
                max-width: 600px;
                text-align: center;
            }}
            h1 {{
                font-size: 2.5em;
            }}
        </style>
    </head>
    <body>
        <div>
            <h1>{status_code} {status_code_name}</h1>
            <hr />
            <p>{message}</p>
        </div>
    </body>
</html>
"#,
        status_code = status_code.as_u16(),
        status_code_name = status_code.canonical_reason().unwrap_or(""),
    )
}

impl IntoResponse for ErrorPage {
    fn into_response(self) -> Response {
        let body = create_html(self.status_code, &self.message);
        Response::builder()
            .status(self.status_code)
            .header("Content-Type", "text/html; charset=utf-8")
            .body(body::boxed(body))
            .expect("failed to render error page")
    }
}
