mod commands;
mod procedures;
mod pty;
mod state;

use tokio::net::UnixStream;
use tokio_tungstenite::client_async;

use crate::commands::Commands;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    pretty_env_logger::init();

    let addr = "/run/hydra/conn.sock";
    log::info!("Connecting to {}", addr);
    let stream = UnixStream::connect(addr).await?;
    let (ws_stream, _) = client_async("ws://localhost:0000", stream).await?;
    log::info!("Connected to {}", addr);

    let commands = Commands::new(ws_stream);
    commands.run().await?;

    Ok(())
}
