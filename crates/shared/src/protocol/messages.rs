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
    WebSocketConnectionResponse {
        req_id: Uuid,
        id: u32,
    },
    WebSocketMessage {
        id: u32,
        message: WebSocketMessage,
    },
}

/// Commands that are sent from the server TO THE CONTAINER
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(tag = "type", content = "data")]
pub enum HostSent {
    RpcRequest { id: Uuid, req: ContainerRpcRequest },
    ProxyHTTPRequest(Uuid, ContainerProxyRequest),
    CreateWebSocketConnection(Uuid, ContainerProxyRequest),
    WebSocketMessage { id: u32, message: WebSocketMessage },
}
