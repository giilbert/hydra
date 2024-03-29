use crate::{
    config::Config,
    proxy_websockets::{WebSocketConnection, WebSocketConnectionCommands},
    rpc::{RpcError, RpcRecords},
    shutdown, Environment,
};
use bollard::{
    container::{self},
    service::{ContainerStateStatusEnum, HostConfig, ThrottleDevice},
    Docker,
};
use futures_util::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use lazy_static::lazy_static;
use parking_lot::Mutex;
use serde_json::Value;
use shared::{
    prelude::*,
    protocol::{
        ContainerProxyRequest, ContainerProxyResponse, ContainerRpcRequest, ContainerSent,
        ExecuteOptions, HostSent, ProxyError,
    },
};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    fs,
    net::{UnixListener, UnixStream},
    sync::{
        mpsc::{self, error::SendError},
        watch, RwLock,
    },
};
use tokio_tungstenite::{accept_async, tungstenite::Message, WebSocketStream};
use uuid::Uuid;

pub type StopRx = watch::Receiver<()>;

lazy_static! {
    static ref DOCKER: Docker =
        Docker::connect_with_local_defaults().expect("unable to connect to docker");
}

/// The heart of this thing
///
/// This struct:
/// - Interfaces with Docker, to create and destroy the container
/// - Relays container commands between the container and the respective RunRequest
#[derive(Debug)]
pub struct Container {
    pub docker_id: String,
    pub display_id: String,
    pub stopped: AtomicBool,
    pub socket_dir: PathBuf,

    /// An event that is fired when the container is stopped
    stop_rx: watch::Receiver<()>,
    stop_tx: watch::Sender<()>,

    // used for bidirectional communication between the container and connected client
    container_rx: Mutex<Option<mpsc::Receiver<ContainerSent>>>,
    container_tx: mpsc::Sender<ContainerSent>,

    commands_tx: mpsc::Sender<ContainerCommands>,

    deletion_tx: Option<mpsc::Sender<String>>,
    /// Keeps track of RPC calls and is used for responses
    rpc_records: RpcRecords<Result<Value, String>>,
    /// Keeps track of proxy requests and is used for responses
    proxy_records: RpcRecords<Result<ContainerProxyResponse, ProxyError>>,

    websocket_connection_request_records: RpcRecords<Result<WebSocketConnection, ProxyError>>,
    websocket_connections: RwLock<HashMap<u32, mpsc::Sender<WebSocketConnectionCommands>>>,
}

#[derive(Clone, Debug)]
pub enum ContainerCommands {
    SendMessage(HostSent),
    RemoveWebSocketConnection(u32),
    Stop,
}

