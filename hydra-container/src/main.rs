mod commands;
mod procedures;
mod pty;
mod state;

use futures_util::{SinkExt, StreamExt};
use portable_pty::CommandBuilder;
use tokio::net::UnixStream;
use tokio_tungstenite::client_async;

use crate::commands::Commands;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    //     let mut command = CommandBuilder::new("python3");
    //     command.args(&["main.py"]);
    //     let mut pty = pty::create(command).await?;
    //     let mut out = pty.output.take().unwrap();

    //     while let Some(line) = out.recv().await {
    //         log::info!("stdout: {}", line);
    //     }

    let addr = "/run/hydra/conn.sock";
    log::info!("Connecting to {}..", addr);
    let stream = UnixStream::connect(addr).await?;
    let (ws_stream, _) = client_async("ws://localhost:0000", stream).await?;
    log::info!("Connected to {}", addr);

    let commands = Commands::new(ws_stream);
    commands.run().await?;

    Ok(())
}
