// GhostWire Server - Local Development Entry Point
// This binary is used for local development without Shuttle runtime

mod relay;
mod status_page;
mod util;

use axum::{
    extract::{ws::WebSocketUpgrade, ConnectInfo, State},
    http::HeaderMap,
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use relay::RelayState;
use std::net::SocketAddr;
use std::sync::Arc;
use tower_governor::{GovernorLayer, governor::GovernorConfigBuilder};
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use tracing::info;
use tracing_subscriber::EnvFilter;

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
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> impl IntoResponse {
    let from_ip = util::real_ip(&headers, Some(addr));
    // Enforce the frame-size cap at the protocol layer; handle_websocket also
    // guards relay::MAX_MESSAGE_BYTES as defense in depth.
    ws.max_message_size(relay::MAX_MESSAGE_BYTES)
        .max_frame_size(relay::MAX_MESSAGE_BYTES)
        .on_upgrade(move |socket| relay::handle_websocket(socket, state, from_ip))
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

    // Connection-level rate limit: 1 new WS connection per 6 s ≈ 10/min, burst 3.
    let mut governor_builder = GovernorConfigBuilder::default();
    governor_builder.per_second(6).burst_size(3);
    let governor_conf = Arc::new(
        governor_builder
            .key_extractor(util::RealIpExtractor)
            .finish()
            .expect("governor config"),
    );

    // Create shared state
    let state = RelayState::new();

    // Build the router
    let app = Router::new()
        .route("/", get(root))
        .route("/health", get(health_check))
        .route(
            "/ws",
            get(ws_handler).layer(GovernorLayer::new(governor_conf)),
        )
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

    // Start server — into_make_service_with_connect_info exposes ConnectInfo<SocketAddr>
    // so ws_handler can extract the real peer address for rate-limit keying.
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}
