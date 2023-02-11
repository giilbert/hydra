use protocol::{ContainerSent, HostSent};

use futures_util::{SinkExt, StreamExt};
use tokio::net::{unix::SocketAddr, UnixListener, UnixStream};
use tokio_tungstenite::accept_async;

async fn accept_connection(peer: SocketAddr, stream: UnixStream) {
    if let Err(e) = handle_connection(peer, stream).await {
        log::error!("Error during the websocket handshake occurred: {}", e);
    }
}

async fn handle_connection(peer: SocketAddr, stream: UnixStream) -> anyhow::Result<()> {
    let mut ws_stream = accept_async(stream).await.expect("Failed to accept");

    log::info!("New WebSocket connection: {:?}", peer);

    while let Some(msg) = ws_stream.next().await {
        let msg = msg?;
        if msg.is_text() || msg.is_binary() {
            ws_stream.send(msg).await?;
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let addr = "conn.sock";
    let listener = UnixListener::bind(addr)?;

    log::info!("Listening on {addr}");
    while let Ok((stream, peer_addr)) = listener.accept().await {
        log::info!("Accepted connection from: {:?}", peer_addr);

        tokio::spawn(accept_connection(peer_addr, stream));
    }

    Ok(())
}
