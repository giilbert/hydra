mod commands;
mod procedures;
mod pty;
mod state;

use crate::commands::Commands;
use color_eyre::Result;
use tokio::net::UnixStream;
use tokio_tungstenite::client_async;

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
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
