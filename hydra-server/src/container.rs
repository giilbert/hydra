use std::path::PathBuf;

use bollard::{container, service::HostConfig, Docker};
use futures_util::StreamExt;
use lazy_static::lazy_static;
use protocol::{ContainerSent, HostSent};
use tokio::{
    fs,
    net::{UnixListener, UnixStream},
    sync::mpsc,
};
use tokio_tungstenite::{
    accept_async,
    tungstenite::{Message, WebSocket},
    WebSocketStream,
};
use uuid::Uuid;

lazy_static! {
    static ref DOCKER: Docker = Docker::connect_with_local_defaults().unwrap();
}

pub struct Container {
    id: Uuid,
    commands_tx: mpsc::Sender<ContainerCommands>,
    socket_dir: PathBuf,
}

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

        tokio::task::spawn(run_container(ws_stream, commands_rx));

        Ok(Self {
            id,
            commands_tx,
            socket_dir,
        })
    }

    pub fn commands_tx(&self) -> mpsc::Sender<ContainerCommands> {
        self.commands_tx.clone()
    }
}

async fn run_container(
    ws_stream: WebSocketStream<UnixStream>,
    mut commands_rx: mpsc::Receiver<ContainerCommands>,
) -> anyhow::Result<()> {
    let (tx, mut rx) = ws_stream.split();

    loop {
        tokio::select! {
            Some(msg) = rx.next() => {
                match msg {
                    // Ok(msg) => log::info!("{msg:?}"),
                    Ok(msg) => {
                        match msg {
                            Message::Binary(bin) => {
                                let msg = rmp_serde::from_slice::<ContainerSent>(&bin).unwrap();
                                log::info!("recv: {:?}", msg);
                            },
                            msg => log::info!("recv other than binary: {:?}", msg)
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
                    ContainerCommands::SendMessage(msg) => log::info!("Sending: {:?}", msg),
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
