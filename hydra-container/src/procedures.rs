use std::{path::PathBuf, sync::Arc};

use parking_lot::Mutex;
use portable_pty::CommandBuilder;
use protocol::{ContainerRpcProcedure, ContainerRpcRequest, ExecuteOptions};
use serde_json::{json, Value};
use tokio::{fs, sync::mpsc};

use crate::{commands::Command, pty, state::State};

pub async fn handle_rpc_procedure(
    commands: &mpsc::Sender<Command>,
    req: ContainerRpcRequest,
    state: Arc<Mutex<State>>,
) -> anyhow::Result<Result<Value, String>> {
    let ContainerRpcRequest {
        ref procedure,
        ref parameters,
    } = req;

    match procedure {
        ContainerRpcProcedure::PtyCreate => {
            let command = parameters["command"].as_str().unwrap();
            let arguments = parameters["arguments"].as_array().unwrap();
            let mut cmd = CommandBuilder::new(command);
            for arg in arguments {
                cmd.arg(arg.as_str().unwrap());
            }
            let pty = pty::create(cmd).await?;
            let id = state.lock().register_pty(pty.commands_tx.clone());

            tokio::spawn(pty.send_output(commands.clone()));

            return Ok(Ok(json!({ "id": id })));
        }
        ContainerRpcProcedure::PtyInput => {
            let id = parameters["id"].as_u64().unwrap();
            let input = parameters["input"].as_str().unwrap();

            let mut state = state.lock();
            let pty = state.get_pty(id).unwrap();

            pty.send(pty::PtyCommands::Input(pty::PtyInput::Text(
                input.to_string(),
            )))
            .await?;

            return Ok(Ok(json!({})));
        }
        ContainerRpcProcedure::SetupFromOptions => {
            log::info!("SetupFromOptions: {:?}", parameters);

            let root_path = PathBuf::from("/root/");
            let options = serde_json::from_value::<ExecuteOptions>(parameters.clone())?;
            for file in options.files {
                let path = file.path.clone();
                let content = file.content.clone();

                fs::write(root_path.join(path), content).await?;
            }

            Ok(Ok(().into()))
        }
    }
}
