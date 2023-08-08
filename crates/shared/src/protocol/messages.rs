use uuid::Uuid;

use crate::prelude::*;

use super::{ContainerProxyRequest, ContainerProxyResponse, ContainerRpcRequest, WebSocketMessage};

/// Commands that are sent from the container TO THE SERVER
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(tag = "type", content = "data")]
pub enum ContainerSent {
    Ping {
        timestamp: u64,
    },
    RpcResponse {
        req_id: Uuid,
        result: String,
    },
    PtyOutput {
        pty_id: u32,
        output: String,
    },
    PtyExit {
        pty_id: u32,
    },
    ProxyResponse {
        req_id: Uuid,
        response: Result<ContainerProxyResponse, String>,
    },
    WebSocketConnectionResponse {
        req_id: Uuid,
        ws_id: u32,
    },
    WebSocketMessage {
        ws_id: u32,
        message: WebSocketMessage,
    },
}

/// Commands that are sent from the server TO THE CONTAINER
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(tag = "type", content = "data")]
pub enum HostSent {
    RpcRequest {
        req_id: Uuid,
        req: ContainerRpcRequest,
    },
    ProxyHTTPRequest {
        req_id: Uuid,
        req: ContainerProxyRequest,
    },
    CreateWebSocketConnection {
        req_id: Uuid,
        req: ContainerProxyRequest,
    },
    WebSocketMessage {
        ws_id: u32,
        message: WebSocketMessage,
    },
}
