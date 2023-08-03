use crate::{
    proxy_websockets::{WebSocketConnection, WebSocketConnectionRequest},
    AppState,
};
use axum::{
    async_trait,
    body::Bytes,
    extract::{FromRequestParts, Host, State, WebSocketUpgrade},
    http::{request::Parts, HeaderMap, HeaderName, Method, StatusCode},
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use shared::{protocol::ContainerProxyRequest, ErrorResponse};
use std::{collections::HashMap, str::FromStr};
use tokio::sync::oneshot;
use uuid::Uuid;

#[axum::debug_handler]
pub async fn proxy_http(
    State(app_state): State<AppState>,
    ExtractMethod(method): ExtractMethod,
    Host(host): Host,
    headers: HeaderMap,
    body: Bytes,
) -> Result<impl IntoResponse, ErrorResponse> {
    let ParsedHost { session_id, port } = ParsedHost::parse(host)?;

    let uri = headers
        .get("X-Hydra-URI")
        .ok_or_else(|| ErrorResponse::bad_request("X-Hydra-URI header not found"))?
        .to_str()
        .map_err(|_| ErrorResponse::bad_request("X-Hydra-URI is an invalid string"))?;

    let proxy_requests = app_state
        .proxy_requests
        .read()
        .await
        .get(&session_id.into())
        .ok_or_else(|| ErrorResponse::not_found("Container not found"))?
        .clone();

    let request = ContainerProxyRequest {
        method: method.to_string(),
        uri: uri.to_string(),
        // FIXME: is there a way to not clone the underlying byte representation?
        body: body.to_vec(),
        headers: parse_client_headers(&headers)?,
        port: port.into(),
    };
    let (response_tx, response_rx) = oneshot::channel();

    // TODO: add a timeout
    proxy_requests
        .send((request, response_tx))
        .await
        .map_err(|_| ErrorResponse::error("Failed to send request to container proxy"))?;

    let res = response_rx
        .await
        .map_err(|_| ErrorResponse::error("Failed to receiver response from container proxy"))?;

    let headers = parse_container_headers(&res.headers)?;
    let status_code = StatusCode::try_from(res.status_code)
        .map_err(|_| ErrorResponse::bad_gateway("container sent invalid status code"))?;
    Ok((status_code, headers, res.body))
}

#[axum::debug_handler]
pub async fn proxy_websocket(
    State(app_state): State<AppState>,
    Host(host): Host,
    upgrade: WebSocketUpgrade,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ErrorResponse> {
    let ParsedHost { session_id, port } = ParsedHost::parse(host)?;

    let uri = headers
        .get("X-Hydra-URI")
        .ok_or_else(|| ErrorResponse::bad_request("X-Hydra-URI header not found"))?
        .to_str()
        .map_err(|_| ErrorResponse::bad_request("X-Hydra-URI is an invalid string"))?;

    let request = ContainerProxyRequest {
        method: "GET".to_string(),
        uri: uri.to_string(),
        // FIXME: is there a way to not clone the underlying byte representation?
        body: vec![],
        headers: parse_client_headers(&headers)?,
        port: port.into(),
    };

    let (req, res) = WebSocketConnectionRequest::new(request);
    app_state
        .websocket_connection_requests
        .read()
        .await
        .get(&session_id.into())
        .ok_or_else(|| ErrorResponse::not_found("Container not found"))?
        .send(req)
        .await
        .map_err(|_| ErrorResponse::error("Failed to send request to container proxy"))?;

    let WebSocketConnection { mut rx, tx, .. } = res.await.map_err(|e| {
        log::error!("Failed to receive response from container proxy: {}", e);
        ErrorResponse::error("Failed to receive response from container proxy")
    })?;

    Ok(upgrade.on_upgrade(|ws| async move {
        let (mut ws_tx, mut ws_rx) = ws.split();
        loop {
            tokio::select! {
                // client -> container
                client_message = ws_rx.next() => {
                    match client_message {
                        Some(Ok(message)) => {
                            if let Err(e) = tx.send(message.into()).await {
                                log::warn!("client -> container send websocket error: {e:#?}");
                                break;
                            }
                        },
                        Some(Err(e)) => {
                            log::warn!("client -> container websocket error: {}", e);
                            break;
                        },
                        None => break,
                    }
                },
                // container -> client
                container_message = rx.recv() => {
                    match container_message {
                        Some(message) => {
                            if let Err(e) = ws_tx.send(message.into()).await {
                                log::warn!("container -> client send websocket error: {e:#?}");
                                break;
                            }
                        },
                        None => break,
                    }
                },
            }
        }
    }))
}

pub struct ExtractMethod(pub Method);

#[async_trait]
impl<S> FromRequestParts<S> for ExtractMethod {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(ExtractMethod(parts.method.clone()))
    }
}

struct ParsedHost {
    pub session_id: Uuid,
    pub port: u16,
}

impl ParsedHost {
    pub fn parse(host: String) -> Result<ParsedHost, ErrorResponse> {
        let bad_option = || ErrorResponse::bad_request("Invalid host header.");
        let bad_result = |_| ErrorResponse::bad_request("Invalid host header.");
        let bad_result_int = |_| ErrorResponse::bad_request("Invalid host header.");

        let subdomain = host.split('.').next().ok_or_else(bad_option)?.to_string();
        let parts = subdomain.split("--").collect::<Vec<_>>();

        let session_id: Uuid = parts
            .get(0)
            .ok_or_else(bad_option)?
            .parse()
            .map_err(bad_result)?;
        let port: u16 = parts
            .get(1)
            .ok_or_else(bad_option)?
            .parse()
            .map_err(bad_result_int)?;

        Ok(ParsedHost { session_id, port })
    }
}

fn parse_client_headers(
    client_headers: &HeaderMap,
) -> Result<HashMap<String, String>, ErrorResponse> {
    let mut headers = HashMap::new();
    for (k, v) in client_headers.iter() {
        headers.insert(
            k.to_string(),
            v.to_str()
                .map_err(|_| ErrorResponse::bad_request("Invalid header value"))?
                .to_string(),
        );
    }

    Ok(headers)
}

fn parse_container_headers(
    container_headers: &HashMap<String, String>,
) -> Result<HeaderMap, ErrorResponse> {
    let mut headers = HeaderMap::new();
    for (k, v) in container_headers.iter() {
        headers.insert(
            HeaderName::from_str(&k)
                .map_err(|_| ErrorResponse::bad_gateway("container sent invalid header name"))?,
            v.parse()
                .map_err(|_| ErrorResponse::bad_gateway("container sent invalid header value"))?,
        );
    }

    Ok(headers)
}