impl Container {
    pub async fn new(deletion_tx: Option<mpsc::Sender<String>>) -> Result<Arc<Self>> {
        let id = Uuid::new_v4();
        let hydra_run_dir = if Environment::get() == Environment::Production {
            PathBuf::from("/run/hydra")
        } else {
            PathBuf::from("/tmp/hydra")
        };
        let container_socket_dir = hydra_run_dir.join(format!("sockets/{}", id));
        let (stop_tx, mut stop_rx) = watch::channel(());

        fs::create_dir_all(&container_socket_dir)
            .await
            .expect("unable to create container socket dir");

        let addr = format!("{}/conn.sock", container_socket_dir.to_string_lossy());
        let listener = UnixListener::bind(addr).expect("unable to bind to unix socket");

        let create_response = DOCKER
            .create_container(
                Some(container::CreateContainerOptions {
                    name: format!("hydrad--tck-{id}"),
                    ..Default::default()
                }),
                container::Config {
                    image: Some("hydra-turtle"),
                    host_config: Some(HostConfig {
                        binds: Some(vec![format!(
                            "{}:/run/hydra",
                            hydra_run_dir.join(&container_socket_dir).to_string_lossy()
                        )]),
                        // // TESTING VALUE
                        // auto_remove: None,
                        auto_remove: Some(true),
                        cpu_quota: Some(Config::global().docker.cpu_shares),
                        cpuset_cpus: Some(Config::global().docker.cpu_set.clone()),
                        memory: Some(
                            Config::global()
                                .docker
                                .memory
                                .try_into()
                                .expect("invalid memory in config"),
                        ),
                        blkio_device_read_bps: Some(vec![ThrottleDevice {
                            // if its running on fly, it is on the /dev/vda device
                            path: Some(if std::env::var("FLY_PRIVATE_IP").is_ok() {
                                "/dev/vda".to_string()
                            } else {
                                "/dev/sda".to_string()
                            }),
                            rate: Some(Config::global().docker.disk_read_rate as i64),
                        }]),
                        blkio_device_write_bps: Some(vec![ThrottleDevice {
                            // if its running on fly, it is on the /dev/vda device
                            path: Some(if std::env::var("FLY_PRIVATE_IP").is_ok() {
                                "/dev/vda".to_string()
                            } else {
                                "/dev/sda".to_string()
                            }),
                            rate: Some(Config::global().docker.disk_write_rate as i64),
                        }]),
                        ..Default::default()
                    }),
                    // env: Some(vec!["RUST_LOG=hydrad=info"]),
                    ..Default::default()
                },
            )
            .await?;

        let display_id = format!("dok-{}", &create_response.id[0..5]);

        log::info!(
            "Created container: [{display_id}], full id: {}",
            create_response.id
        );

        DOCKER
            .start_container::<String>(&create_response.id, None)
            .await?;

        async fn accept_connection(
            display_id: String,
            container_socket_dir: PathBuf,
            listener: UnixListener,
        ) -> Result<WebSocketStream<UnixStream>> {
            match listener.accept().await {
                Ok((stream, _)) => accept_async(stream).await.map_err(|e| e.into()),
                Err(e) => {
                    log::error!("[{display_id}] Error during initial WebSocket Connection: {e}",);
                    fs::remove_dir_all(&container_socket_dir)
                        .await
                        .expect("unable to remove container socket dir");
                    return Err(e.into());
                }
            }
        }

        let ws_stream = tokio::select! {
            ws_stream = accept_connection(display_id.clone(), container_socket_dir.clone(), listener) => ws_stream?,
            _ = tokio::time::sleep(std::time::Duration::from_secs(10)) => {
                fs::remove_dir_all(&container_socket_dir).await.expect("unable to remove container socket dir");
                return Err(eyre!("container did not connect to websocket within 10 seconds"));
            }
            _ = stop_rx.changed() => {
                fs::remove_dir_all(&container_socket_dir).await.expect("unable to remove container socket dir");
                return Err(eyre!("container stopped before connecting to websocket"));
            }
        };

        let (container_commands_tx, container_commands_rx) = mpsc::channel::<ContainerCommands>(32);

        let rpc_records = RpcRecords::new(stop_rx.clone());
        let proxy_records = RpcRecords::new(stop_rx.clone());
        let websocket_connection_request_records = RpcRecords::new(stop_rx.clone());
        let websocket_connections = RwLock::new(HashMap::new());

        let (container_message_tx, container_message_rx) = mpsc::channel::<ContainerSent>(64);

        let container = Arc::new(Self {
            docker_id: create_response.id,
            display_id,
            deletion_tx,
            commands_tx: container_commands_tx,
            rpc_records,
            proxy_records,
            socket_dir: container_socket_dir,
            container_rx: parking_lot::Mutex::new(Some(container_message_rx)),
            container_tx: container_message_tx,
            stop_rx,
            stop_tx,
            stopped: false.into(),
            websocket_connection_request_records,
            websocket_connections,
        });

        // tokio::spawn(container.clone().forward_logs());
        tokio::spawn(container.clone().monitor());
        tokio::spawn(container.clone().run(ws_stream, container_commands_rx));

        Ok(container)
    }

    pub fn listen(&self) -> Result<mpsc::Receiver<ContainerSent>> {
        self.container_rx
            .lock()
            .take()
            .ok_or(eyre!("container already listened to"))
    }

    pub fn on_stop(&self) -> StopRx {
        self.stop_rx.clone()
    }

