use std::{path::PathBuf, sync::Arc};

use portable_pty::CommandBuilder;
use protocol::ContainerRpcRequest;
use serde_json::{json, Value};
use tokio::{
    fs,
    sync::{mpsc, Mutex},
};

use crate::{commands::Command, pty, state::State};

pub async fn handle_rpc_procedure(
    commands: &mpsc::Sender<Command>,
    req: ContainerRpcRequest,
    state: Arc<Mutex<State>>,
) -> anyhow::Result<Result<Value, String>> {
    log::debug!("Got RPC: {:?}", req);

    match req {
        ContainerRpcRequest::PtyCreate { command, arguments } => {
            let mut cmd = CommandBuilder::new(command);
            for arg in arguments {
                cmd.arg(arg);
            }

            let pty = pty::create(cmd).await?;
            let id = state.lock().await.register_pty(pty.commands_tx.clone());

            tokio::spawn(pty.send_output(commands.clone()));

            return Ok(Ok(json!({ "id": id })));
        }
        ContainerRpcRequest::PtyInput { id, input } => {
            // panic!();
            let mut state = state.lock().await;
            let pty = state
                .get_pty(id)
                .ok_or_else(|| anyhow::anyhow!("cannot find pty"))?;

            pty.send(pty::PtyCommands::Input(pty::PtyInput::Text(
                input.to_string(),
            )))
            .await?;

            return Ok(Ok(json!({})));
        }
        ContainerRpcRequest::SetupFromOptions { files } => {
            let root_path = PathBuf::from("/root/");

            for file in files {
                let path = file.path.clone();
                let content = file.content.clone();

                fs::write(root_path.join(path), content).await?;
            }

            Ok(Ok(().into()))
        }
        ContainerRpcRequest::Crash => {
            panic!("ContainerRpcRequest::Crash");
        }
    }
}
