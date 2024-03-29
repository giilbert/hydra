use std::sync::Arc;

use crate::session::Session;
use crate::AppState;
use axum::debug_handler;
use axum::extract::{Query, WebSocketUpgrade};
use axum::response::Response;
use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use shared::{
    protocol::{ContainerSent, ExecuteOptions},
    ErrorResponse,
};
use uuid::Uuid;

#[derive(Serialize)]
pub struct ExecuteResponse {
    pub ticket: String,
}

#[derive(Deserialize)]
pub struct ExecuteQueryParams {
    api_key: String,
}

/// Creates a run request
#[debug_handler]
pub async fn execute(
    State(app_state): State<AppState>,
    Query(params): Query<ExecuteQueryParams>,
    Json(options): Json<ExecuteOptions>,
) -> Result<Json<ExecuteResponse>, ErrorResponse> {
    if params.api_key != app_state.api_key {
        return ErrorResponse::unauthorized("Invalid API key").into();
    }

    let session = Session::new(options, app_state.clone())
        .await
        .map_err(|e| {
            log::error!("{e}");
            ErrorResponse::error("Failed to create run request.").into()
        })?;

    let ticket = session.ticket.clone();
    session.prime_self_destruct().await;

    app_state
        .sessions
        .write()
        .await
        .insert(session.ticket, Arc::new(session));

    Ok(Json(ExecuteResponse {
        ticket: ticket.to_string(),
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteHeadlessRequest {
    pub execute_options: ExecuteOptions,
    pub input: String,
}
#[derive(Serialize)]
pub struct ExecuteHeadlessReponse {
    pub output: String,
}

#[debug_handler]
pub async fn execute_headless(
    State(app_state): State<AppState>,
    Query(params): Query<ExecuteQueryParams>,
    Json(options): Json<ExecuteHeadlessRequest>,
) -> Result<Json<ExecuteHeadlessReponse>, ErrorResponse> {
    if params.api_key != app_state.api_key {
        return ErrorResponse::unauthorized("Invalid API key").into();
    }

    let mut container_req = app_state.container_pool.take_one().await;
    let container = container_req
        .recv()
        .await
        .ok_or_else(|| ErrorResponse::error("Error receiving container"))?;

    log::info!("execute_headless");
    container
        .rpc_setup_from_options(options.execute_options)
        .await
        .map_err(|e| {
            log::error!("error setting up: {e}");
            ErrorResponse::error("Failed to set up container")
        })?;

    // TODO: let the user specify the command
    let pty_id = container
        .rpc_pty_create("python3".into(), vec!["main.py".into()])
        .await
        .map_err(|e| {
            log::error!("error creating pty: {e}");
            ErrorResponse::error("Failed to create pty.")
        })? as u32;

    // FIXME:
    // this is a hack to make sure the container is ready to receive input
    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

    container
        .rpc_pty_input(pty_id, options.input)
        .await
        .map_err(|e| {
            log::error!("{e}");
            ErrorResponse::error("Failed to input to pty.")
        })?;

    let mut full_output = String::new();

    let mut container_rx = container.listen().expect("error listening to container");

    loop {
        tokio::select! {
            Some(output) = container_rx.recv() => {
                match output {
                    ContainerSent::PtyOutput { pty_id: id, output } => {
                        if id == pty_id {
                            full_output += output.as_str();
                        }
                    }
                    ContainerSent::PtyExit { pty_id: id } => {
                        if id == pty_id {
                            break;
                        }
                    }
                    _ => ()
                }
            }
        }
    }

    tokio::spawn(async move {
        if let Err(e) = container.stop().await {
            log::error!("error stopping container: {e}");
        }
    });

    Ok(Json(ExecuteHeadlessReponse {
        output: full_output,
    }))
}

#[derive(Deserialize)]
pub struct ExecuteWebsocketRequest {
    ticket: Uuid,
}

#[debug_handler]
pub async fn execute_websocket(
    State(app_state): State<AppState>,
    Query(request): Query<ExecuteWebsocketRequest>,
    ws: WebSocketUpgrade,
) -> Result<Response, ErrorResponse> {
    let session = app_state
        .sessions
        .write()
        .await
        .get(&request.ticket)
        .cloned()
        .ok_or_else(|| ErrorResponse::not_found("Session not found"))?;

    Ok(ws.on_upgrade(move |ws| async move {
        let ticket = session.ticket.clone();

        app_state
            .proxy_requests
            .write()
            .await
            .insert(session.ticket.clone(), session.proxy_requests.clone());
        app_state
            .websocket_connection_requests
            .write()
            .await
            .insert(
                session.ticket.clone(),
                session.websocket_connections_requests_tx.clone(),
            );

        if let Err(err) = session.handle_websocket_connection(ws).await {
            log::error!("Error handling WebSocket connection: {:?}", err);
        }

        app_state.proxy_requests.write().await.remove(&ticket);
        app_state
            .websocket_connection_requests
            .write()
            .await
            .remove(&ticket);
    }))
}