    pub async fn stop(&self) -> Result<()> {
        log::info!("[{}]: 1,0. queued normal stop", self.display_id);
        self.commands_tx.send(ContainerCommands::Stop).await?;
        if let Some(deletion_tx) = &self.deletion_tx {
            deletion_tx.send(self.docker_id.clone()).await?;
        }
        self.stopped.store(true, Ordering::Relaxed);
        Ok(())
    }

    pub async fn rpc(&self, req: ContainerRpcRequest) -> Result<Result<Value, String>> {
        let id = Uuid::new_v4();

        log::debug!("[{}]: sent RPC request", self.display_id);
        self.commands_tx
            .send(ContainerCommands::SendMessage(HostSent::RpcRequest {
                req_id: id,
                req,
            }))
            .await?;

        log::debug!("[{}]: waiting for RPC response", self.display_id);
        let res = self.rpc_records.get_response(id).await?;
        log::debug!("[{}]: got RPC response", self.display_id);

        Ok(res)
    }

    pub async fn rpc_setup_from_options(&self, options: ExecuteOptions) -> Result<()> {
        self.rpc(ContainerRpcRequest::SetupFromOptions {
            options: Arc::from(options),
        })
        .await?
        .map_err(|e| eyre!("container failed to setup from options: {:?}", e))?;

        Ok(())
    }

    pub async fn rpc_pty_create(&self, command: String, arguments: Vec<String>) -> Result<u64> {
        let response = self
            .rpc(ContainerRpcRequest::PtyCreate { command, arguments })
            .await?
            .map_err(|e| eyre!("container failed to create pty: {:?}", e))?;

        Ok(response.as_u64().ok_or_else(|| eyre!("invalid pty id"))?)
    }

    pub async fn rpc_pty_input(&self, pty_id: u32, input: String) -> Result<()> {
        self.rpc(ContainerRpcRequest::PtyInput { pty_id, input })
            .await?
            .map_err(|e| eyre!("error inputting: {:?}", e))?;

        Ok(())
    }

    pub async fn proxy_request(
        &self,
        req: ContainerProxyRequest,
    ) -> Result<ContainerProxyResponse, ProxyError> {
        let req_id = Uuid::new_v4();

        log::debug!("[{}]: sent proxy request", self.display_id);
        self.commands_tx
            .send(ContainerCommands::SendMessage(HostSent::ProxyHTTPRequest {
                req_id,
                req,
            }))
            .await
            .map_err(ProxyError::server_error(
                "failed to send proxy request message",
            ))?;

        log::debug!("[{}]: waiting for proxy response", self.display_id);

        let response = self
            .proxy_records
            .get_response(req_id)
            .await
            .map_err(|e| match e {
                RpcError::Timeout => {
                    ProxyError::UserProgramError("program failed to respond before timeout".into())
                }
                _ => {
                    log::error!("error receiving websocket connection response: {:?}", e);
                    ProxyError::InternalError
                }
            })?;

        log::debug!("[{}]: got proxy response", self.display_id);

        response
    }

    pub async fn create_websocket_connection(
        &self,
        req: ContainerProxyRequest,
    ) -> Result<WebSocketConnection, ProxyError> {
        let req_id = Uuid::new_v4();

        self.commands_tx
            .send(ContainerCommands::SendMessage(
                HostSent::CreateWebSocketConnection { req_id, req },
            ))
            .await
            .map_err(ProxyError::server_error(
                "failed to send proxy request message",
            ))?;

        let response = self
            .websocket_connection_request_records
            .get_response(req_id)
            .await
            .map_err(|e| match e {
                RpcError::Timeout => {
                    ProxyError::UserProgramError("program failed to respond before timeout".into())
                }
                _ => {
                    log::error!("error receiving websocket connection response: {:?}", e);
                    ProxyError::InternalError
                }
            })?;

        response
    }

