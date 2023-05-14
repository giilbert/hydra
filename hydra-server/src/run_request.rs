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

/// # Where all the magic happens
///
/// ## This struct:
/// - Handles communication between the client and the container
///   - Server -> client: `ServerMessage` enum
///   - Client -> server: `ClientMessage` enum
/// - Makes sure everything is cleaned up when the client leaves
/// - Make sure everything the client does with the container is allowed
///
/// ## It DOES NOT:
/// - Make sure everything is cleaned up when the container crashes
/// - Handle messages DIRECTLY from the container
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

    /// After a set duration of inactivity, clean the container up
    pub fn prime_self_destruct(&mut self) {
        let app_state = self.app_state.clone();
        let ticket = self.ticket.clone();
        let container = self.container.clone();

        self.self_destruct_timer = Some(tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;

            app_state.write().run_requests.remove(&ticket);
            let _ = container.write().await.stop().await;
            log::info!(
                "Container {}: cleaned",
                &container.read().await.docker_id[..5]
            );
        }));
    }

    pub fn cancel_self_destruct(&mut self) {
        if let Some(handle) = self.self_destruct_timer.take() {
            handle.abort();
        }
    }

    async fn handle_message(
        &mut self,
        message: String,
        _messages_tx: &mut mpsc::Sender<Message>,
    ) -> anyhow::Result<()> {
        let data = match serde_json::from_str::<ClientMessage>(&message) {
            Ok(data) => data,
            Err(e) => {
                log::error!("Error parsing message: {}", e);
                return Ok(());
            }
        };

        let mut container = self.container.write().await;

        match data {
            ClientMessage::PtyInput { id, input } => {
                if let Err(err) = container
                    .rpc(ContainerRpcRequest::PtyInput { id, input })
                    .await?
                {
                    log::error!("PtyInput error: {}", err)
                }
            }
            ClientMessage::Run => {
                if let Err(err) = container
                    .rpc(ContainerRpcRequest::PtyCreate {
                        command: "python3".to_string(),
                        arguments: vec!["main.py".to_string()],
                    })
                    .await?
                {
                    log::error!("Run error: {}", err);
                }
            }
            ClientMessage::Crash => {
                if let Err(err) = container.rpc(ContainerRpcRequest::Crash).await? {
                    log::error!("Crash error: {}", err);
                }
            }
            _ => (),
        }

        Ok(())
    }

    async fn send_client_message(
        &mut self,
        message: ServerMessage,
        messages_tx: &mut mpsc::Sender<Message>,
    ) -> anyhow::Result<()> {
        messages_tx
            .send(Message::Text(
                serde_json::to_string(&message).expect("serde_json error"),
            ))
            .await?;
        Ok(())
    }

    async fn handle_container_message(
        &mut self,
        message: ContainerSent,
        messages_tx: &mut mpsc::Sender<Message>,
    ) -> anyhow::Result<()> {
        match message {
            ContainerSent::PtyOutput { output, .. } => {
                self.send_client_message(ServerMessage::PtyOutput { output }, messages_tx)
                    .await?;
            }
            ContainerSent::PtyExit { id } => {
                self.send_client_message(ServerMessage::PtyExit { id }, messages_tx)
                    .await?;
            }
            _ => (),
        }

        Ok(())
    }

    pub async fn handle_websocket_connection(mut self, ws: WebSocket) -> anyhow::Result<()> {
        let mut stop_rx = self.container.read().await.stop_rx.clone();
        self.cancel_self_destruct();

        let (mut messages_tx, mut messages_rx) = mpsc::channel::<Message>(100);
        let (mut ws_tx, mut ws_rx) = ws.split();

        let mut container_rx = self
            .container
            .write()
            .await
            .container_rx
            .take()
            .expect("container already taken");

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

                    if let Err(err) = self.handle_message(message, &mut messages_tx).await {
                        log::error!("Error handling message: {}", err);
                    }
                }
                Some(message_to_send) = messages_rx.recv() => {
                    ws_tx.send(message_to_send).await?;
                }
                Some(container_message) = container_rx.recv() => {
                    if let Err(err) = self.handle_container_message(container_message, &mut messages_tx).await {
                        log::error!("Error handling container message: {}", err);
                    }
                }
                _ = stop_rx.changed() => {
                    break
                }
            }
        }

        log::info!("Connection closed");
        self.prime_self_destruct();

        Ok(())
    }
}
