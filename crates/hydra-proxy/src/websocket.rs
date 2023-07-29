use futures_util::{SinkExt, StreamExt};
use http::Request;
use hyper::{upgrade::Upgraded, HeaderMap, Uri};
use tokio_tungstenite::{connect_async, tungstenite::Message, WebSocketStream};

pub async fn accept_websocket_connection(
    proxy_url: String,
    headers: HeaderMap,
    ws: WebSocketStream<Upgraded>,
) {
    let (mut tx, mut rx) = ws.split();

    log::info!("accepted websocket connection {}", proxy_url);

    // make a request to the proxy server with headers and uri
    // the proxy server will respond with a websocket connection
    let mut request = Request::builder()
        .method("GET")
        .uri(proxy_url.clone())
        .body(())
        .unwrap();
    *request.headers_mut() = headers;

    let (connection, _) = connect_async(request)
        .await
        .map_err(|e| {
            log::error!("{}", e.to_string());
        })
        .unwrap();

    log::info!("accepted websocket connection");

    let (mut server_tx, mut server_rx) = connection.split();

    tokio::spawn(async move {
        loop {
            tokio::select! {
                message = rx.next() => {
                    match message {
                        Some(Ok(message)) => {
                            // log::info!("received message from client: {:?}", message);
                            server_tx.send(message).await.unwrap();
                        }
                        Some(Err(e)) => {
                            log::error!("websocket error: {}", e);
                            break;
                        }
                        None => {
                            log::info!("client closed connection");
                            break;
                        }
                    }
                }
                message = server_rx.next() => {
                    match message {
                        Some(Ok(message)) => {
                            // log::info!("received message from server: {:?}", message);
                            tx.send(message).await.unwrap();
                        }
                        Some(Err(e)) => {
                            log::error!("websocket error: {}", e);
                            break;
                        }
                        None => {
                            log::info!("server closed connection");
                            break;
                        }
                    }
                }
            }
        }

        let mut ws = tx.reunite(rx).unwrap();
        let _ = ws.close(None).await;
    });
}
