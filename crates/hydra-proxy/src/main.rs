mod discovery;
mod error_page;
mod handler;
mod websocket;

use crate::handler::handler;
use axum::{extract::DefaultBodyLimit, handler::Handler};
use redis::{aio::Connection, Client};
use shared::prelude::*;
use std::{net::SocketAddr, sync::Arc};
use tokio::sync::Mutex;

const MAX_BODY_SIZE: usize = 20 * 1024 * 1024; // 20 MB

pub struct AppState {
    pub redis: Mutex<Connection>,
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    pretty_env_logger::init();
    dotenv::dotenv().ok();

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

    let state = AppState {
        redis: Mutex::new(redis),
    };

    let addr = SocketAddr::from(([0, 0, 0, 0], 3101));
    log::info!("Listening on {}", addr);

    axum::Server::bind(&addr.into())
        .serve(
            handler
                .layer(DefaultBodyLimit::max(MAX_BODY_SIZE))
                .with_state(Arc::new(state))
                .into_make_service(),
        )
        .await?;

    Ok(())
}
