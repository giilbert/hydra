use axum::handler::HandlerWithoutStateExt;
use std::net::SocketAddr;

mod handler;
mod websocket;

use crate::handler::handler;

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    let addr = SocketAddr::from(([0, 0, 0, 0], 3101));
    log::info!("Listening on {}", addr);

    axum::Server::bind(&addr.into())
        .serve(tower::make::Shared::new(handler.into_service()))
        .await
        .unwrap();
}
