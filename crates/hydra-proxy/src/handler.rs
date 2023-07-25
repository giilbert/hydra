use std::f32::consts::E;

use crate::{error_page::ErrorPage, websocket::accept_websocket_connection};
use axum::{
    async_trait,
    body::Bytes,
    extract::FromRequestParts,
    http::{request::Parts, HeaderMap, HeaderValue, Method, StatusCode, Uri},
    response::IntoResponse,
};
use hyper::{header, upgrade::OnUpgrade};
use sha1::{Digest, Sha1};
use tokio_tungstenite::{
    tungstenite::protocol::{Role, WebSocketConfig},
    WebSocketStream,
};

fn sign(key: &[u8]) -> HeaderValue {
    use base64::engine::Engine as _;

    let mut sha1 = Sha1::default();
    sha1.update(key);
    sha1.update(&b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11"[..]);
    let b64 = Bytes::from(base64::engine::general_purpose::STANDARD.encode(sha1.finalize()));
    HeaderValue::from_maybe_shared(b64).expect("base64 is a valid value")
}

#[axum::debug_handler]
pub async fn handler(
    mut custom_extract: CustomExtract,
    uri: Uri,
    // Host(host): Host,
    mut headers: HeaderMap,
    body: Bytes,
) -> Result<impl IntoResponse, ErrorPage> {
    let is_websocket_upgrade = headers
        .get("Upgrade")
        .map(|v| v == "websocket")
        .unwrap_or(false);

    let hydra_server_url = "http://localhost:3100";
    let proxy_url = format!("{hydra_server_url}/proxy");

    headers.insert(
        "X-Hydra-URI",
        HeaderValue::from_str(&uri.to_string())
            .map_err(|_| ErrorPage::error("Something went wrong."))?,
    );

    if is_websocket_upgrade {
        if custom_extract.method != Method::GET {
            return Err(ErrorPage::bad_request(
                "WebSocket upgrade must be GET request.",
            ));
        }

        let sec_websocket_key = headers
            .get(header::SEC_WEBSOCKET_KEY)
            .ok_or_else(|| ErrorPage::bad_request("Missing Sec-WebSocket-Key header."))?
            .clone();
        let on_upgrade = custom_extract
            .on_upgrade
            .take()
            .ok_or_else(|| ErrorPage::error("Something went wrong."))?;

        let config = WebSocketConfig {
            ..Default::default()
        };

        tokio::spawn(async move {
            let upgraded = match on_upgrade.await {
                Ok(upgraded) => upgraded,
                Err(err) => {
                    log::error!("WebSocket upgrade error: {err}");
                    return;
                }
            };

            accept_websocket_connection(
                WebSocketStream::from_raw_socket(upgraded, Role::Server, Some(config)).await,
            )
            .await;
        });

        const UPGRADE: HeaderValue = HeaderValue::from_static("upgrade");
        const WEBSOCKET: HeaderValue = HeaderValue::from_static("websocket");

        let mut response_headers = HeaderMap::new();
        response_headers.insert(header::CONNECTION, UPGRADE);
        response_headers.insert(header::UPGRADE, WEBSOCKET);
        response_headers.insert(
            header::SEC_WEBSOCKET_ACCEPT,
            sign(sec_websocket_key.as_bytes()),
        );
        response_headers.insert("X-Forwarded-By", HeaderValue::from_static("hydra-proxy"));

        return Ok((
            StatusCode::SWITCHING_PROTOCOLS,
            response_headers,
            Bytes::new(),
        ));
    }

    let client = reqwest::Client::new();
    let request = client
        .request(custom_extract.method, proxy_url)
        .headers(headers)
        .body(body)
        .build()
        .map_err(|_| ErrorPage::error("Error constructing forwarded request."))?;

    let response = client
        .execute(request)
        .await
        .map_err(|_| ErrorPage::bad_gateway("Error handling request."))?;

    let mut response_headers = response.headers().clone();
    // TODO: check that the content length is below BODY_LIMIT
    let status_code = response.status();
    let response_bytes = response.bytes().await.map_err(|e| {
        log::error!("error decoding response body: {e}");
        ErrorPage::error("Error decoding response body")
    })?;

    response_headers.insert("X-Forwarded-By", HeaderValue::from_static("hydra-proxy"));

    Ok((status_code, response_headers, response_bytes))
}

pub struct CustomExtract {
    pub method: Method,
    pub on_upgrade: Option<OnUpgrade>,
}

#[async_trait]
impl<S> FromRequestParts<S> for CustomExtract {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let on_upgrade = parts.extensions.remove::<OnUpgrade>();

        Ok(CustomExtract {
            method: parts.method.clone(),
            on_upgrade,
        })
    }
}
