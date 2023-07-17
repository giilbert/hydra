use axum::{
    async_trait,
    body::Bytes,
    extract::{FromRequestParts, Host},
    http::{request::Parts, HeaderMap, HeaderValue, Method, StatusCode, Uri},
    response::IntoResponse,
};
use reqwest::Response;

#[axum::debug_handler]
pub async fn handler(
    ExtractMethod(method): ExtractMethod,
    uri: Uri,
    Host(host): Host,
    mut headers: HeaderMap,
    body: Bytes,
) -> Result<impl IntoResponse, StatusCode> {
    let is_websocket_upgrade = headers
        .get("Upgrade")
        .map(|v| v == "websocket")
        .unwrap_or(false);

    if is_websocket_upgrade {
        unimplemented!("websocket proxying is not yet implemented");
    }

    let hydra_server_url = "http://localhost:3100";
    let proxy_url = format!("{hydra_server_url}/proxy");

    headers.insert(
        "X-Hydra-URI",
        HeaderValue::from_str(&uri.to_string()).map_err(|_| StatusCode::BAD_REQUEST)?,
    );

    let client = reqwest::Client::new();
    let request = client
        .request(method, proxy_url)
        .headers(headers)
        .body(body)
        .build()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let response = client
        .execute(request)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut response_headers = response.headers().clone();
    // TODO: check that the content length is below BODY_LIMIT
    let status_code = response.status();
    let response_bytes = response.bytes().await.map_err(|e| {
        log::error!("error decoding response body: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    response_headers.insert(
        "X-Forwarded-By",
        HeaderValue::from_str("hydra-proxy").unwrap(),
    );

    Ok((status_code, response_headers, response_bytes))
}

pub struct ExtractMethod(pub Method);

#[async_trait]
impl<S> FromRequestParts<S> for ExtractMethod {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(ExtractMethod(parts.method.clone()))
    }
}
