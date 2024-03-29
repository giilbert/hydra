use crate::{
    discovery::resolve_server_authority, error_page::ErrorPage,
    websocket::accept_websocket_connection, AppState,
};
use axum::{
    async_trait,
    body::Bytes,
    extract::{FromRequestParts, Host, Query, WebSocketUpgrade},
    http::{request::Parts, HeaderMap, HeaderValue, Method, StatusCode, Uri},
    response::IntoResponse,
    Extension,
};
use futures_util::{SinkExt, StreamExt};
use http::Request;
use hyper::{header, upgrade::OnUpgrade};
use sha1::{Digest, Sha1};
use shared::{
    prelude::*, protocol::WebSocketMessage as SharedWebSocketMessage, ErrorResponse,
    ErrorResponseBody,
};
use std::sync::Arc;
use tokio_tungstenite::{
    connect_async,
    tungstenite::protocol::{Role, WebSocketConfig},
    WebSocketStream,
};
use uuid::Uuid;

fn sign(key: &[u8]) -> HeaderValue {
    use base64::engine::Engine as _;

    let mut sha1 = Sha1::default();
    sha1.update(key);
    sha1.update(&b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11"[..]);
    let b64 = Bytes::from(base64::engine::general_purpose::STANDARD.encode(sha1.finalize()));
    HeaderValue::from_maybe_shared(b64).expect("base64 is a valid value")
}

#[axum::debug_handler]
pub async fn container_proxy_handler(
    Extension(app_state): Extension<Arc<AppState>>,
    mut custom_extract: CustomExtract,
    uri: Uri,
    Host(host): Host,
    mut headers: HeaderMap,
    body: Bytes,
) -> Result<impl IntoResponse, ErrorPage> {
    let is_websocket_upgrade = headers
        .get("Upgrade")
        .map(|v| v == "websocket")
        .unwrap_or(false);

    let ParsedHost { session_id } = ParsedHost::parse(host)?;
    log::info!("[{session_id}] {} {:#?}", custom_extract.method, uri);

    let proxy_authority = resolve_server_authority(&app_state, &session_id)
        .await
        .map_err(|e| {
            log::error!("error resolving server url: {e}");
            ErrorPage::error("Something went wrong.")
        })?
        .ok_or_else(|| ErrorPage::not_found("Session not found."))?;

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

            let proxy_url = format!("ws://{proxy_authority}/proxy-websocket");
            log::info!("[{session_id}] ws connection -> {proxy_url}");

            accept_websocket_connection(
                proxy_url,
                headers,
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
        .request(
            custom_extract.method,
            format!("http://{proxy_authority}/proxy"),
        )
        .headers(headers)
        .body(body)
        .build()
        .map_err(|e| {
            log::error!("error constructing forwarded request: {e}");
            ErrorPage::error("Error constructing forwarded request.")
        })?;

    let response = client.execute(request).await.map_err(|e| {
        log::error!("{e:#?}");
        ErrorPage::bad_gateway("Error handling request. Is your program online?")
    })?;

    if response
        .headers()
        .get("X-Hydra-Error")
        .is_some_and(|v| v == "true")
    {
        let error_response = response
            .json::<ErrorResponseBody>()
            .await
            .map_err(|_| ErrorPage::error("Something went wrong."))?;

        let error_page = match error_response.status {
            502 => ErrorPage::bad_gateway(error_response.message),
            400 => ErrorPage::bad_request(error_response.message),
            401 => ErrorPage::unauthorized(error_response.message),
            404 => ErrorPage::not_found(error_response.message),
            _ => ErrorPage::error(error_response.message),
        };

        return Err(error_page);
    }

    let mut response_headers = response.headers().clone();
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

struct ParsedHost {
    pub session_id: Uuid,
}

impl ParsedHost {
    pub fn parse(host: String) -> Result<ParsedHost, ErrorPage> {
        let bad_option = || ErrorPage::bad_request("Invalid host header.");
        let bad_result = |_| ErrorPage::bad_request("Invalid host header.");

        let subdomain = host.split('.').next().ok_or_else(bad_option)?.to_string();
        let parts = subdomain.split("--").collect::<Vec<_>>();

        let session_id = parts
            .get(0)
            .ok_or_else(bad_option)?
            .parse::<Uuid>()
            .map_err(bad_result)?;

        Ok(ParsedHost { session_id })
    }
}

#[derive(Deserialize)]
pub struct WebSocketQueryParams {
    ticket: Uuid,
}

pub async fn client_websocket_handler(
    Extension(state): Extension<Arc<AppState>>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
    Query(query): Query<WebSocketQueryParams>,
) -> Result<impl IntoResponse, ErrorResponse> {
    let server_authority = resolve_server_authority(&state, &query.ticket)
        .await
        .map_err(|e| {
            log::error!("error resolving server url: {e}");
            ErrorResponse::error("Something went wrong.")
        })?
        .ok_or_else(|| ErrorResponse::not_found("Session not found."))?;

    let mut builder = Request::builder().uri(format!(
        "ws://{server_authority}/execute?ticket={}",
        query.ticket
    ));
    for (key, value) in headers {
        if let Some(key) = key {
            builder = builder.header(key, value);
        }
    }
    let request = builder.body(()).expect("request builder is valid");

    let (server_ws, _response) = connect_async(request).await.map_err(|e| {
        match e {
            tokio_tungstenite::tungstenite::Error::Http(response) => {
                log::error!("error connecting to server websocket {response:#?}");
                let body_text = response
                    .body()
                    .as_ref()
                    .map(|b| String::from_utf8_lossy(b).to_string())
                    .unwrap_or_else(|| "[error reading body]".to_string());
                log::error!("body text: {body_text}");
            }
            _ => log::error!("error connecting to server websocket {e:#?}"),
        }

        ErrorResponse::error("Something went wrong.")
    })?;

    Ok(ws.on_upgrade(|ws| async move {
        let (mut server_tx, mut server_rx) = server_ws.split();
        let (mut client_tx, mut client_rx) = ws.split();

        // forward server_rx messages to client_tx
        loop {
            tokio::select! {
                server_msg = server_rx.next() => {
                    match server_msg {
                        Some(Ok(msg)) => {
                            let msg = SharedWebSocketMessage::from(msg);
                            if let Err(e) = client_tx.send(msg.into()).await {
                                log::error!("error sending to client websocket: {e}");
                                break;
                            }
                        }
                        Some(Err(e)) => {
                            log::error!("error reading from server websocket: {e}");
                            break;
                        }
                        None => {
                            break;
                        }
                    }
                },
                client_msg = client_rx.next() => {
                    match client_msg {
                        Some(Ok(msg)) => {
                            let msg = SharedWebSocketMessage::from(msg);
                            if let Err(e) = server_tx.send(msg.into()).await {
                                log::error!("error sending to server websocket: {e}");
                                break;
                            }
                        }
                        Some(Err(e)) => {
                            log::error!("error reading from client websocket: {e}");
                            break;
                        }
                        None => {
                            break;
                        }
                    }
                },
            }
        }

        let _ = server_tx
            .reunite(server_rx)
            .expect("pairs are matching")
            .close(None)
            .await;
        let _ = client_tx
            .reunite(client_rx)
            .expect("pairs are matching")
            .close()
            .await;
    }))
}
