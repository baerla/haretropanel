use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use tracing::info;

use crate::infrastructure::web::AppState;

pub async fn ws_solar(
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    info!("WebSocket client connected to /ws/solar");

    let (mut ws_sender, mut ws_receiver) = socket.split();
    let mut broadcast_rx = state.dashboard_service.subscribe_to_ws();

    // Spawn task: broadcast channel → WebSocket sender
    let sender_task = tokio::spawn(async move {
        loop {
            match broadcast_rx.recv().await {
                Ok(payload) => {
                    if ws_sender
                        .send(Message::Text(payload.to_string().into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("WebSocket lagged behind, skipped {} messages", n);
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }
    });

    // Close when client disconnects
    while let Some(Ok(_msg)) = ws_receiver.next().await {
        // receive messages to keep connection alive
    }
    sender_task.abort();

    info!("WebSocket client disconnected from /ws/solar");
}
