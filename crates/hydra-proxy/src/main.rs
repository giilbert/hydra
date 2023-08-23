mod discovery;
mod error_page;
mod handler;
mod websocket;

use crate::handler::{client_websocket_handler, container_proxy_handler};
use axum::{
    body::Body,
    extract::{DefaultBodyLimit, Host},
    http::Request,
    routing::{any, get},
    Extension, Router,
};
use redis::{aio::Connection, Client};
use shared::prelude::*;
use std::{net::SocketAddr, sync::Arc};
use tokio::sync::Mutex;
use tower::ServiceExt;

const MAX_BODY_SIZE: usize = 20 * 1024 * 1024; // 20 MB

pub struct AppState {
    pub redis: Mutex<Connection>,
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    pretty_env_logger::init();
    dotenv::dotenv().ok();

    // client websockets connect to this url
    let server_proxy_domain = std::env::var("SERVER_PROXY_DOMAIN")?;

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

    let server_router = Router::new().route("/execute", get(client_websocket_handler));
    let proxy_router = any(container_proxy_handler);

    let universal_handler = any(|Host(hostname), request: Request<Body>| async move {
        if hostname == server_proxy_domain {
            server_router.oneshot(request).await
        } else {
            proxy_router.oneshot(request).await
        }
    });

    let app = universal_handler
        .layer::<_, _, std::convert::Infallible>(Extension(Arc::new(state)))
        .layer(DefaultBodyLimit::max(MAX_BODY_SIZE));

    axum::Server::bind(&addr.into())
        .serve(app.into_make_service())
        .await?;

    Ok(())
}
