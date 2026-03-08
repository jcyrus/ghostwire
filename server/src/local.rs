// GhostWire Server - Local Development Entry Point
// This binary is used for local development without Shuttle runtime

mod relay;
mod status_page;

use axum::{
    extract::{ws::WebSocketUpgrade, State},
    http::HeaderMap,
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use relay::RelayState;
use std::net::SocketAddr;
use tracing_subscriber::EnvFilter;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use tracing::info;

/// Health check endpoint
async fn health_check() -> &'static str {
    "GhostWire Relay - Status: ONLINE"
}

/// Root endpoint with server info
async fn root(State(state): State<RelayState>, headers: HeaderMap) -> Html<String> {
    let client_count = state.client_count().await;

    Html(status_page::render(client_count, &headers, true))
}

/// WebSocket upgrade handler
async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<RelayState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| relay::handle_websocket(socket, state))
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                EnvFilter::new("info")
                    .add_directive("ghostwire_server=debug".parse().expect("Invalid tracing directive"))
                    .add_directive("tower_http=debug".parse().expect("Invalid tracing directive"))
            }),
        )
        .init();

    info!("🚀 Starting GhostWire Relay Server (Local Mode)");

    // Create shared state
    let state = RelayState::new();

    // Build the router
    let app = Router::new()
        .route("/", get(root))
        .route("/health", get(health_check))
        .route("/ws", get(ws_handler))
        .with_state(state)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::default().include_headers(true)),
        );

    // Bind to address
    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    info!("👻 GhostWire Relay listening on http://{}", addr);
    info!("📡 WebSocket endpoint: ws://{}/ws", addr);
    info!("🌐 Status page: http://{}", addr);

    // Start server
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
