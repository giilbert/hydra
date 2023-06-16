use std::{collections::HashMap, sync::Arc};

use axum::{
    routing::{get, post},
    Router,
};
use container::Container;
use execute::execute;
use pool::ContainerPool;
use run_request::RunRequest;
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};
use uuid::Uuid;

use crate::execute::execute_websocket;

mod container;
mod execute;
mod pool;
mod rpc;
mod run_request;

type AppState = Arc<RwLock<AppStateInner>>;

#[derive(Debug)]
pub struct AppStateInner {
    pub run_requests: HashMap<Uuid, RunRequest>,
    pub container_pool: ContainerPool,
    pub api_key: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();
    pretty_env_logger::init();

    let state = AppState::new(RwLock::new(AppStateInner {
        run_requests: Default::default(),
        container_pool: ContainerPool::new(5).await,
        api_key: std::env::var("HYDRA_API_KEY").unwrap_or_else(|_| {
            log::warn!("No API key set. Using `hydra`.");
            "hydra".to_string()
        }),
    }));

    let router = Router::new()
        .route("/", get(|| async { "Hydra" }))
        .route("/execute", post(execute).get(execute_websocket))
        .with_state(state.clone())
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_headers(Any)
                .allow_methods(Any),
        );

    log::info!("Server listening on 0.0.0.0:3001");
    axum::Server::bind(&"0.0.0.0:3001".parse()?)
        .serve(router.into_make_service())
        .await?;

    Ok(())
}
