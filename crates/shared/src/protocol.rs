use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio_tungstenite::tungstenite::{protocol::CloseFrame, Message};
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
    WebSocketConnectionResponse {
        req_id: Uuid,
        id: u32,
    },
    WebSocketMessage {
        id: u32,
        message: WebSocketMessage,
    },
}

/// This wrapper is needed because the tungstenite Message enum does not implement Serialize
/// and we need to send it over the wire. This also provides a way to convert between axum
/// and tungstenite messages.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum WebSocketMessage {
    Text(String),
    Binary(Vec<u8>),
    Ping(Vec<u8>),
    Pong(Vec<u8>),
    Close(Option<(u16, String)>),
}

impl From<WebSocketMessage> for Message {
    fn from(value: WebSocketMessage) -> Message {
        use WebSocketMessage::*;

        match value {
            Text(t) => Message::Text(t),
            Binary(b) => Message::Binary(b),
            Ping(b) => Message::Ping(b),
            Pong(b) => Message::Pong(b),
            Close(data) => Message::Close(data.map(|inner| CloseFrame {
                code: inner.0.into(),
                reason: inner.1.into(),
            })),
        }
    }
}

impl From<Message> for WebSocketMessage {
    fn from(value: Message) -> WebSocketMessage {
        match value {
            Message::Binary(d) => WebSocketMessage::Binary(d),
            Message::Text(t) => WebSocketMessage::Text(t),
            Message::Close(d) => WebSocketMessage::Close(
                d.map(|inner| (inner.code.into(), inner.reason.to_string())),
            ),
            Message::Ping(d) => WebSocketMessage::Ping(d),
            Message::Pong(d) => WebSocketMessage::Pong(d),
            Message::Frame(_) => unimplemented!(),
        }
    }
}

impl From<axum::extract::ws::Message> for WebSocketMessage {
    fn from(value: axum::extract::ws::Message) -> Self {
        use axum::extract::ws::Message::*;
        match value {
            Binary(d) => WebSocketMessage::Binary(d),
            Text(t) => WebSocketMessage::Text(t),
            Close(d) => WebSocketMessage::Close(
                d.map(|inner| (inner.code.into(), inner.reason.to_string())),
            ),
            Ping(d) => WebSocketMessage::Ping(d),
            Pong(d) => WebSocketMessage::Pong(d),
        }
    }
}

impl Into<axum::extract::ws::Message> for WebSocketMessage {
    fn into(self) -> axum::extract::ws::Message {
        use axum::extract::ws::Message::*;
        match self {
            WebSocketMessage::Binary(d) => Binary(d),
            WebSocketMessage::Text(t) => Text(t),
            WebSocketMessage::Close(d) => Close(d.map(|inner| axum::extract::ws::CloseFrame {
                code: inner.0.into(),
                reason: inner.1.into(),
            })),
            WebSocketMessage::Ping(d) => Ping(d),
            WebSocketMessage::Pong(d) => Pong(d),
        }
    }
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
