use std::{path::PathBuf, sync::Arc};

use bollard::{container, service::HostConfig, Docker};
use futures_util::{SinkExt, StreamExt};
use lazy_static::lazy_static;
use parking_lot::Mutex;
use protocol::{ContainerRpcRequest, ContainerSent, HostSent};
use serde_json::Value;
use tokio::{
    fs,
    net::{UnixListener, UnixStream},
    sync::mpsc,
};
use tokio_tungstenite::{accept_async, tungstenite::Message, WebSocketStream};
use uuid::Uuid;

use crate::rpc::RpcRecords;

lazy_static! {
    static ref DOCKER: Docker = Docker::connect_with_local_defaults().unwrap();
}

pub struct Container {
    id: Uuid,
    commands_tx: mpsc::Sender<ContainerCommands>,
    socket_dir: PathBuf,
    rpc_records: Arc<Mutex<RpcRecords>>,
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
            .await
            .unwrap();

        log::info!("Created container: {}", res.id);

        DOCKER
            .start_container::<String>(&res.id, None)
            .await
            .unwrap();

        let ws_stream = match listener.accept().await {
            Ok((stream, _)) => accept_async(stream).await?,
            Err(e) => {
                log::error!("Error during WebSocket Connection: {}", e);
                return Err(e.into());
            }
        };

        let (commands_tx, commands_rx) = mpsc::channel::<ContainerCommands>(32);

        let rpc_records = Arc::new(Mutex::new(RpcRecords::new()));

        tokio::task::spawn(run_container(
            res.id,
            ws_stream,
            commands_rx,
            rpc_records.clone(),
        ));

        Ok(Self {
            id,
            commands_tx,
            socket_dir,
            rpc_records,
        })
    }

    pub async fn stop(&mut self) -> anyhow::Result<()> {
        self.commands_tx.send(ContainerCommands::Stop).await?;
        fs::remove_dir_all(&self.socket_dir).await?;

        Ok(())
    }

    pub async fn rpc(&mut self, req: ContainerRpcRequest) -> anyhow::Result<Result<Value, String>> {
        let id = Uuid::new_v4();
        let mut rpc_records = self.rpc_records.lock();

        self.commands_tx
            .send(ContainerCommands::SendMessage(HostSent::RpcRequest {
                id,
                req,
            }))
            .await?;

        let response = rpc_records.await_response(id).await?;

        drop(rpc_records);

        Ok(response.await?)
    }
}

async fn run_container(
    container_id: String,
    ws_stream: WebSocketStream<UnixStream>,
    mut commands_rx: mpsc::Receiver<ContainerCommands>,
    rpc_records: Arc<Mutex<RpcRecords>>,
) -> anyhow::Result<()> {
    let (mut tx, mut rx) = ws_stream.split();

    tokio::spawn(async move {
        let mut log_stream = DOCKER.logs(
            &container_id,
            Some(container::LogsOptions::<String> {
                follow: true,
                stdout: true,
                stderr: true,
                ..Default::default()
            }),
        );

        while let Some(Ok(msg)) = log_stream.next().await {
            log::info!("[{}]: {}", &container_id[..5], &msg);
        }
    });

    loop {
        tokio::select! {
            Some(msg) = rx.next() => {
                match msg {
                    Ok(msg) => {
                        match msg {
                            Message::Binary(bin) => {
                                let msg = rmp_serde::from_slice::<ContainerSent>(&bin).unwrap();

                                match msg {
                                    ContainerSent::RpcResponse { id, result } => {
                                        let response = serde_json::from_str::<Result<Value, String>>(&result).unwrap();
                                        let mut rpc_records = rpc_records.lock();
                                        if let Err(err) = rpc_records.handle_incoming(id, response) {
                                            log::error!("Error handling rpc response: {err:#?}");
                                        };
                                    },
                                    ContainerSent::PtyOutput { id, output } => {
                                        log::info!("pty output: [{id}] {output}");
                                    },
                                    _ => (),
                                }
                            },
                            msg => log::warn!("recv other than binary: {:?}", msg)
                        }
                    },
                    Err(e) => {
                        log::error!("Error during WebSocket Connection: {}", e);
                        break;
                    }
                }
            }
            Some(msg) = commands_rx.recv() => {
                match msg {
                    ContainerCommands::SendMessage(msg) => {
                        let bin = rmp_serde::to_vec_named(&msg).unwrap();
                        tx.send(Message::Binary(bin)).await?;
                    }
                    ContainerCommands::Stop => {
                        log::info!("Stopping");
                        break;
                    }
                }
            }
        }
    }

    let mut ws_stream = tx.reunite(rx)?;
    ws_stream.close(None).await?;

    Ok(())
}
