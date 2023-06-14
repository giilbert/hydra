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
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // console_subscriber::init();
    pretty_env_logger::init();

    let state = AppState::new(RwLock::new(AppStateInner {
        run_requests: Default::default(),
        container_pool: ContainerPool::new(5).await,
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

    // tokio::spawn(async move {
    //     loop {
    //         tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    //         log::info!("Run requests: {:?}", state.read().run_requests.keys());
    //     }
    // });
    log::info!("Server listening on 0.0.0.0:3001");
    axum::Server::bind(&"0.0.0.0:3001".parse()?)
        .serve(router.into_make_service())
        .await?;

    Ok(())
}
