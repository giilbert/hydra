use std::{collections::HashMap, str::FromStr, sync::Arc, time::Duration};

use futures_util::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use protocol::{ContainerProxyRequest, ContainerProxyResponse, ContainerSent, HostSent};

use reqwest::{
    header::{HeaderMap, HeaderName},
    Method,
};
use tokio::{
    fs,
    net::UnixStream,
    sync::{mpsc, Mutex},
};
use tokio_tungstenite::{tungstenite::Message, WebSocketStream};

use crate::{
    procedures::handle_rpc_procedure,
    pty::{Pty, PtyCommands},
    state::State,
};

#[derive(Debug)]
pub enum Command {
    Send(Message),
    RemovePty(u32),
}

pub struct Commands {
    ws_tx: SplitSink<WebSocketStream<UnixStream>, Message>,
    ws_rx: SplitStream<WebSocketStream<UnixStream>>,
    commands_rx: mpsc::Receiver<Command>,
    pub commands_tx: mpsc::Sender<Command>,
    state: Arc<Mutex<State>>,
}

impl Commands {
    pub fn new(ws: WebSocketStream<UnixStream>) -> Self {
        let (ws_tx, ws_rx) = ws.split();
        let (commands_tx, commands_rx) = mpsc::channel(512);

        Self {
            state: Arc::new(Mutex::new(State::new(commands_tx.clone()))),
            ws_tx,
            ws_rx,
            commands_rx,
            commands_tx,
        }
    }

    pub async fn run(mut self) -> anyhow::Result<()> {
        let state = self.state.clone();
        tokio::spawn(async move {
            while let Some(command) = self.commands_rx.recv().await {
                match command {
                    Command::Send(msg) => {
                        if let Err(e) = self.ws_tx.send(msg).await {
                            log::error!("Failed to send message: {}", e);
                        }
                    }
                    Command::RemovePty(id) => {
                        state.lock().await.remove_pty(id);
                    }
                }
            }

            Ok::<_, anyhow::Error>(())
        });

        while let Some(Ok(Message::Binary(msg))) = self.ws_rx.next().await {
            let msg = rmp_serde::from_slice::<HostSent>(&msg);

            log::debug!("Received HostSent message: {:?}", msg);

            let msg = match msg {
                Ok(m) => m,
                Err(e) => {
                    log::error!("Failed to deserialize message: {}", e);
                    continue;
                }
            };

            match msg {
                HostSent::RpcRequest { id, req } => {
                    let res =
                        handle_rpc_procedure(&self.commands_tx, req, self.state.clone()).await;

                    let res = match res {
                        Ok(r) => r,
                        Err(e) => {
                            log::error!("Failed to handle RPC request: {}", e);
                            Err("An internal error occurred".to_string())
                        }
                    };

                    self.commands_tx
                        .send_timeout(
                            Command::Send(Message::Binary(rmp_serde::to_vec_named(
                                &ContainerSent::RpcResponse {
                                    id,
                                    result: serde_json::to_string(&res)?,
                                },
                            )?)),
                            Duration::from_millis(100),
                        )
                        .await?;

                    log::debug!("Sent RPC response: {:?}", res);
                }
                HostSent::ProxyRequest(req_id, req) => {
                    log::info!("Received proxy request: {:?}", req);
                    let proxy_response = make_proxy_request(req).await;
                    log::info!("Proxy response: {:?}", proxy_response);

                    self.commands_tx
                        .send_timeout(
                            Command::Send(Message::Binary(rmp_serde::to_vec_named(
                                &ContainerSent::ProxyResponse {
                                    req_id,
                                    response: proxy_response,
                                },
                            )?)),
                            std::time::Duration::from_millis(100),
                        )
                        .await?;
                }
            }
        }

        Ok::<_, anyhow::Error>(())
    }
}

async fn make_proxy_request(req: ContainerProxyRequest) -> Result<ContainerProxyResponse, String> {
    let method = Method::try_from(req.method.as_str()).map_err(|e| e.to_string())?;
    let url = format!("http://localhost:{}", req.port);
    let mut headers = HeaderMap::new();

    for (k, v) in req.headers {
        headers.insert(
            HeaderName::from_str(&k).map_err(|_| "Error parsing header name")?,
            v.parse().map_err(|_| "Error parsing header value")?,
        );
    }

    log::debug!("Making request: {} {}", method, url);

    // FIXME: THE LINE BELOW SEGFAULTS
    let client = reqwest::Client::new();
    let response = client
        .request(method, url)
        .headers(headers)
        .body(req.body)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let container_response = ContainerProxyResponse {
        status_code: response.status().as_u16(),
        headers: response
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap().to_string()))
            .collect::<HashMap<_, _>>(),
        // TODO: no-copy
        body: response.bytes().await.map_err(|e| e.to_string())?.to_vec(),
    };

    Ok(container_response)
}
