use crate::{container::Container, proxy_websockets::WebSocketConnectionRequest, AppState};
use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use parking_lot::Mutex;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use shared::{
    prelude::*,
    protocol::{
        ContainerProxyRequest, ContainerProxyResponse, ContainerRpcRequest, ContainerSent,
        ExecuteOptions, ProxyError,
    },
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
};
use uuid::Uuid;

/// Sent from the client to the server
#[derive(Debug, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ClientMessage {
    PtyInput { id: u32, input: String },
    CreatePty { rows: u16, cols: u16 },
    Run,
    Crash,
}

/// Sent from the server to the client
#[derive(Debug, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum ServerMessage {
    PtyOutput { output: String },
    PtyExit { id: u32 },
}

pub type ProxyPayload = (
    ContainerProxyRequest,
    oneshot::Sender<Result<ContainerProxyResponse, ProxyError>>,
);

/// Where all the magic happens
///
/// **This struct:**
/// - Handles communication between the client and the container
///   - Server -> client: `ServerMessage` enum
///   - Client -> server: `ClientMessage` enum
/// - Makes sure everything is cleaned up when the client leaves
/// - Make sure everything the client does with the container is allowed
/// - Registers the session with redis, for proxying
///
/// **It DOES NOT:**
/// - Make sure everything is cleaned up when the container crashes
/// - Handle messages DIRECTLY from the container
#[derive(Debug)]
pub struct Session {
    pub ticket: Uuid,
    pub display_id: String,
    pub proxy_requests: mpsc::Sender<ProxyPayload>,
    pub websocket_connections_requests_tx: mpsc::Sender<WebSocketConnectionRequest>,
    pub options: Arc<ExecuteOptions>,
    pub exited: Arc<AtomicBool>,

    proxy_rx: Mutex<Option<mpsc::Receiver<ProxyPayload>>>,
    websocket_connections_requests_rx: Mutex<Option<mpsc::Receiver<WebSocketConnectionRequest>>>,
    container: Arc<Container>,
    self_destruct_timer: Mutex<Option<JoinHandle<()>>>,
    app_state: AppState,

    messages_tx: mpsc::Sender<Message>,
    message_rx: Mutex<Option<mpsc::Receiver<Message>>>,
}

impl Session {
    pub async fn new(options: ExecuteOptions, app_state: AppState) -> Result<Self> {
        let options = Arc::new(options);
        let ticket = Uuid::new_v4();
        let container = {
            let mut recv = app_state.container_pool.take_one().await;
            recv.recv()
                .await
                .ok_or_else(|| eyre!("No containers available"))?
        };

        log::info!(
            "[tck-{}] received container {}",
            &ticket.to_string()[0..5],
            container.display_id
        );

        let result = container
            .rpc(ContainerRpcRequest::SetupFromOptions {
                options: options.clone(),
            })
            .await?;

        if let Err(e) = result {
            container.stop().await?;
            log::error!(
                "[tck-{}] failed to setup container: {}",
                &ticket.to_string()[0..5],
                e
            );
            bail!("Failed to setup container")
        }

        let (proxy_tx, proxy_rx) = mpsc::channel(32);
        let (websocket_connections_requests_tx, websocket_connections_requests_rx) =
            mpsc::channel(32);
        let (messages_tx, messages_rx) = mpsc::channel::<Message>(256);

        Ok(Session {
            ticket,
            display_id: format!(
                "tck-{}, dok-{}",
                &ticket.to_string()[0..5],
                &container.docker_id[0..5]
            ),
            proxy_requests: proxy_tx,
            proxy_rx: Mutex::new(Some(proxy_rx)),
            exited: Arc::new(AtomicBool::new(false)),
            options,
            websocket_connections_requests_tx,
            websocket_connections_requests_rx: Mutex::new(Some(websocket_connections_requests_rx)),
            container,
            self_destruct_timer: Mutex::new(None),
            app_state,
            messages_tx,
            message_rx: Mutex::new(Some(messages_rx)),
        })
    }

