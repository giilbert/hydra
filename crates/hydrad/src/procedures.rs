use crate::{commands::Command, pty, state::State};
use portable_pty::CommandBuilder;
use serde_json::{json, Value};
use shared::{prelude::*, protocol::ContainerRpcRequest};
use std::{path::PathBuf, sync::Arc};
use tokio::{fs, sync::mpsc};

pub async fn handle_rpc_procedure(
    commands: &mpsc::Sender<Command>,
    req: ContainerRpcRequest,
    state: Arc<State>,
) -> Result<Result<Value, String>> {
    log::debug!("Got RPC: {:?}", req);

    match req {
        ContainerRpcRequest::PtyCreate { command, arguments } => {
            let mut cmd = CommandBuilder::new(command);
            for arg in arguments {
                cmd.arg(arg);
            }
            cmd.cwd("/playground");

            let pty = pty::create(cmd).await?;
            let id = state.register_pty(pty.commands_tx.clone());

            tokio::spawn(pty.send_output(commands.clone()));

            return Ok(Ok(json!(id)));
        }
        ContainerRpcRequest::PtyInput { pty_id, input } => {
            state
                .send_pty_command(pty_id, pty::PtyCommands::Input(pty::PtyInput::Text(input)))
                .await?;
            return Ok(Ok(json!({})));
        }
        ContainerRpcRequest::SetupFromOptions { options } => {
            let root_path = PathBuf::from("/playground/");

            for file in &options.files {
                let full_path = root_path.join(&file.path);
                // TODO: have better error handing for rpcs
                let parent = full_path.parent().expect("invalid path");
                fs::create_dir_all(parent).await?;
                fs::write(full_path, &file.content).await?;
            }

            Ok(Ok(().into()))
        }
        ContainerRpcRequest::Crash => {
            panic!("ContainerRpcRequest::Crash");
        }
    }
}
