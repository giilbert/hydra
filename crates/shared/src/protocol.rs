use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Commands that are sent from the container TO THE SERVER
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(tag = "type", content = "data")]
pub enum ContainerSent {
    Ping {
        timestamp: u64,
    },
    RpcResponse {
        id: Uuid,
        result: String,
    },
    PtyOutput {
        id: u32,
        output: String,
    },
    PtyExit {
        id: u32,
    },
    ProxyResponse {
        req_id: Uuid,
        response: Result<ContainerProxyResponse, String>,
    },
}

/// Commands that are sent from the server TO THE CONTAINER
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(tag = "type", content = "data")]
pub enum HostSent {
    RpcRequest { id: Uuid, req: ContainerRpcRequest },
    ProxyHTTPRequest(Uuid, ContainerProxyRequest),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ContainerProxyRequest {
    pub method: String,
    pub uri: String,
    pub port: u32,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ContainerProxyResponse {
    pub status_code: u16,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}
