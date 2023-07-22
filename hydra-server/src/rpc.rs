use shared::prelude::*;
use std::collections::HashMap;
use tokio::sync::oneshot;
use uuid::Uuid;

#[derive(Debug)]
pub struct RpcRecords<T: std::fmt::Debug> {
    pub records: HashMap<Uuid, oneshot::Sender<Result<T, String>>>,
}

impl<T> RpcRecords<T>
where
    T: std::fmt::Debug,
{
    pub fn new() -> Self {
        Self {
            records: HashMap::new(),
        }
    }

    pub fn await_response(&mut self, id: Uuid) -> Result<oneshot::Receiver<Result<T, String>>> {
        let (tx, rx) = oneshot::channel();
        self.records.insert(id, tx);

        Ok(rx)
    }

    pub fn handle_incoming(&mut self, id: Uuid, value: Result<T, String>) -> Result<()> {
        log::debug!("Got RPC response (unsorted): {:?}", value);

        if let Some(tx) = self.records.remove(&id) {
            if let Err(v) = tx.send(value) {
                log::error!("Failed to send rpc value: {:?}", v);
            }
        }

        Ok(())
    }
}
