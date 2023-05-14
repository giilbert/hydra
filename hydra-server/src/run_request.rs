use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use protocol::{ContainerRpcRequest, ContainerSent, ExecuteOptions};
use serde::{Deserialize, Serialize};
use tokio::{
    sync::{mpsc, RwLock},
    task::JoinHandle,
};
use uuid::Uuid;

use crate::{container::Container, AppState};

#[derive(Debug, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ClientMessage {
    PtyInput { id: u32, input: String },
    CreatePty { rows: u16, cols: u16 },
    Run,
    Crash,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum ServerMessage {
    PtyOutput { output: String },
    PtyExit { id: u32 },
}

#[derive(Debug)]
pub struct RunRequest {
    pub ticket: Uuid,
    container: Arc<RwLock<Container>>,
    self_destruct_timer: Option<JoinHandle<()>>,
    app_state: AppState,
}

impl RunRequest {
    pub async fn new(options: ExecuteOptions, app_state: AppState) -> anyhow::Result<Self> {
        let mut container = Container::new().await?;

        container
            .rpc(ContainerRpcRequest::SetupFromOptions {
                files: options.files,
            })
            .await?
            .map_err(|_| anyhow::anyhow!("Error setting up container"))?;

        Ok(RunRequest {
            ticket: Uuid::new_v4(),
            container: Arc::new(RwLock::new(container)),
            self_destruct_timer: None,
            app_state,
        })
    }

    pub fn prime_self_destruct(&mut self) {
        let app_state = self.app_state.clone();
        let ticket = self.ticket.clone();
        let container = self.container.clone();

        self.self_destruct_timer = Some(tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;

            app_state.write().run_requests.remove(&ticket);
            container
                .write()
                .await
                .stop()
                .await
                .expect("Unable to stop container");
        }));
    }

    pub fn cancel_self_destruct(&mut self) {
        if let Some(handle) = self.self_destruct_timer.take() {
            handle.abort();
        }
    }

    async fn handle_message(&mut self, message: String, _messages_tx: &mut mpsc::Sender<Message>) {
        let data = match serde_json::from_str::<ClientMessage>(&message) {
            Ok(data) => data,
            Err(e) => {
                log::error!("Error parsing message: {}", e);
                return;
            }
        };

        match data {
            ClientMessage::PtyInput { id, input } => {
                self.container
                    .write()
                    .await
                    .rpc(ContainerRpcRequest::PtyInput { id, input })
                    .await
                    .unwrap()
                    .unwrap();
            }
            ClientMessage::Run => {
                self.container
                    .write()
                    .await
                    .rpc(ContainerRpcRequest::PtyCreate {
                        command: "python3".to_string(),
                        arguments: vec!["main.py".to_string()],
                    })
                    .await
                    .unwrap()
                    .unwrap();
            }
            ClientMessage::Crash => {
                self.container
                    .write()
                    .await
                    .rpc(ContainerRpcRequest::Crash)
                    .await
                    .unwrap()
                    .unwrap();
            }
            _ => (),
        }
    }

    async fn send_client_message(
        &mut self,
        message: ServerMessage,
        messages_tx: &mut mpsc::Sender<Message>,
    ) {
        messages_tx
            .send(Message::Text(serde_json::to_string(&message).unwrap()))
            .await
            .unwrap();
    }

    async fn handle_container_message(
        &mut self,
        message: ContainerSent,
        messages_tx: &mut mpsc::Sender<Message>,
    ) {
        match message {
            ContainerSent::PtyOutput { output, .. } => {
                self.send_client_message(ServerMessage::PtyOutput { output }, messages_tx)
                    .await;
            }
            ContainerSent::PtyExit { id } => {
                self.send_client_message(ServerMessage::PtyExit { id }, messages_tx)
                    .await;
            }
            _ => (),
        }
    }

    pub async fn handle_websocket_connection(mut self, ws: WebSocket) {
        self.cancel_self_destruct();

        let (mut messages_tx, mut messages_rx) = mpsc::channel::<Message>(100);
        let (mut ws_tx, mut ws_rx) = ws.split();

        let mut container_rx = self.container.write().await.container_rx.take().unwrap();

        loop {
            tokio::select! {
                message = ws_rx.next() => {
                    let message = match message {
                        Some(Ok(Message::Text(message))) => message,
                       Some(Ok(_)) => continue,
                        Some(Err(e)) => {
                            log::error!("Error receiving message: {}", e);
                            break;
                        }
                        None => break,
                    };

                    self.handle_message(message, &mut messages_tx).await;
                }
                Some(message_to_send) = messages_rx.recv() => {
                    ws_tx.send(message_to_send).await.unwrap();
                }
                Some(container_message) = container_rx.recv() => {
                    self.handle_container_message(container_message, &mut messages_tx).await;
                }
            }
        }

        log::info!("Connection closed");
        self.prime_self_destruct();
    }
}