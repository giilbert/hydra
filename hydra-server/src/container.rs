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

use crate::rpc::RpcRecords;

lazy_static! {
    static ref DOCKER: Docker =
        Docker::connect_with_local_defaults().expect("unable to connect to docker");
}

#[derive(Debug)]
pub struct Container {
    _id: Uuid,
    pub docker_id: String,
    commands_tx: mpsc::Sender<ContainerCommands>,
    pub container_rx: Option<mpsc::Receiver<ContainerSent>>,
    socket_dir: PathBuf,
    rpc_records: Arc<Mutex<RpcRecords>>,
    pub stop_rx: watch::Receiver<()>,
    pub stopped: bool,
}

#[derive(Clone, Debug)]
pub enum ContainerCommands {
    SendMessage(HostSent),
    Stop,
}

impl Container {
    pub async fn new() -> anyhow::Result<Self> {
        let id = Uuid::new_v4();
        let cwd = std::env::current_dir()?;
        let socket_dir = cwd.join(format!("sockets/{}", id));
        let (stop, stop_rx) = watch::channel(());

        fs::create_dir_all(&socket_dir).await?;

        let addr = format!("{}/conn.sock", socket_dir.to_string_lossy());
        let listener = UnixListener::bind(addr)?;

        let res = DOCKER
            .create_container(
                Some(container::CreateContainerOptions {
                    name: format!("hydra-container-{id}"),
                    ..Default::default()
                }),
                container::Config {
                    image: Some("hydra-container"),
                    host_config: Some(HostConfig {
                        binds: Some(vec![format!("{}:/run/hydra", socket_dir.to_string_lossy())]),
                        auto_remove: Some(true),
                        cpu_quota: Some(20000),
                        cpuset_cpus: Some("0-1".into()),
                        memory: Some(64 * 1000 * 1000),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            )
            .await?;

        log::info!("Created container: {}", res.id);

        DOCKER.start_container::<String>(&res.id, None).await?;

        let ws_stream = match listener.accept().await {
            Ok((stream, _)) => accept_async(stream).await?,
            Err(e) => {
                log::error!("Error during initial WebSocket Connection: {}", e);
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
            stop,
            stop_rx.clone(),
        ));

        Ok(Self {
            _id: id,
            docker_id: res.id,
            commands_tx,
            socket_dir,
            rpc_records,
            container_rx: Some(container_rx),
            stop_rx,
            stopped: false,
        })
    }

    pub async fn stop(&mut self) -> anyhow::Result<()> {
        log::info!("Container {}: queued normal stop", &self.docker_id[..5]);
        self.commands_tx.send(ContainerCommands::Stop).await?;
        self.stopped = true;
        Ok(())
    }

    pub async fn rpc(&mut self, req: ContainerRpcRequest) -> anyhow::Result<Result<Value, String>> {
        let id = Uuid::new_v4();

        self.commands_tx
            .send(ContainerCommands::SendMessage(HostSent::RpcRequest {
                id,
                req,
            }))
            .await?;

        let response = tokio::select! {
            d = self.rpc_records.lock().await.await_response(id).await? => d,
            _ = self.stop_rx.changed() => anyhow::bail!("container stopped during RPC")
        };
        let res = response?;

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
    stop: watch::Sender<()>,
    mut stop_rx: watch::Receiver<()>,
) -> anyhow::Result<()> {
    let (mut tx, mut rx) = ws_stream.split();

    let container_id_clone = container_id.clone();
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
                        "[{}]: {}",
                        &container_id_clone[..5],
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
            Some(msg) = rx.next() => {
                match msg {
                    Ok(msg) => {
                        match msg {
                            Message::Binary(bin) => {
                                let msg = rmp_serde::from_slice::<ContainerSent>(&bin).expect("rmp_serde deserialize error");

                                match msg {
                                    ContainerSent::RpcResponse { id, result } => {
                                        let response = serde_json::from_str::<Result<Value, String>>(&result).expect("serde_json deserialize error");
                                        let mut rpc_records = rpc_records.lock().await;
                                        if let Err(err) = rpc_records.handle_incoming(id, response) {
                                            log::error!("Error handling rpc response: {err:#?}");
                                        };
                                    },
                                    _ => container_tx.send(msg).await?,
                                }
                            },
                            msg => log::warn!("recv other than binary: {:?}", msg)
                        }
                    },
                    Err(e) => {
                        log::error!("Error during container WebSocket Connection: {}", e);
                        break;
                    }
                }
            }
            Some(msg) = commands_rx.recv() => {
                match msg {
                    ContainerCommands::SendMessage(msg) => {
                        let bin = rmp_serde::to_vec_named(&msg).expect("rmp serde serialize error");
                        tx.send(Message::Binary(bin)).await?;
                    }
                    ContainerCommands::Stop => {
                        break;
                    }
                }
            }
        }
    }

    log::info!("Container {}: closing", &container_id[..5]);
    let mut ws_stream = tx.reunite(rx)?;

    let _ = ws_stream.close(None).await;

    fs::remove_dir_all(&socket_dir)
        .await
        .expect("Error removing socket dir");

    DOCKER
        .remove_container(
            &container_id,
            Some(bollard::container::RemoveContainerOptions {
                force: true,
                ..Default::default()
            }),
        )
        .await
        .expect("error deleting container");

    stop.send(()).expect("Error broadcasting stop");

    log::info!("Container {}: stopped", &container_id[..5]);

    Ok(())
}
