use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(tag = "type", content = "data")]
pub enum ContainerSent {
    Ping { timestamp: u64 },
    RpcResponse { id: Uuid, result: String },
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(tag = "type", content = "data")]
pub enum HostSent {
    RpcRequest { id: Uuid, req: ContainerRpcRequest },
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub enum ContainerRpcProcedure {
    Test,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct ContainerRpcRequest {
    pub procedure: ContainerRpcProcedure,
    pub parameters: Value,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct ContainerRpcResponse {}
