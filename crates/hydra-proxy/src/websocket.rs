use futures_util::{SinkExt, StreamExt};
use http::Request;
use hyper::{upgrade::Upgraded, HeaderMap};
use tokio_tungstenite::{connect_async, WebSocketStream};

pub async fn accept_websocket_connection(
    proxy_url: String,
    headers: HeaderMap,
    ws: WebSocketStream<Upgraded>,
) {
    let (mut client_tx, mut client_rx) = ws.split();

    // make a request to the proxy server with headers and uri
    // the proxy server will respond with a websocket connection
    let mut request = Request::builder()
        .method("GET")
        .uri(proxy_url.clone())
        .body(())
        .expect("request should be valid");
    *request.headers_mut() = headers;

    let connection = match connect_async(request).await {
        Ok((connection, _)) => connection,
        Err(e) => {
            log::warn!("error connecting to container server: {}", e);
            return;
        }
    };

    let (mut server_tx, mut server_rx) = connection.split();

    tokio::spawn(async move {
        loop {
            tokio::select! {
                message = client_rx.next() => {
                    match message {
                        Some(Ok(message)) => {
                            if let Err(e) = server_tx.send(message).await {
                                log::warn!("websocket client -> server forwarding error: {e:#?}");
                                break
                            }
                        }
                        Some(Err(e)) => {
                            log::warn!("websocket error: {e:#?}");
                            break;
                        }
                        None => break
                    }
                }
                message = server_rx.next() => {
                    match message {
                        Some(Ok(message)) => {
                            if let Err(e) = client_tx.send(message).await {
                                log::warn!("websocket server -> client forwarding error: {e:#?}");
                                break
                            }
                        }
                        Some(Err(e)) => {
                            log::warn!("websocket error: {e:#?}");
                            break
                        }
                        None => break
                    }
                }
            }
        }

        let mut ws = client_tx
            .reunite(client_rx)
            .expect("sink and stream should be from the same websocket");
        let _ = ws.close(None).await;
    });
}
