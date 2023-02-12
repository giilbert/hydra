use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug)]
#[serde(tag = "type", content = "data")]
pub enum ContainerSent {
    Ping { timestamp: u64 },
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(tag = "type", content = "data")]
pub enum HostSent {}
