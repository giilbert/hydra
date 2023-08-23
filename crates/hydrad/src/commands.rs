use crate::{
    procedures::handle_rpc_procedure,
    proxy::{make_proxy_request, open_websocket_connection},
    state::State,
};
use futures_util::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use shared::{
    prelude::*,
    protocol::{ContainerSent, HostSent, ProxyError},
};
use std::{sync::Arc, time::Duration};
use tokio::{net::UnixStream, sync::mpsc};
use tokio_tungstenite::{tungstenite::Message, WebSocketStream};

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
    state: Arc<State>,
}

impl Commands {
    pub fn new(ws: WebSocketStream<UnixStream>) -> Self {
        let (ws_tx, ws_rx) = ws.split();
        let (commands_tx, commands_rx) = mpsc::channel(512);

        Self {
            state: Arc::new(State::new(commands_tx.clone())),
            ws_tx,
            ws_rx,
            commands_rx,
            commands_tx,
        }
    }

    pub async fn run(mut self) -> Result<()> {
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
                        state.remove_pty(id);
                    }
                }
            }

            Ok::<_, Report>(())
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
                HostSent::RpcRequest { req_id, req } => {
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
                                    req_id,
                                    result: serde_json::to_string(&res)?,
                                },
                            )?)),
                            Duration::from_millis(100),
                        )
                        .await?;

                    log::debug!("Sent RPC response: {:?}", res);
                }
                HostSent::ProxyHTTPRequest { req_id, req } => {
                    log::debug!("Received proxy request: {:?}", req);
                    let proxy_response = make_proxy_request(req).await;
                    log::debug!("Proxy response: {:?}", proxy_response);

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
                HostSent::WebSocketMessage { ws_id, message } => {
                    log::debug!("Received WebSocket message: [{ws_id}] {message:?}");
                    let _ = self.state.send_ws_message(ws_id, message).await;
                }
                HostSent::CreateWebSocketConnection { req_id, req } => {
                    let response = open_websocket_connection(self.state.clone(), req)
                        .await
                        .map(|(ws_id, commands)| {
                            self.state.add_websocket(ws_id, commands);
                            ws_id
                        });

                    self.commands_tx
                        .send_timeout(
                            Command::Send(Message::Binary(rmp_serde::to_vec_named(
                                &ContainerSent::WebSocketConnectionResponse { req_id, response },
                            )?)),
                            Duration::from_millis(100),
                        )
                        .await?;
                }
            }
        }

        Ok(())
    }
}
