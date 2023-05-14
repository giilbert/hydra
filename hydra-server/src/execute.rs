use axum::debug_handler;
use axum::extract::{Query, WebSocketUpgrade};
use axum::response::Response;
use axum::{extract::State, Json};
use protocol::{ContainerRpcProcedure, ContainerRpcRequest, ExecuteOptions};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::run_request::RunRequest;
use crate::{container::Container, AppState};

#[derive(Serialize)]
pub struct ExecuteResponse {
    pub ticket: String,
}

#[debug_handler]
pub async fn execute(
    State(app_state): State<AppState>,
    Json(options): Json<ExecuteOptions>,
) -> Result<Json<ExecuteResponse>, &'static str> {
    let mut run_request = RunRequest::new(options, app_state.clone())
        .await
        .map_err(|_| "Failed to create run request.")?;
    let ticket = run_request.ticket.clone();
    run_request.prime_self_destruct();

    app_state
        .write()
        .run_requests
        .insert(run_request.ticket, run_request);

    Ok(Json(ExecuteResponse {
        ticket: ticket.to_string(),
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
) -> Result<Response, &'static str> {
    app_state
        .write()
        .run_requests
        .remove(&request.ticket)
        .map_or(Err("No such ticket"), |run_request| {
            Ok(ws.on_upgrade(move |ws| run_request.handle_websocket_connection(ws)))
        })
}
