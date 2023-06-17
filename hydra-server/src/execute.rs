use axum::debug_handler;
use axum::extract::{Query, WebSocketUpgrade};
use axum::response::Response;
use axum::{extract::State, Json};
use protocol::ExecuteOptions;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::run_request::RunRequest;
use crate::AppState;

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
) -> Result<Json<ExecuteResponse>, &'static str> {
    if params.api_key != app_state.read().await.api_key {
        return Err("Invalid API key");
    }

    let run_request = RunRequest::new(options, app_state.clone())
        .await
        .map_err(|e| {
            log::error!("{e}");
            "Failed to create run request."
        })?;

    let ticket = run_request.ticket.clone();
    run_request.prime_self_destruct().await;

    app_state
        .write()
        .await
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
        .await
        .run_requests
        .remove(&request.ticket)
        .map_or(Err("No such ticket"), |run_request| {
            Ok(ws.on_upgrade(move |ws| async {
                if let Err(err) = run_request.handle_websocket_connection(ws).await {
                    log::error!("Error handling WebSocket connection: {:?}", err);
                }
            }))
        })
}
