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
#[serde(tag = "type", content = "data")]
pub enum ContainerRpcRequest {
    SetupFromOptions {
        files: Vec<File>,
    },
    PtyCreate {
        command: String,
        arguments: Vec<String>,
    },
    PtyInput {
        id: u32,
        input: String,
    },
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct ContainerRpcResponse {}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct File {
    pub path: String,
    pub content: String,
}

#[derive(Serialize, Deserialize)]
pub struct ExecuteOptions {
    pub files: Vec<File>,
}
