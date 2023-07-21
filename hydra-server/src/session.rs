use crate::{container::Container, AppState};
use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use protocol::{
    ContainerProxyRequest, ContainerProxyResponse, ContainerRpcRequest, ContainerSent,
    ExecuteOptions,
};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::{
    sync::{mpsc, oneshot, Mutex, RwLock},
    task::JoinHandle,
};
use uuid::Uuid;

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

pub type ProxyPayload = (
    ContainerProxyRequest,
    oneshot::Sender<ContainerProxyResponse>,
);

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
pub struct Session {
    pub ticket: Uuid,
    pub display_id: String,
    pub proxy_requests: mpsc::Sender<ProxyPayload>,

    proxy_rx: Arc<Mutex<Option<mpsc::Receiver<ProxyPayload>>>>,
    container: Arc<RwLock<Container>>,
    self_destruct_timer: Mutex<Option<JoinHandle<()>>>,
    app_state: AppState,
}

impl Session {
    pub async fn new(options: ExecuteOptions, app_state: AppState) -> anyhow::Result<Self> {
        let ticket = Uuid::new_v4();
        let container = {
            let mut recv = app_state.container_pool.take_one().await;
            recv.recv()
                .await
                .ok_or_else(|| anyhow::anyhow!("No containers available"))?
        };

        log::info!(
            "[tck-{}] received container {}",
            &ticket.to_string()[0..5],
            container.display_id
        );

        let result = container
            .rpc(ContainerRpcRequest::SetupFromOptions {
                files: options.files,
            })
            .await?;

        if let Err(e) = result {
            container.stop().await?;
            log::error!(
                "[tck-{}] failed to setup container: {}",
                &ticket.to_string()[0..5],
                e
            );
            anyhow::bail!("Failed to setup container")
        }

        let (proxy_tx, proxy_rx) = mpsc::channel(32);

        Ok(Session {
            ticket,
            display_id: format!(
                "tck-{}, dok-{}",
                ticket.to_string()[0..5].to_string(),
                container.docker_id[0..5].to_string()
            ),
            proxy_requests: proxy_tx,
            proxy_rx: Arc::new(Mutex::new(Some(proxy_rx))),
            container: Arc::new(RwLock::new(container)),
            self_destruct_timer: Mutex::new(None),
            app_state,
        })
    }

