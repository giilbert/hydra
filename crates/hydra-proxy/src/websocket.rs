use futures_util::{SinkExt, StreamExt};
use hyper::upgrade::Upgraded;
use tokio_tungstenite::{tungstenite::Message, WebSocketStream};

pub async fn accept_websocket_connection(ws: WebSocketStream<Upgraded>) {
    let (mut tx, rx) = ws.split();

    log::info!("accepted websocket connection");

    tx.send(Message::Text("Hello!".into())).await.unwrap();

    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    let mut ws = tx.reunite(rx).unwrap();
    ws.close(None).await.unwrap();
}
