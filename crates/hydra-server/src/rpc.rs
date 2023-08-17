use crate::container::StopRx;
use dashmap::DashMap;
use shared::prelude::*;
use thiserror::Error;
use tokio::sync::oneshot;
use uuid::Uuid;

// TODO: add a max size and a timeout
#[derive(Debug)]
pub struct RpcRecords<T: std::fmt::Debug> {
    stop_rx: StopRx,
    pub records: DashMap<Uuid, oneshot::Sender<T>>,
}

#[derive(Debug, Error)]
pub enum RpcError {
    #[error("rpc timed out")]
    Timeout,
    #[error("container stopped during rpc")]
    Stopped,
    #[error("failed to receive response")]
    RecvErr,
}

impl<T> RpcRecords<T>
where
    T: std::fmt::Debug,
{
    pub fn new(stop_rx: StopRx) -> Self {
        Self {
            stop_rx,
            records: DashMap::new(),
        }
    }

    pub async fn get_response(&self, id: Uuid) -> Result<T, RpcError> {
        use tokio::time::{self, Duration};
        let (tx, rx) = oneshot::channel();
        self.records.insert(id, tx);

        let mut stop_rx = self.stop_rx.clone();
        let response = tokio::select! {
            res = rx => res.map_err(|_| RpcError::RecvErr),
            _ = time::sleep(Duration::from_secs(10)) => Err(RpcError::Timeout),
            _ = stop_rx.changed() => Err(RpcError::Stopped),
        };

        response
    }

    pub fn handle_incoming(&self, id: Uuid, value: T) -> Result<()> {
        log::debug!("Got RPC response (unsorted): {:?}", value);

        if let Some((.., tx)) = self.records.remove(&id) {
            if let Err(v) = tx.send(value) {
                log::error!("Failed to send rpc value: {:?}", v);
            }
        }

        Ok(())
    }
}
