// GhostWire Server - Shuttle Entry Point
// This is the "dumb relay" server that knows nothing about message content

mod relay;
mod status_page;

use axum::{
    Router,
    extract::{State, ws::WebSocketUpgrade},
    http::HeaderMap,
    response::{Html, IntoResponse},
    routing::get,
};
use relay::RelayState;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};

/// Health check endpoint
async fn health_check() -> &'static str {
    "GhostWire Relay - Status: ONLINE"
}

/// Root endpoint with server info
async fn root(State(state): State<RelayState>, headers: HeaderMap) -> Html<String> {
    let client_count = state.client_count().await;

    Html(status_page::render(client_count, &headers, false))
}

/// WebSocket upgrade handler
async fn ws_handler(ws: WebSocketUpgrade, State(state): State<RelayState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| relay::handle_websocket(socket, state))
}

/// Redirect to the install script
async fn install_redirect() -> impl IntoResponse {
    axum::response::Redirect::temporary(
        "https://raw.githubusercontent.com/jcyrus/GhostWire/main/install.sh",
    )
}

/// Redirect to the PowerShell install script
async fn install_ps1_redirect() -> impl IntoResponse {
    axum::response::Redirect::temporary(
        "https://raw.githubusercontent.com/jcyrus/GhostWire/main/install.ps1",
    )
}

/// Main Shuttle entry point
#[shuttle_runtime::main]
async fn main() -> shuttle_axum::ShuttleAxum {
    // Shuttle handles tracing initialization, so we don't need to do it here

    // Create shared state
    let state = RelayState::new();

    // Build the router
    let router = Router::new()
        .route("/", get(root))
        .route("/health", get(health_check))
        .route("/ws", get(ws_handler))
        .route("/install", get(install_redirect))
        .route("/install.ps1", get(install_ps1_redirect))
        .with_state(state)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::default().include_headers(true)),
        );

    Ok(router.into())
}
