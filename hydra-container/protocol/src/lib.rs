use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug)]
#[serde(tag = "type", content = "data")]
pub enum ContainerSent {}

#[derive(Deserialize, Serialize, Debug)]
#[serde(tag = "type", content = "data")]
pub enum HostSent {}
