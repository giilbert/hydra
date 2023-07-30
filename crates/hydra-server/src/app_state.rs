use crate::{
    config::Environment,
    pool::ContainerPool,
    proxy_websockets::{WebSocketConnection, WebSocketConnectionRequest},
    session::{ProxyPayload, Session},
};
use redis::Client;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    inner: Arc<AppStateInner>,
}

impl std::ops::Deref for AppState {
    type Target = Arc<AppStateInner>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

pub struct AppStateInner {
    pub sessions: RwLock<HashMap<Uuid, Arc<Session>>>,
    pub proxy_requests: RwLock<HashMap<Uuid, mpsc::Sender<ProxyPayload>>>,
    pub websocket_connection_requests:
        RwLock<HashMap<Uuid, mpsc::Sender<WebSocketConnectionRequest>>>,
    pub container_pool: ContainerPool,
    pub api_key: String,
    pub redis: RwLock<redis::aio::Connection>,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppStateInner")
            .field("sessions", &self.inner.sessions)
            .field("container_pool", &self.inner.container_pool)
            .field("api_key", &self.inner.api_key)
            .finish_non_exhaustive()
    }
}

impl AppState {
    pub async fn create_with_defaults(environment: Environment) -> Self {
        let redis_client = Client::open(
            std::env::var("REDIS_URL")
                .expect("REDIS_URL should be set in .env")
                .as_str(),
        )
        .expect("unable to create redis client");

        let redis = redis_client
            .get_tokio_connection()
            .await
            .expect("unable to create connection");

        AppState {
            inner: Arc::new(AppStateInner {
                sessions: Default::default(),
                proxy_requests: Default::default(),
                websocket_connection_requests: Default::default(),
                api_key: std::env::var("HYDRA_API_KEY").unwrap_or_else(|_| {
                    log::warn!("No API key set. Using `hydra`.");
                    "hydra".to_string()
                }),
                container_pool: ContainerPool::new(if environment == Environment::Development {
                    2
                } else {
                    // // TESTING VALUE
                    // 2
                    8
                })
                .await,
                redis: RwLock::new(redis),
            }),
        }
    }
}
