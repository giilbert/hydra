use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(tag = "type", content = "data")]
pub enum ContainerSent {
    Ping { timestamp: u64 },
    RpcResponse { id: Uuid, result: String },
    PtyOutput { id: u32, output: String },
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(tag = "type", content = "data")]
pub enum HostSent {
    RpcRequest { id: Uuid, req: ContainerRpcRequest },
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub enum ContainerRpcProcedure {
    SetupFromOptions,
    PtyCreate,
    PtyInput,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct ContainerRpcRequest {
    pub procedure: ContainerRpcProcedure,
    pub parameters: Value,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct ContainerRpcResponse {}

#[derive(Serialize, Deserialize)]
pub struct File {
    pub path: String,
    pub content: String,
}

#[derive(Serialize, Deserialize)]
pub struct ExecuteOptions {
    pub files: Vec<File>,
}
