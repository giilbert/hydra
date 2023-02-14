use std::{collections::HashMap, sync::Arc};

use futures_util::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use parking_lot::{Mutex, RwLock};
use protocol::{ContainerSent, HostSent};
use tokio::{net::UnixStream, sync::mpsc};
use tokio_tungstenite::{tungstenite::Message, WebSocketStream};

use crate::{
    procedures::handle_rpc_procedure,
    pty::{Pty, PtyCommands},
    state::State,
};

#[derive(Debug)]
pub enum Command {
    Send(Message),
    RemovePty(u64),
}

pub struct Commands {
    ws_tx: SplitSink<WebSocketStream<UnixStream>, Message>,
    ws_rx: SplitStream<WebSocketStream<UnixStream>>,
    commands_rx: mpsc::Receiver<Command>,
    pub commands_tx: mpsc::Sender<Command>,
    state: Arc<Mutex<State>>,
}

impl Commands {
    pub fn new(ws: WebSocketStream<UnixStream>) -> Self {
        let (ws_tx, ws_rx) = ws.split();
        let (commands_tx, commands_rx) = mpsc::channel(32);

        Self {
            state: Arc::new(Mutex::new(State::new(commands_tx.clone()))),
            ws_tx,
            ws_rx,
            commands_rx,
            commands_tx,
        }
    }

    pub async fn run(mut self) -> anyhow::Result<()> {
        let state = self.state.clone();
        tokio::spawn(async move {
            while let Some(command) = self.commands_rx.recv().await {
                match command {
                    Command::Send(msg) => {
                        if let Err(e) = self.ws_tx.send(msg).await {
                            log::error!("Failed to send message: {}", e);
                        }
                    }
                    Command::RemovePty(id) => {
                        state.lock().remove_pty(id);
                    }
                }
            }
        });

        while let Some(Ok(Message::Binary(msg))) = self.ws_rx.next().await {
            let msg = rmp_serde::from_slice::<HostSent>(&msg);

            let msg = match msg {
                Ok(m) => m,
                Err(e) => {
                    log::error!("Failed to deserialize message: {}", e);
                    continue;
                }
            };

            match msg {
                HostSent::RpcRequest { id, req } => {
                    let res =
                        handle_rpc_procedure(&self.commands_tx, req, self.state.clone()).await;

                    let res = match res {
                        Ok(r) => r,
                        Err(e) => {
                            log::error!("Failed to handle RPC request: {}", e);
                            continue;
                        }
                    };

                    self.commands_tx
                        .send(Command::Send(Message::Binary(rmp_serde::to_vec_named(
                            &ContainerSent::RpcResponse {
                                id,
                                result: serde_json::to_string(&res)?,
                            },
                        )?)))
                        .await?;
                }
            }
        }

        Ok::<_, anyhow::Error>(())
    }
}
