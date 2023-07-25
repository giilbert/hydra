mod error_page;
mod handler;
mod websocket;

use crate::handler::handler;
use axum::handler::HandlerWithoutStateExt;
use shared::prelude::*;
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    pretty_env_logger::init();

    let addr = SocketAddr::from(([0, 0, 0, 0], 3101));
    log::info!("Listening on {}", addr);

    axum::Server::bind(&addr.into())
        .serve(tower::make::Shared::new(handler.into_service()))
        .await?;

    Ok(())
}
