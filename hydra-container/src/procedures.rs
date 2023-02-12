use protocol::ContainerRpcRequest;
use serde_json::{json, Value};

pub fn handle_rpc_procedure(req: ContainerRpcRequest) -> anyhow::Result<Result<Value, String>> {
    log::info!("Handling RPC procedure: {:?}", req);

    Ok(Ok(json!({
        "hello": "world"
    })))
}
