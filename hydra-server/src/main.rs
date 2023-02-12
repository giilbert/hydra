mod container;

use container::Container;
use protocol::{ContainerSent, HostSent};

use futures_util::{SinkExt, StreamExt};
use tokio::net::{unix::SocketAddr, UnixListener, UnixStream};
use tokio_tungstenite::accept_async;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    Container::new().await?;

    tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;

    Ok(())
}
