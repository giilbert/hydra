use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Commands that are sent from the container TO THE SERVER
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(tag = "type", content = "data")]
pub enum ContainerSent {
    Ping { timestamp: u64 },
    RpcResponse { id: Uuid, result: String },
    PtyOutput { id: u32, output: String },
    PtyExit { id: u32 },
}

/// Commands that are sent from the server TO THE CONTAINER
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
    Crash,
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
