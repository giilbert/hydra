mod commands;
mod procedures;
mod proxy;
mod pty;
mod state;

use crate::commands::Commands;
use shared::prelude::*;
use tokio::net::UnixStream;
use tokio_tungstenite::client_async;

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    pretty_env_logger::init();

    let addr = "/run/hydra/conn.sock";
    let stream = UnixStream::connect(addr).await?;
    let (ws_stream, _) = client_async("ws://localhost:0000", stream).await?;
    log::info!("hydrad successfully connected to {}", addr);

    let commands = Commands::new(ws_stream);
    commands.run().await?;

    Ok(())
}
