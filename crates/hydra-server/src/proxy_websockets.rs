use crate::container::{ContainerCommands, StopRx};
use shared::protocol::{ContainerProxyRequest, HostSent, WebSocketMessage};
use tokio::sync::{mpsc, oneshot};

#[derive(Debug)]
pub struct WebSocketConnection {
    pub id: u32,
    // forwarded to the client
    pub rx: mpsc::Receiver<WebSocketMessage>,
    pub container_tx: mpsc::Sender<WebSocketConnectionCommands>,

    // received from the client
    pub tx: mpsc::Sender<WebSocketMessage>,
}

impl WebSocketConnection {
    pub fn new(
        id: u32,
        mut stop_rx: StopRx,
        container_commands: mpsc::Sender<ContainerCommands>,
    ) -> Self {
        let (client_tx, rx) = mpsc::channel::<WebSocketMessage>(256);
        let (tx, mut client_rx) = mpsc::channel::<WebSocketMessage>(256);
        let (container_tx, mut container_rx) = mpsc::channel(64);

        let tx_clone = client_tx.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    // forwards messages from the container to the client
                    command = container_rx.recv() => {
                        match command {
                            Some(WebSocketConnectionCommands::Send(data)) => {
                                tx_clone.send(data.into()).await.ok();
                            }
                            // Some(WebSocketConnectionCommands::Close) => {
                            //     break;
                            // }
                            None => {
                                break;
                            }
                        }
                    }
                    // forwards messages from the client to the container
                    message = client_rx.recv() => {
                        match message {
                            Some(message) => {
                                if let Err(e) = container_commands
                                    .send(ContainerCommands::SendMessage(HostSent::WebSocketMessage {
                                        id,
                                        message: WebSocketMessage::from(message),
                                    }))
                                    .await
                                {
                                    log::warn!("client -> container send websocket error: {e:#?}");
                                    break;
                                }
                            }
                            None => {
                                break;
                            }
                        }
                    }
                    _ = stop_rx.changed() => {
                        break;
                    }
                }
            }

            let _ = container_commands
                .send(ContainerCommands::RemoveWebSocketConnection(id))
                .await;
        });

        Self {
            id,
            rx,
            tx,
            container_tx,
        }
    }
}

pub enum WebSocketConnectionCommands {
    Send(WebSocketMessage),
    // Close,
}

#[derive(Debug)]
pub struct WebSocketConnectionRequest {
    pub proxy_request: ContainerProxyRequest,
    pub tx: oneshot::Sender<WebSocketConnection>,
}

impl WebSocketConnectionRequest {
    pub fn new(
        proxy_request: ContainerProxyRequest,
    ) -> (Self, oneshot::Receiver<WebSocketConnection>) {
        let (tx, rx) = oneshot::channel();
        (Self { proxy_request, tx }, rx)
    }
}