    async fn forward_logs(self: Arc<Self>) {
        let mut log_stream = DOCKER.logs(
            &self.docker_id,
            Some(container::LogsOptions::<String> {
                follow: true,
                stdout: true,
                stderr: true,
                ..Default::default()
            }),
        );

        loop {
            tokio::select! {
                msg = log_stream.next() => {
                    match msg {
                        Some(Ok(msg)) => {
                            log::info!(
                                "[{}] [LOG]: {}",
                                self.display_id,
                                &msg.to_string().trim_end()
                            );
                        }
                        Some(Err(e)) => {
                            log::info!("[{}] Error reading logs: {e:?}", self.display_id);
                        }
                        None => {
                            log::info!("[{}] Log stream ended", self.display_id);
                            break;
                        }
                    }
                }
            }
        }
    }

    async fn handle_message(&self, msg: ContainerSent) -> Result<()> {
        match msg {
            ContainerSent::RpcResponse { req_id: id, result } => {
                let response = serde_json::from_str::<Result<Value, String>>(&result)
                    .expect("serde_json deserialize error");
                log::debug!("Got rpc response: {:#?}", response);
                log::debug!("Handling rpc response");
                if let Err(err) = self.rpc_records.handle_incoming(id, response) {
                    log::error!(
                        "[{}] Error handling rpc response: {err:#?}",
                        self.display_id
                    );
                };
            }
            ContainerSent::ProxyResponse { req_id, response } => {
                // log::debug!("Got rpc response: {:#?}", response);
                // log::debug!("Handling proxy response");
                if let Err(err) = self.proxy_records.handle_incoming(req_id, response) {
                    log::error!(
                        "[{}] Error handling proxy response: {err:#?}",
                        self.display_id
                    );
                };
            }
            ContainerSent::WebSocketConnectionResponse { req_id, response } => {
                let ws_id = match response {
                    Ok(ws_id) => ws_id,
                    Err(e) => {
                        if let Err(err) = self
                            .websocket_connection_request_records
                            .handle_incoming(req_id, Err(e))
                        {
                            log::error!(
                                "[{}] Error handling websocket connection response: {err:#?}",
                                self.display_id
                            );
                        };
                        return Ok(());
                    }
                };

                let connection =
                    WebSocketConnection::new(ws_id, self.stop_rx.clone(), self.commands_tx.clone());
                let container_tx = connection.container_tx.clone();

                self.websocket_connections
                    .write()
                    .await
                    .insert(ws_id, container_tx);

                if let Err(err) = self
                    .websocket_connection_request_records
                    .handle_incoming(req_id, Ok(connection))
                {
                    log::error!(
                        "[{}] Error handling websocket connection response: {err:#?}",
                        self.display_id
                    );
                };
            }
            ContainerSent::WebSocketMessage { ws_id: id, message } => {
                let _ = self
                    .websocket_connections
                    .read()
                    .await
                    .get(&id)
                    .ok_or_else(|| eyre!("no websocket connection with id {}", id))?
                    .send(WebSocketConnectionCommands::Send(message.into()))
                    .await;
            }
            _ => self.container_tx.send(msg).await?,
        }

        Ok(())
    }

