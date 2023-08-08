use std::sync::Arc;

use crate::prelude::*;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type", content = "data")]
pub enum ContainerRpcRequest {
    SetupFromOptions {
        options: Arc<ExecuteOptions>,
    },
    PtyCreate {
        command: String,
        arguments: Vec<String>,
    },
    PtyInput {
        pty_id: u32,
        input: String,
    },
    Crash,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct File {
    pub path: String,
    pub content: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ExecuteOptions {
    #[serde(default)]
    /// Whether to keep the container alive after the
    /// program is ran, to be used for further runs
    pub persistent: bool,
    pub files: Vec<File>,
}