    /// After a set duration of inactivity, clean the container up
    pub async fn prime_self_destruct(&self) {
        let app_state = self.app_state.clone();
        let ticket = self.ticket.clone();
        let container = self.container.clone();

        *self.self_destruct_timer.lock().await = Some(tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;

            let count: u32 = app_state
                .redis
                .write()
                .await
                .del(format!("session-{}", ticket))
                .await
                .expect("redis error while deleting session");

            if count != 1 {
                log::error!("error removing session from redis: count != 1");
            }

            app_state.sessions.write().await.remove(&ticket);
            let _ = container.write().await.stop().await;
        }));
    }

    pub async fn cancel_self_destruct(&mut self) {
        if let Some(handle) = self.self_destruct_timer.lock().await.take() {
            handle.abort();
        }
    }

    async fn handle_client_message(
        &self,
        message: String,
        _messages_tx: &mut mpsc::Sender<Message>,
    ) -> anyhow::Result<()> {
        let data = match serde_json::from_str::<ClientMessage>(&message) {
            Ok(data) => data,
            Err(e) => {
                log::error!("[{}] Error parsing message: {}", self.display_id, e);
                return Ok(());
            }
        };

        let container = self.container.read().await;

        match data {
            ClientMessage::PtyInput { id, input } => {
                if let Err(err) = container
                    .rpc(ContainerRpcRequest::PtyInput { id, input })
                    .await?
                {
                    log::error!("[{}] PtyInput error: {}", self.display_id, err);
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
                    log::error!("[{}] Run error: {}", self.display_id, err);
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
        message: ContainerSent,
        messages_tx: &mut mpsc::Sender<Message>,
    ) -> anyhow::Result<()> {
        match message {
            ContainerSent::PtyOutput { output, .. } => {
                Self::send_client_message(ServerMessage::PtyOutput { output }, messages_tx).await?;
            }
            ContainerSent::PtyExit { id } => {
                Self::send_client_message(ServerMessage::PtyExit { id }, messages_tx).await?;
            }
            _ => (),
        }

        Ok(())
    }

    pub async fn handle_websocket_connection(mut self, ws: WebSocket) -> anyhow::Result<()> {
        let mut stop_rx = self.container.read().await.on_stop();
        self.cancel_self_destruct().await;

        let machine_ip = if std::env::var("FLY_PRIVATE_IP").is_ok() {
            std::env::var("FLY_PRIVATE_IP").unwrap()
        } else {
            "http://localhost:3100".to_string()
        };

        let _: () = self
            .app_state
            .redis
            .write()
            .await
            .set(format!("session-{}", self.ticket), machine_ip)
            .await?;

        let this = Arc::new(self);

        let (mut messages_tx, mut messages_rx) = mpsc::channel::<Message>(100);
        let (ws_tx, mut ws_rx) = ws.split();

        let mut container_rx = this
            .container
            .write()
            .await
            .listen()
            .expect("container already taken");

        // this task handles messages from the container
        let display_id_clone = this.display_id.clone();
        let mut messages_tx_clone = messages_tx.clone();
        let mut stop_rx_clone = stop_rx.clone();
        let container_message_task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    Some(container_message) = container_rx.recv() => {
                        if let Err(err) = Self::handle_container_message(container_message, &mut messages_tx_clone).await {
                            log::error!("[{}] Error handling container message: {}", display_id_clone, err);
                        }
                    }
                    _ = stop_rx_clone.changed() => {
                        break
                    }
                }
            }
        });

        // this task handles proxy requests
        let this_clone = this.clone();
        let mut proxy_rx = this.proxy_rx.lock().await.take().unwrap();
        let mut stop_rx_clone = stop_rx.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    Some((req, response_tx)) = proxy_rx.recv() => {
                        // TODO: handle error
                        let response = match this_clone
                            .container
                            .read()
                            .await
                            .proxy_request(req)
                            .await {
                                Ok(response) => response,
                                Err(err) => {
                                    log::error!("[{}] Error handling proxy request: {}", this_clone.display_id, err);
                                    continue;
                                }
                            };

                        response_tx.send(response).unwrap();
                    }
                    _ = stop_rx_clone.changed() => {
                        break
                    }
                }
            }
        });

        // this task forwards messages to the client
        let this_clone = this.clone();
        let mut stop_rx_clone = stop_rx.clone();
        // takes ws_tx and then puts it back when the task is done
        let maybe_ws_tx = Arc::new(Mutex::new(Some(ws_tx)));
        let maybe_ws_tx_clone = maybe_ws_tx.clone();
        let client_sender_task = tokio::spawn(async move {
            let mut ws_tx = maybe_ws_tx_clone.lock().await.take().unwrap();
            loop {
                tokio::select! {
                    Some(message_to_send) = messages_rx.recv() => {
                        let _ = ws_tx.send(message_to_send).await;
                    }
                    _ = stop_rx_clone.changed() => {
                        break
                    }
                }
            }
            *maybe_ws_tx_clone.lock().await = Some(ws_tx);
        });

        // this task handles messages from the client
        loop {
            tokio::select! {
                message = ws_rx.next() => {
                    let message = match message {
                        Some(Ok(Message::Text(message))) => message,
                        Some(Ok(Message::Close(_))) => break,
                        Some(Ok(_)) => continue,
                        Some(Err(e)) => {
                            log::error!("[{}] Error receiving message: {}", this_clone.display_id, e);
                            break;
                        }
                        None => break,
                    };

                    log::debug!("[{}] Got client message: {}", this_clone.display_id, message);

                    if let Err(err) = this_clone.handle_client_message(message, &mut messages_tx).await {
                        log::error!("[{}] Error handling message: {}", this_clone.display_id, err);
                    }

                    log::debug!("[{}] Handled client message", this_clone.display_id);
                }
                _ = stop_rx.changed() => {
                    break
                }
            }
        }

        log::debug!("[{}] Closing websocket", this.display_id);

        if let Err(e) = this.container.read().await.stop().await {
            log::error!(
                "[{}] Error trying to stop container: {}",
                this.display_id,
                e
            );
        }

        client_sender_task.await?;
        container_message_task.await?;

        let ws_tx = maybe_ws_tx
            .lock()
            .await
            .take()
            .expect("ws_tx not given back");

        if let Err(e) = ws_rx.reunite(ws_tx)?.close().await {
            use tokio_tungstenite::tungstenite::Error as WSError;

            let inner = e
                .into_inner()
                .downcast::<WSError>()
                // i think only tungstenite errors are possible, but idk
                .map_err(|e| {
                    anyhow::anyhow!(
                        "[{}] Received anything but a tungstenite error: {:?}",
                        this.display_id,
                        e
                    )
                })?;

            match *inner {
                WSError::ConnectionClosed => {
                    log::info!("[{}] Connection closed", this.display_id);
                }
                e => {
                    log::error!("[{}] Error closing connection: {}", this.display_id, e);
                }
            }
        }

        this.prime_self_destruct().await;

        Ok(())
    }
}
