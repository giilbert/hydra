use crate::{
    config::Config,
    proxy_websockets::{
        WebSocketConnection, WebSocketConnectionCommands, WebSocketConnectionRequest,
    },
    rpc::RpcRecords,
    shutdown, Environment,
};
use bollard::{
    container,
    service::{ContainerStateStatusEnum, HostConfig},
    Docker,
};
use futures_util::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use lazy_static::lazy_static;
use serde_json::Value;
use shared::{
    prelude::*,
    protocol::{
        ContainerProxyRequest, ContainerProxyResponse, ContainerRpcRequest, ContainerSent,
        ExecuteOptions, HostSent,
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
    sync::{mpsc, watch, Mutex, RwLock},
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

    _id: Uuid,

    /// An event that is fired when the container is stopped
    stop_rx: watch::Receiver<()>,

    container_rx: Option<mpsc::Receiver<ContainerSent>>,
    deletion_tx: Option<mpsc::Sender<String>>,
    commands_tx: mpsc::Sender<ContainerCommands>,
    /// Keeps track of RPC calls and is used for responses
    rpc_records: Arc<Mutex<RpcRecords<Value>>>,
    /// Keeps track of proxy requests and is used for responses
    proxy_records: Arc<Mutex<RpcRecords<ContainerProxyResponse>>>,

    websocket_connection_request_records: Arc<Mutex<RpcRecords<WebSocketConnection>>>,
    websocket_connections: Arc<RwLock<HashMap<u32, mpsc::Sender<WebSocketConnectionCommands>>>>,
}

#[derive(Clone, Debug)]
pub enum ContainerCommands {
    SendMessage(HostSent),
    RemoveWebSocketConnection(u32),
    Stop,
}

impl Container {
    pub async fn new(deletion_tx: Option<mpsc::Sender<String>>) -> Result<Self> {
        let id = Uuid::new_v4();
        let hydra_run_dir = if Environment::get() == Environment::Production {
            PathBuf::from("/run/hydra")
        } else {
            PathBuf::from("/tmp/hydra")
        };
        let container_socket_dir = hydra_run_dir.join(format!("sockets/{}", id));
        let (stop_tx, stop_rx) = watch::channel(());

        fs::create_dir_all(&container_socket_dir).await?;

        let addr = format!("{}/conn.sock", container_socket_dir.to_string_lossy());
        let listener = UnixListener::bind(addr)?;

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
                        memory: Some(Config::global().docker.memory.try_into()?),
                        ..Default::default()
                    }),
                    env: Some(vec!["RUST_LOG=hydrad=info"]),
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

        // FIXME: error if container does not connect after some time
        let ws_stream = match listener.accept().await {
            Ok((stream, _)) => accept_async(stream).await?,
            Err(e) => {
                log::error!("[{display_id}] Error during initial WebSocket Connection: {e}",);
                fs::remove_dir_all(&container_socket_dir).await?;
                return Err(e.into());
            }
        };

        let (container_commands_tx, container_commands_rx) = mpsc::channel::<ContainerCommands>(32);

        let rpc_records = Arc::new(Mutex::new(RpcRecords::new()));
        let proxy_records = Arc::new(Mutex::new(RpcRecords::new()));
        let websocket_connection_request_records = Arc::new(Mutex::new(RpcRecords::new()));
        let websocket_connections = Arc::new(RwLock::new(HashMap::new()));

        let (container_message_tx, container_message_rx) = mpsc::channel::<ContainerSent>(64);

        tokio::spawn(Container::forward_logs(
            create_response.id.clone(),
            format!("dok-{}", &create_response.id[0..5]),
            stop_rx.clone(),
        ));
        tokio::spawn(Container::run(
            create_response.id.clone(),
            ws_stream,
            container_commands_rx,
            container_commands_tx.clone(),
            rpc_records.clone(),
            proxy_records.clone(),
            websocket_connection_request_records.clone(),
            websocket_connections.clone(),
            container_message_tx,
            container_socket_dir.clone(),
            stop_tx,
            stop_rx.clone(),
            deletion_tx.clone(),
        ));

        Ok(Self {
            _id: id,
            docker_id: create_response.id,
            display_id,
            deletion_tx,
            commands_tx: container_commands_tx,
            rpc_records,
            proxy_records,
            socket_dir: container_socket_dir,
            container_rx: Some(container_message_rx),
            stop_rx,
            stopped: false.into(),
            websocket_connection_request_records,
            websocket_connections,
        })
    }

    pub fn listen(&mut self) -> Result<mpsc::Receiver<ContainerSent>> {
        self.container_rx
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
                id,
                req,
            }))
            .await?;

        log::debug!("[{}]: waiting for RPC response", self.display_id);
        let await_response = self.rpc_records.lock().await.await_response(id)?;
        let mut stop_rx_clone = self.stop_rx.clone();
        let response = tokio::select! {
            d = await_response => d,
            _ = tokio::time::sleep(Duration::from_secs(10)) => bail!("container failed to respond to RPC in 10 seconds"),
            _ = stop_rx_clone.changed() => bail!("container stopped during RPC")
        };
        let res = response?;

        log::debug!("[{}]: got RPC response", self.display_id);

        Ok(res)
    }

    pub async fn rpc_setup_from_options(&self, options: ExecuteOptions) -> Result<()> {
        self.rpc(ContainerRpcRequest::SetupFromOptions {
            files: options.files,
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
        self.rpc(ContainerRpcRequest::PtyInput { id: pty_id, input })
            .await?
            .map_err(|e| eyre!("error inputting: {:?}", e))?;

        Ok(())
    }

    pub async fn proxy_request(
        &self,
        req: ContainerProxyRequest,
    ) -> Result<ContainerProxyResponse> {
        let id = Uuid::new_v4();

        log::debug!("[{}]: sent proxy request", self.display_id);
        self.commands_tx
            .send(ContainerCommands::SendMessage(HostSent::ProxyHTTPRequest(
                id, req,
            )))
            .await?;

        log::debug!("[{}]: waiting for proxy response", self.display_id);
        let await_response = self.proxy_records.lock().await.await_response(id)?;
        let mut stop_rx_clone = self.stop_rx.clone();
        let response = tokio::select! {
            d = await_response => d,
            _ = tokio::time::sleep(Duration::from_secs(10)) => bail!("container failed to respond to RPC in 10 seconds"),
            _ = stop_rx_clone.changed() => bail!("container stopped during RPC")
        };
        let res = response?;

        log::debug!("[{}]: got proxy response", self.display_id);

        res.map_err(|e| eyre!("container failed to proxy: {:?}", e))
    }

    pub async fn create_websocket_connection(
        &self,
        req: ContainerProxyRequest,
    ) -> Result<WebSocketConnection> {
        let id = Uuid::new_v4();

        self.commands_tx
            .send(ContainerCommands::SendMessage(
                HostSent::CreateWebSocketConnection(id, req),
            ))
            .await?;

        let await_response = self
            .websocket_connection_request_records
            .lock()
            .await
            .await_response(id)?;

        let mut stop_rx_clone = self.stop_rx.clone();
        let response = tokio::select! {
            d = await_response => d,
            _ = tokio::time::sleep(Duration::from_secs(10)) => bail!("container failed to respond to RPC in 10 seconds"),
            _ = stop_rx_clone.changed() => bail!("container stopped during RPC")
        };
        let res = response?;

        res.map_err(|e| eyre!("container failed to create websocket connection: {:?}", e))
    }

    async fn forward_logs(container_id: String, logging_id: String, mut stop_rx: StopRx) {
        let mut log_stream = DOCKER.logs(
            &container_id,
            Some(container::LogsOptions::<String> {
                follow: true,
                stdout: true,
                stderr: true,
                ..Default::default()
            }),
        );

        loop {
            tokio::select! {
                Some(Ok(msg)) = log_stream.next() => {
                    log::info!(
                        "[{}] [LOG]: {}",
                        logging_id,
                        &msg.to_string().trim_end()
                    );
                }
                _ = stop_rx.changed() => {
                    break;
                }
            }
        }
    }

    async fn handle_message(
        msg: ContainerSent,
        commands: &mpsc::Sender<ContainerCommands>,
        stop_rx: &StopRx,
        rpc_records: &Mutex<RpcRecords<Value>>,
        proxy_records: &Mutex<RpcRecords<ContainerProxyResponse>>,
        websocket_connection_records: &Mutex<RpcRecords<WebSocketConnection>>,
        websocket_connections: &RwLock<HashMap<u32, mpsc::Sender<WebSocketConnectionCommands>>>,
        container_tx: &mpsc::Sender<ContainerSent>,
        logging_id: &str,
    ) -> Result<()> {
        match msg {
            ContainerSent::RpcResponse { id, result } => {
                let response = serde_json::from_str::<Result<Value, String>>(&result)
                    .expect("serde_json deserialize error");
                log::debug!("Got rpc response: {:#?}", response);
                let mut rpc_records = rpc_records.lock().await;
                log::debug!("Handling rpc response");
                if let Err(err) = rpc_records.handle_incoming(id, response) {
                    log::error!("[{logging_id}] Error handling rpc response: {err:#?}");
                };
            }
            ContainerSent::ProxyResponse { req_id, response } => {
                // log::debug!("Got rpc response: {:#?}", response);
                let mut proxy_records = proxy_records.lock().await;
                // log::debug!("Handling proxy response");
                if let Err(err) = proxy_records.handle_incoming(req_id, response) {
                    log::error!("[{logging_id}] Error handling proxy response: {err:#?}");
                };
            }
            ContainerSent::WebSocketConnectionResponse { req_id, id } => {
                let connection = WebSocketConnection::new(id, stop_rx.clone(), commands.clone());
                let container_tx = connection.container_tx.clone();

                websocket_connections.write().await.insert(id, container_tx);
                let mut websocket_connection_records = websocket_connection_records.lock().await;
                if let Err(err) =
                    websocket_connection_records.handle_incoming(req_id, Ok(connection))
                {
                    log::error!(
                        "[{logging_id}] Error handling websocket connection response: {err:#?}"
                    );
                };
            }
            ContainerSent::WebSocketMessage { id, message } => {
                let _ = websocket_connections
                    .read()
                    .await
                    .get(&id)
                    .ok_or_else(|| eyre!("no websocket connection with id {}", id))?
                    .send(WebSocketConnectionCommands::Send(message.into()))
                    .await;
            }
            _ => container_tx.send(msg).await?,
        }

        Ok(())
    }

    async fn run_event_loop(
        container_ws_rx: &mut SplitStream<WebSocketStream<UnixStream>>,
        container_ws_tx: &mut SplitSink<WebSocketStream<UnixStream>, Message>,

        rpc_records: Arc<Mutex<RpcRecords<Value>>>,
        proxy_records: Arc<Mutex<RpcRecords<ContainerProxyResponse>>>,
        websocket_connection_records: Arc<Mutex<RpcRecords<WebSocketConnection>>>,
        websocket_connections: Arc<RwLock<HashMap<u32, mpsc::Sender<WebSocketConnectionCommands>>>>,

        commands_rx: &mut mpsc::Receiver<ContainerCommands>,
        commands_tx: &mpsc::Sender<ContainerCommands>,

        container_tx: &mpsc::Sender<ContainerSent>,
        stop_rx: StopRx,

        logging_id: &str,
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
                                msg => log::debug!("[{logging_id}] Got message: {:#?}", msg)
                            }

                            Container::handle_message(
                                msg,
                                &commands_tx,
                                &stop_rx,
                                &rpc_records,
                                &proxy_records,
                                &websocket_connection_records,
                                &websocket_connections,
                                &container_tx,
                                &logging_id,
                            )
                            .await?;
                        }
                        Err(e) => {
                            log::error!("[{logging_id}] Container WebSocket unexpectedly hung up: {}", e);
                            break;
                        }
                        _ => log::warn!("[{logging_id}] Got unexpected message: {:#?}", msg)
                    }
                }
                Some(msg) = commands_rx.recv() => {
                    match msg {
                        ContainerCommands::SendMessage(msg) => {
                            let bin = rmp_serde::to_vec_named(&msg).expect("rmp serde serialize error");
                            log::debug!("[{logging_id}] Sending message: {:#?}", msg);
                            container_ws_tx.send(Message::Binary(bin)).await?;
                            log::debug!("[{logging_id}] Sent message: {:#?}", msg);
                        }
                        ContainerCommands::RemoveWebSocketConnection(id) => {
                            websocket_connections.write().await.remove(&id);
                        }
                        ContainerCommands::Stop => {
                            log::info!("[{logging_id}]: 1,1. received stop command, exiting event loop");
                            break;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    async fn run(
        container_id: String,
        ws_stream: WebSocketStream<UnixStream>,
        mut commands_rx: mpsc::Receiver<ContainerCommands>,
        commands_tx: mpsc::Sender<ContainerCommands>,
        rpc_records: Arc<Mutex<RpcRecords<Value>>>,
        proxy_records: Arc<Mutex<RpcRecords<ContainerProxyResponse>>>,
        websocket_connection_records: Arc<Mutex<RpcRecords<WebSocketConnection>>>,
        websocket_connections: Arc<RwLock<HashMap<u32, mpsc::Sender<WebSocketConnectionCommands>>>>,
        container_tx: mpsc::Sender<ContainerSent>,
        socket_dir: PathBuf,
        stop_tx: watch::Sender<()>,
        stop_rx: StopRx,
        deletion_tx: Option<mpsc::Sender<String>>,
    ) -> Result<()> {
        // reading stop messages:
        // eg 1,0. message
        // first number - must be counted up to 5 starting at 1
        // second number - optional numbers

        let (mut container_ws_tx, mut container_ws_rx) = ws_stream.split();
        let logging_id = format!("dok-{}", &container_id[0..5]);

        // this event loop exits when stop is fired or when the container websocket closes
        if let Err(e) = Container::run_event_loop(
            &mut container_ws_rx,
            &mut container_ws_tx,
            rpc_records,
            proxy_records,
            websocket_connection_records,
            websocket_connections,
            &mut commands_rx,
            &commands_tx,
            &container_tx,
            stop_rx,
            &logging_id,
        )
        .await
        {
            log::error!(
                "[{logging_id}] Error running container event loop: {:#?}. Cleaning up container.",
                e
            );
        }

        log::info!("[{logging_id}]: 2. broadcasting stop");
        // this notifies all other tasks
        let _ = stop_tx.send(());

        let mut ws_stream = container_ws_tx
            .reunite(container_ws_rx)
            .expect("container tx and rx do not match");
        let _ = ws_stream.close(None).await;

        log::info!("[{logging_id}]: 3. closed container websocket");

        fs::remove_dir_all(&socket_dir)
            .await
            .expect("Error removing socket dir");

        log::info!("[{logging_id}]: 4. removing container socket directory");

        let res = DOCKER.inspect_container(&container_id, None).await?;
        let state = res.state.map(|state| state.status).flatten();

        if state
            .clone()
            .is_some_and(|state| state != ContainerStateStatusEnum::REMOVING)
        {
            log::info!(
                "[{logging_id}]: 5,0. removing docker container, state == {}",
                state.unwrap()
            );

            if let Err(err) = DOCKER
                .remove_container(
                    &container_id,
                    Some(bollard::container::RemoveContainerOptions {
                        force: true,
                        ..Default::default()
                    }),
                )
                .await
            {
                log::error!("5,0. error removing container: {:#?}", err);
            }
        }

        log::info!("[{logging_id}]: 5,1 removed docker container, exiting");

        if let Some(deletion_tx) = deletion_tx {
            // notify the deletion task that this container is done and deleted
            deletion_tx.send(container_id).await?;
        }

        Ok(())
    }
}
