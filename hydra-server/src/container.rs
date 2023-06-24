use std::{path::PathBuf, sync::Arc};

use bollard::{container, service::HostConfig, Docker};
use futures_util::{SinkExt, StreamExt};
use lazy_static::lazy_static;
use protocol::{ContainerRpcRequest, ContainerSent, HostSent};
use serde_json::Value;
use tokio::{
    fs,
    net::{UnixListener, UnixStream},
    sync::{mpsc, watch, Mutex},
};
use tokio_tungstenite::{accept_async, tungstenite::Message, WebSocketStream};
use uuid::Uuid;

use crate::{rpc::RpcRecords, Environment};

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
    pub container_rx: Option<mpsc::Receiver<ContainerSent>>,
    pub stop: Arc<watch::Sender<()>>,
    /// An event that is fired when the container is stopped
    pub stop_rx: watch::Receiver<()>,
    pub stopped: bool,

    _id: Uuid,
    deletion_tx: Option<mpsc::Sender<String>>,
    commands_tx: mpsc::Sender<ContainerCommands>,
    /// Keeps track of RPC calls and is used for responses
    rpc_records: Arc<Mutex<RpcRecords>>,
}

#[derive(Clone, Debug)]
pub enum ContainerCommands {
    SendMessage(HostSent),
    Stop,
}

impl Container {
    pub async fn new(deletion_tx: Option<mpsc::Sender<String>>) -> anyhow::Result<Self> {
        let id = Uuid::new_v4();
        let run_dir = if Environment::get() == Environment::Production {
            PathBuf::from("/run/hydra")
        } else {
            PathBuf::from("/tmp/hydra")
        };
        let socket_dir = run_dir.join(format!("sockets/{}", id));
        let (stop, stop_rx) = watch::channel(());
        let stop = Arc::new(stop);

        fs::create_dir_all(&socket_dir).await?;

        let addr = format!("{}/conn.sock", socket_dir.to_string_lossy());
        let listener = UnixListener::bind(addr)?;

        let res = DOCKER
            .create_container(
                Some(container::CreateContainerOptions {
                    name: format!("hydra-container--tck-{id}"),
                    ..Default::default()
                }),
                container::Config {
                    image: Some("hydra-container"),
                    host_config: Some(HostConfig {
                        binds: Some(vec![format!(
                            "{}:/run/hydra",
                            run_dir.join(&socket_dir).to_string_lossy()
                        )]),
                        auto_remove: Some(true),
                        cpu_quota: Some(20000),
                        cpuset_cpus: Some("0-1".into()),
                        memory: Some(16 * 1000 * 1000),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            )
            .await?;

        let display_id = format!("dok-{}", &res.id[0..5]);

        log::info!("Created container: [{display_id}]");
        log::info!("Full ID: {}", res.id);

        DOCKER.start_container::<String>(&res.id, None).await?;

        let ws_stream = match listener.accept().await {
            Ok((stream, _)) => accept_async(stream).await?,
            Err(e) => {
                log::error!("[{display_id}] Error during initial WebSocket Connection: {e}",);
                fs::remove_dir_all(&socket_dir).await?;
                return Err(e.into());
            }
        };

        let (commands_tx, commands_rx) = mpsc::channel::<ContainerCommands>(32);

        let rpc_records = Arc::new(Mutex::new(RpcRecords::new()));
        let (container_tx, container_rx) = mpsc::channel::<ContainerSent>(64);

        tokio::task::spawn(run_container(
            res.id.clone(),
            ws_stream,
            commands_rx,
            rpc_records.clone(),
            container_tx,
            socket_dir.clone(),
            stop.clone(),
            stop_rx.clone(),
            deletion_tx.clone(),
        ));

        Ok(Self {
            _id: id,
            docker_id: res.id,
            display_id,
            deletion_tx,
            commands_tx,
            rpc_records,
            container_rx: Some(container_rx),
            stop,
            stop_rx,
            stopped: false,
        })
    }

    pub async fn stop(&mut self) -> anyhow::Result<()> {
        log::info!("[{}]: queued normal stop", self.display_id);
        self.commands_tx.send(ContainerCommands::Stop).await?;
        if let Some(deletion_tx) = &self.deletion_tx {
            deletion_tx.send(self.docker_id.clone()).await?;
        }
        self.stopped = true;
        Ok(())
    }

    pub async fn rpc(&mut self, req: ContainerRpcRequest) -> anyhow::Result<Result<Value, String>> {
        let id = Uuid::new_v4();

        log::debug!("[{}]: sent RPC request", self.display_id);
        self.commands_tx
            .send(ContainerCommands::SendMessage(HostSent::RpcRequest {
                id,
                req,
            }))
            .await?;

        log::debug!("[{}]: waiting for RPC response", self.display_id);
        let response = tokio::select! {
            d = self.rpc_records.lock().await.await_response(id).await? => d,
            _ = self.stop_rx.changed() => anyhow::bail!("container stopped during RPC")
        };
        let res = response?;

        log::debug!("[{}]: got RPC response", self.display_id);

        Ok(res)
    }
}

async fn run_container(
    container_id: String,
    ws_stream: WebSocketStream<UnixStream>,
    mut commands_rx: mpsc::Receiver<ContainerCommands>,
    rpc_records: Arc<Mutex<RpcRecords>>,
    container_tx: mpsc::Sender<ContainerSent>,
    socket_dir: PathBuf,
    stop: Arc<watch::Sender<()>>,
    mut stop_rx: watch::Receiver<()>,
    deletion_tx: Option<mpsc::Sender<String>>,
) -> anyhow::Result<()> {
    let (mut container_ws_tx, mut container_ws_rx) = ws_stream.split();
    let logging_id = format!("dok-{}", &container_id[0..5]);

    // Task to forward logs to the terminal
    let container_id_clone = container_id.clone();
    let logging_id_clone = logging_id.clone();
    tokio::spawn(async move {
        let mut log_stream = DOCKER.logs(
            &container_id_clone,
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
                        logging_id_clone,
                        &msg.to_string().trim_end()
                    );
                }
                _ = stop_rx.changed() => {
                    break;
                }
            }
        }
    });

