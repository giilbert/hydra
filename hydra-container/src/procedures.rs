use std::sync::Arc;

use parking_lot::Mutex;
use portable_pty::CommandBuilder;
use protocol::{ContainerRpcProcedure, ContainerRpcRequest};
use serde_json::{json, Value};
use tokio::sync::mpsc;

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
            let cmd = CommandBuilder::new("bash");
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
    }
}