    /// After a set duration of inactivity, clean the container up
    pub async fn prime_self_destruct(&self) {
        let app_state = self.app_state.clone();
        let ticket = self.ticket.clone();
        let container = self.container.clone();
        let exited = self.exited.clone();

        *self.self_destruct_timer.lock() = Some(tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;

            let count: u32 = app_state
                .redis
                .write()
                .await
                .del(format!("session:{}", ticket))
                .await
                .expect("redis error while deleting session");
            exited.store(true, Ordering::SeqCst);

            if count != 1 {
                log::error!("error removing session from redis: count != 1");
            }

            app_state.sessions.write().await.remove(&ticket);
            let _ = container.stop().await;
        }));
    }

    /// If the session is still active, cancel the self destruct timer
    pub fn cancel_self_destruct(&self) {
        if let Some(handle) = self.self_destruct_timer.lock().take() {
            handle.abort();
        }
    }

    async fn handle_client_message(&self, message: String) -> Result<()> {
        let data = match serde_json::from_str::<ClientMessage>(&message) {
            Ok(data) => data,
            Err(e) => {
                log::error!("[{}] Error parsing message: {}", self.display_id, e);
                return Ok(());
            }
        };

        match data {
            ClientMessage::PtyInput { id, input } => {
                if let Err(err) = self
                    .container
                    .rpc(ContainerRpcRequest::PtyInput { pty_id: id, input })
                    .await?
                {
                    log::error!("[{}] PtyInput error: {}", self.display_id, err);
                }
            }
            ClientMessage::Run => {
                if let Err(err) = self
                    .container
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
                if let Err(err) = self.container.rpc(ContainerRpcRequest::Crash).await? {
                    log::error!("Crash error: {}", err);
                }
            }
            _ => (),
        }

        Ok(())
    }

    async fn send_to_client(&self, message: ServerMessage) -> Result<()> {
        self.messages_tx
            .send(Message::Text(
                serde_json::to_string(&message).expect("serde_json error"),
            ))
            .await?;
        Ok(())
    }

    /// The container only forwards a few messages here for us to handle and send to the client.
    /// This function handles those messages, returning true if the session should be cleaned up.
    async fn handle_container_message(&self, message: ContainerSent) -> Result<bool> {
        match message {
            ContainerSent::PtyOutput { output, .. } => {
                self.send_to_client(ServerMessage::PtyOutput { output })
                    .await?;
            }
            ContainerSent::PtyExit { pty_id: id } => {
                self.send_to_client(ServerMessage::PtyExit { id }).await?;

                if !self.options.persistent {
                    return Ok(true);
                }
            }
            _ => (),
        }

        Ok(false)
    }

    /// This function is called when a client makes a HTTP request to the proxy
    /// and the proxy forwards it to the container.
    ///
    /// This is a separate task since proxying requests is an expensive operation
    /// and we don't want to block the main session loop.
    async fn proxy_requests_loop(self: Arc<Self>) {
        let mut proxy_rx = self
            .proxy_rx
            .lock()
            .take()
            .expect("proxy_rx should be Some");
        let mut stop_rx_clone = self.container.on_stop();

        loop {
            tokio::select! {
                Some((req, response_tx)) = proxy_rx.recv() => {
                    let response = self.container.proxy_request(req).await;
                    let _ = response_tx.send(response);
                }
                _ = stop_rx_clone.changed() => break
            }
        }
    }

    pub async fn handle_container_messages_loop(self: Arc<Self>) {
        let mut container_rx = self.container.listen().expect("container already taken");
        let mut stop_rx = self.container.on_stop();
        loop {
            tokio::select! {
                Some(container_message) = container_rx.recv() => {
                    match self.handle_container_message(container_message).await {
                        Ok(true) => {
                            let _ = self.container.stop().await;
                            break;
                        }
                        Err(e) => log::error!(
                            "[{}] Error handling container message: {e:#?}",
                            self.display_id
                        ),
                        _ => (),
                    }
                }
                _ = stop_rx.changed() => break
            }
        }
    }

    /// This function is called when a WebSocket connection from the
    /// client (not the container) is made
    pub async fn handle_websocket_connection(self: Arc<Self>, ws: WebSocket) -> Result<()> {
        let mut stop_rx = self.container.on_stop();
        self.cancel_self_destruct();

        // get the ip of the machine the container is running on and store it in redis
        let machine_ip = if std::env::var("FLY_PRIVATE_IP").is_ok() {
            std::env::var("FLY_PRIVATE_IP").expect("FLY_PRIVATE_IP should be set")
        } else if std::env::var("DOCKER").is_ok() {
            "hydra-server".to_string()
        } else {
            "localhost".to_string()
        };
        let _: () = self
            .app_state
            .redis
            .write()
            .await
            .set_ex(format!("session:{}", self.ticket), machine_ip, 60 * 60 * 12)
            .await?;

        let (mut ws_tx, mut ws_rx) = ws.split();

        // this task handles proxy requests
        let proxy_requests_task = tokio::spawn(self.clone().proxy_requests_loop());
        let handle_container_messages_task =
            tokio::spawn(self.clone().handle_container_messages_loop());

        let mut messages_rx = self
            .message_rx
            .lock()
            .take()
            .expect("message_rx should be Some");
        let mut websocket_connections_request_rx = self
            .websocket_connections_requests_rx
            .lock()
            .take()
            .expect("websocket_connections_request_rx should be Some");

        // this task handles messages from the client
        loop {
            tokio::select! {
                message = ws_rx.next() => {
                    let message = match message {
                        Some(Ok(Message::Text(message))) => message,
                        Some(Ok(Message::Close(_))) => break,
                        Some(Ok(_)) => continue,
                        Some(Err(e)) => {
                            log::error!("[{}] Error receiving message: {e:#?}", self.display_id);
                            break;
                        }
                        None => break,
                    };

                    log::debug!("[{}] Got client message: {message}", self.display_id);

                    if let Err(err) = self.handle_client_message(message).await {
                        log::error!("[{}] Error handling message: {}", self.display_id, err);
                    }

                    log::debug!("[{}] Handled client message", self.display_id);
                }
                Some(message_to_send) = messages_rx.recv() => {
                    let _ = ws_tx.send(message_to_send).await;
                }
                Some(request) = websocket_connections_request_rx.recv() => {
                    let response = match self
                        .container
                        .create_websocket_connection(request.proxy_request)
                        .await {
                            Ok(response) => response,
                            Err(err) => {
                                log::error!("[{}] Error handling WebSocket connection request: {err:#?}", self.display_id);
                                continue;
                            }
                        };

                    if let Err(e) = request.tx.send(response) {
                        log::error!("[{}] Error sending WebSocket connection response: {e:#?}", self.display_id);
                    }
                }
                _ = stop_rx.changed() => break
            }
        }

        log::debug!("[{}] Closing websocket", self.display_id);

        if let Err(e) = self.container.stop().await {
            log::error!(
                "[{}] Error trying to stop container: {e:#?}",
                self.display_id,
            );
        }

        proxy_requests_task.await?;
        // client_websocket_forward_task.await?;
        handle_container_messages_task.await?;

        let _ = ws_rx.reunite(ws_tx)?.close().await;

        self.prime_self_destruct().await;

        Ok(())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if self.exited.load(Ordering::SeqCst) {
            return;
        }

        log::warn!("Session dropped without being exited.");

        let container = self.container.clone();
        let app_state = self.app_state.clone();
        let ticket = self.ticket.clone();
        tokio::spawn(async move {
            let _ = container.stop().await;

            let count: u32 = app_state
                .redis
                .write()
                .await
                .del(format!("session:{}", ticket))
                .await
                .expect("redis error while deleting session");

            if count != 1 {
                log::error!("error removing session from redis: count != 1");
            }
        });
    }
}