    loop {
        tokio::select! {
            Some(msg) = container_ws_rx.next() => {
                match msg {
                    Ok(Message::Binary(bin)) => {
                        let msg = rmp_serde::from_slice::<ContainerSent>(&bin).expect("rmp_serde deserialize error");

                        match &msg {
                            ContainerSent::PtyOutput { .. } => (),
                            msg => log::debug!("[{logging_id}] Got message: {:#?}", msg)
                        }

                        match msg {
                            ContainerSent::RpcResponse { id, result } => {
                                let response = serde_json::from_str::<Result<Value, String>>(&result).expect("serde_json deserialize error");
                                log::debug!("Got rpc response: {:#?}", response);
                                let mut rpc_records = rpc_records.lock().await;
                                log::debug!("Handling rpc response");
                                if let Err(err) = rpc_records.handle_incoming(id, response) {
                                    log::error!("[{logging_id}] Error handling rpc response: {err:#?}");
                                };
                            },
                            _ => container_tx.send(msg).await?,
                        }
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
                    ContainerCommands::Stop => {
                        break;
                    }
                }
            }
        }
    }

    log::info!("[{logging_id}]: broadcasting stop");

    let _ = stop.send(());

    log::info!("[{logging_id}]: closing websocket");
    let mut ws_stream = container_ws_tx.reunite(container_ws_rx)?;
    let _ = ws_stream.close(None).await;

    fs::remove_dir_all(&socket_dir)
        .await
        .expect("Error removing socket dir");

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
        log::warn!(
            "[{logging_id}]: !!THIS MAY OR MAY NOT BE AN ERROR!! error removing container: {err}"
        );
    }

    if let Some(deletion_tx) = deletion_tx {
        deletion_tx.send(container_id).await?;
    }

    log::info!("[{logging_id}]: final stop");

    Ok(())
}
