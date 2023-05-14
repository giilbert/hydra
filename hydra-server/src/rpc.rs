use serde_json::Value;
use std::collections::HashMap;
use tokio::sync::oneshot;

use uuid::Uuid;

#[derive(Debug)]
pub struct RpcRecords {
    pub records: HashMap<Uuid, oneshot::Sender<Result<Value, String>>>,
}

impl RpcRecords {
    pub fn new() -> Self {
        Self {
            records: HashMap::new(),
        }
    }

    pub async fn await_response(
        &mut self,
        id: Uuid,
    ) -> anyhow::Result<oneshot::Receiver<Result<Value, String>>> {
        let (tx, rx) = oneshot::channel();
        self.records.insert(id, tx);

        Ok(rx)
    }

    pub fn handle_incoming(
        &mut self,
        id: Uuid,
        value: Result<Value, String>,
    ) -> anyhow::Result<()> {
        if let Some(tx) = self.records.remove(&id) {
            if let Err(v) = tx.send(value) {
                log::error!("Failed to send rpc value: {:?}", v);
            }
        }

        Ok(())
    }
}