    async fn run_event_loop(
        &self,
        container_ws_rx: &mut SplitStream<WebSocketStream<UnixStream>>,
        container_ws_tx: &mut SplitSink<WebSocketStream<UnixStream>, Message>,
        commands_rx: &mut mpsc::Receiver<ContainerCommands>,
    ) -> Result<()> {
        loop {
            tokio::select! {
                Some(msg) = container_ws_rx.next() => {
                    shutdown::update_last_activity();

                    match msg {
                        Ok(Message::Binary(bin)) => {
                            let msg = rmp_serde::from_slice::<ContainerSent>(&bin)?;

                            match &msg {
                                ContainerSent::PtyOutput { .. } => (),
                                msg => log::debug!("[{}] Got message: {msg:#?}", self.display_id)
                            }

                            self.handle_message(msg).await?;
                        }
                        Err(e) => {
                            log::error!("[{}] Container WebSocket unexpectedly hung up: {e:#?}", self.display_id);
                            break;
                        }
                        _ => log::warn!("[{}] Got unexpected message: {msg:#?}", self.display_id),
                    }
                }
                Some(msg) = commands_rx.recv() => {
                    match msg {
                        ContainerCommands::SendMessage(msg) => {
                            let bin = rmp_serde::to_vec_named(&msg).expect("rmp serde serialize error");
                            log::debug!("[{}] Sending message: {msg:#?}", self.display_id);
                            container_ws_tx.send(Message::Binary(bin)).await?;
                            log::debug!("[{}] Sent message: {msg:#?}", self.display_id);
                        }
                        ContainerCommands::RemoveWebSocketConnection(id) => {
                            self.websocket_connections.write().await.remove(&id);
                        }
                        ContainerCommands::Stop => {
                            log::info!("[{}]: 1,1. received stop command, exiting event loop", self.display_id);
                            break;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// This task is responsible for monitoring the container and
    /// killing it if it goes rogue
    async fn monitor(self: Arc<Self>) {
        use tokio::time;

        let mut stop_rx = self.on_stop();

        let max_size = Config::global().docker.disk_max_size as i64;

        loop {
            let stats = match DOCKER
                .inspect_container(
                    &self.docker_id,
                    Some(container::InspectContainerOptions { size: true }),
                )
                .await
            {
                Ok(stats) => stats,
                Err(_) => break,
            };

            if stats.size_rw.expect("size_rw should be Some") > max_size {
                log::warn!(
                    "[{}] Container is using too much disk space, killing it",
                    self.display_id
                );
                let _ = self.stop_tx.send(());
                break;
            }

            tokio::select! {
                _ = time::sleep(Duration::from_secs(1)) => {},
                _ = stop_rx.changed() => break,
            }
        }

        let _ = self.stop_tx.send(());
    }

    async fn run(
        self: Arc<Self>,
        ws_stream: WebSocketStream<UnixStream>,
        mut commands_rx: mpsc::Receiver<ContainerCommands>,
    ) -> Result<()> {
        // reading stop messages:
        // eg 1,0. message
        // first number - must be counted up to 5 starting at 1
        // second number - optional numbers

        let (mut container_ws_tx, mut container_ws_rx) = ws_stream.split();
        let logging_id = self.display_id.clone();

        // this event loop exits when stop is fired or when the container websocket closes
        if let Err(e) = self
            .run_event_loop(&mut container_ws_rx, &mut container_ws_tx, &mut commands_rx)
            .await
        {
            log::error!(
                "[{logging_id}] Error running container event loop: {:#?}. Cleaning up container.",
                e
            );
        }

        log::info!("[{logging_id}]: 2. broadcasting stop");
        // this notifies all other tasks
        let _ = self.stop_tx.send(());

        let mut ws_stream = container_ws_tx
            .reunite(container_ws_rx)
            .expect("container tx and rx do not match");
        let _ = ws_stream.close(None).await;

        log::info!("[{logging_id}]: 3. closed container websocket");

        fs::remove_dir_all(&self.socket_dir)
            .await
            .expect("Error removing socket dir");

        log::info!("[{logging_id}]: 4. removing container socket directory");

        let res = DOCKER.inspect_container(&self.docker_id, None).await?;
        let state = res.state.map(|state| state.status).flatten();

        if state
            .clone()
            .is_some_and(|state| state != ContainerStateStatusEnum::REMOVING)
        {
            log::info!(
                "[{logging_id}]: 5,0. removing docker container, state == {}",
                state.expect("state should be some here")
            );

            if let Err(err) = DOCKER
                .remove_container(
                    &self.docker_id,
                    Some(bollard::container::RemoveContainerOptions {
                        force: true,
                        ..Default::default()
                    }),
                )
                .await
            {
                log::warn!("5,0. error removing container: {:#?}", err);
            }
        }

        log::info!("[{logging_id}]: 5,1 removed docker container, exiting");

        if let Some(deletion_tx) = self.deletion_tx.clone() {
            // notify the deletion task that this container is done and deleted
            deletion_tx.send(self.display_id.clone()).await?;
        }

        Ok(())
    }
}

impl Drop for Container {
    fn drop(&mut self) {
        if self.stopped.load(Ordering::SeqCst) {
            return;
        }

        log::warn!(
            "Container {} dropped without being stopped first.",
            self.docker_id
        );
    }
}
