use protocol::{ContainerSent, HostSent};

use futures_util::{SinkExt, StreamExt};
use tokio::net::UnixStream;
use tokio_tungstenite::{client_async, tungstenite::Message};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let addr = if cfg!(debug_assertions) {
        "conn.sock"
    } else {
        "/run/hydra/conn.sock"
    };

    log::info!("Connecting to {addr}...");

    let unix_client = UnixStream::connect(addr).await?;
    let (ws_stream, _) = client_async("ws://localhost:0000", unix_client).await?;
    log::info!("Connected to {addr}");

    let (mut tx, mut rx) = ws_stream.split();

    let handle = tokio::task::spawn(async move {
        while let Some(msg) = rx.next().await {
            log::info!("recv: {:?}", msg);
        }
    });

    tx.send(Message::Binary(rmp_serde::to_vec_named(
        &ContainerSent::Ping {
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_millis() as u64,
        },
    )?))
    .await?;

    handle.await?;
    Ok(())
}
