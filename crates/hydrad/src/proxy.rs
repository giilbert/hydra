use crate::{commands::Command, state::State};
use bytes::Bytes;
use futures_util::{SinkExt, Stream, StreamExt};
use http::{header::ToStrError, Request};
use reqwest::{
    header::{HeaderMap, HeaderName},
    Method,
};
use shared::{
    prelude::*,
    protocol::{
        ContainerProxyRequest, ContainerProxyResponse, ContainerSent, ProxyError, WebSocketMessage,
    },
};
use std::{
    collections::HashMap,
    str::FromStr,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
};
use tokio::{net::TcpStream, pin, sync::mpsc};
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};

const MAX_BODY_SIZE: usize = 20 * 1024 * 1024; // 20 MB
static WEBSOCKET_ID: AtomicU32 = AtomicU32::new(0);

pub async fn make_proxy_request(
    req: ContainerProxyRequest,
) -> Result<ContainerProxyResponse, ProxyError> {
    let method = Method::try_from(req.method.as_str()).map_err(|_| {
        ProxyError::RequestError(format!("Invalid method: {}", req.method.as_str()))
    })?;
    let url = format!("http://localhost:{}{}", req.port, req.uri);
    let mut headers = HeaderMap::new();

    for (k, v) in req.headers {
        headers.insert(
            HeaderName::from_str(&k)
                .map_err(|_| ProxyError::RequestError("Error parsing header name".into()))?,
            v.parse()
                .map_err(|_| ProxyError::RequestError("Error parsing header value".into()))?,
        );
    }

    log::debug!("Making request: {} {}", method, url);

    let client = reqwest::Client::new();
    let response = client
        .request(method, url)
        .headers(headers)
        .body(req.body)
        .send()
        .await
        .map_err(|e| ProxyError::UserProgramError(e.to_string()))?;

    let container_response = ContainerProxyResponse {
        status_code: response.status().as_u16(),
        headers: response
            .headers()
            .iter()
            .map(|(k, v)| -> Result<(String, String), ToStrError> {
                Ok((k.to_string(), v.to_str()?.to_string()))
            })
            .collect::<Result<HashMap<_, _>, ToStrError>>()
            .map_err(|_| ProxyError::RequestError("Invalid headers".into()))?,
        body: read_body(
            response
                .headers()
                .get(http::header::CONTENT_LENGTH)
                .map(|v| v.to_str().ok())
                .flatten()
                .map(|v| v.parse().ok())
                .flatten(),
            response.bytes_stream(),
        )
        .await?,
    };

    Ok(container_response)
}

async fn read_body(
    content_length: Option<usize>,
    stream: impl Stream<Item = reqwest::Result<Bytes>>,
) -> Result<Vec<u8>, ProxyError> {
    let mut body = Vec::new();
    pin!(stream);

    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(chunk) => body.extend_from_slice(&chunk),
            Err(e) => {
                log::error!("Error reading body: {}", e);
                return Err(ProxyError::BodyReadError(e.to_string()));
            }
        }

        if let Some(content_length) = content_length {
            if body.len() >= content_length {
                body.truncate(content_length as usize);
                break;
            }
        }

        if body.len() >= MAX_BODY_SIZE {
            log::error!("Body too large");
            return Err(ProxyError::BodyTooLarge);
        }
    }

    Ok(body)
}

#[derive(Deserialize)]
pub enum WebSocketCommands {
    Send(WebSocketMessage),
    Close,
}

pub async fn open_websocket_connection(
    state: Arc<State>,
    req: ContainerProxyRequest,
) -> Result<(u32, mpsc::Sender<WebSocketCommands>), String> {
    let (websocket_tx, websocket_rx) = mpsc::channel::<WebSocketCommands>(512);

    let mut request = Request::builder()
        .method("GET")
        .uri(format!("ws://localhost:{}{}", req.port, req.uri))
        .body(())
        .map_err(|_| "Error building websocket request")?;
    let header_map = request.headers_mut();
    for (k, v) in req.headers {
        header_map.insert(
            HeaderName::from_str(&k).map_err(|_| "Error parsing header name")?,
            v.parse().map_err(|_| "Error parsing header value")?,
        );
    }

    let (connection, _) = connect_async(request)
        .await
        .map_err(|_| "Error connecting to websocket")?;

    let id = WEBSOCKET_ID.fetch_add(1, Ordering::Relaxed);

    tokio::spawn(run_websocket_event_loop(
        id,
        state.clone(),
        connection,
        websocket_rx,
    ));

    Ok((id, websocket_tx))
}

async fn run_websocket_event_loop(
    ws_id: u32,
    state: Arc<State>,
    connection: WebSocketStream<MaybeTlsStream<TcpStream>>,
    mut rx: mpsc::Receiver<WebSocketCommands>,
) -> Result<()> {
    let (mut ws_tx, mut ws_rx) = connection.split();
    let commands = state.commands.lock().clone();

    loop {
        tokio::select! {
            command = rx.recv() => {
                match command {
                    Some(WebSocketCommands::Send(data)) => {
                        ws_tx.send(data.into()).await?;
                    }
                    Some(WebSocketCommands::Close) => break,
                    None => break
                }
            }
            message = ws_rx.next() => {
                match message {
                    Some(Ok(message)) => {
                        commands
                           .send(Command::Send(Message::Binary(rmp_serde::to_vec_named(
                               &ContainerSent::WebSocketMessage { ws_id, message: message.into() },
                           )?)))
                           .await?;
                    }
                    Some(Err(e)) => {
                        log::error!("Error with WebSocket proxy: {e:#?}");
                        break;
                    }
                    None => break
                }
            }
        }
    }

    state.remove_websocket(ws_id);

    Ok(())
}
