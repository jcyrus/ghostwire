// GhostWire Server - Shuttle Entry Point
// This is the "dumb relay" server that knows nothing about message content

mod relay;

use axum::{
    extract::{ws::WebSocketUpgrade, State},
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use relay::RelayState;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};

/// Health check endpoint
async fn health_check() -> &'static str {
    "GhostWire Relay - Status: ONLINE"
}

/// Root endpoint with server info
async fn root(State(state): State<RelayState>) -> Html<String> {
    let client_count = state.client_count().await;

    Html(format!(
        r#"
<!DOCTYPE html>
<html>
<head>
    <title>GhostWire Relay</title>
    <style>
        body {{
            background: #000;
            color: #0f0;
            font-family: 'Courier New', monospace;
            padding: 2rem;
            max-width: 800px;
            margin: 0 auto;
        }}
        h1 {{ color: #0f0; text-shadow: 0 0 10px #0f0; }}
        .status {{ color: #0f0; }}
        .info {{ color: #0a0; margin: 1rem 0; }}
        pre {{ background: #111; padding: 1rem; border: 1px solid #0f0; }}
        a {{ color: #0ff; }}
    </style>
</head>
<body>
    <h1>👻 GhostWire Relay</h1>
    <div class="status">STATUS: ONLINE</div>
    <div class="info">
        <p>Connected Clients: {}</p>
        <p>WebSocket Endpoint: <code>ws://ghost.jcyrus.com/ws</code></p>
    </div>
    <h2>Protocol</h2>
    <pre>{{
  "type": "MSG" | "AUTH" | "SYS",
  "payload": "...",
  "meta": {{
    "sender": "...",
    "timestamp": 1234567890
  }}
}}</pre>
    <h2>Philosophy</h2>
    <p>This server is intentionally "dumb" - it relays messages without reading them.</p>
    <p>All security is client-side. The server knows nothing.</p>
    <hr>
    <p><a href="https://github.com/jcyrus/GhostWire">GitHub</a> | <a href="/health">Health Check</a></p>
</body>
</html>
        "#,
        client_count
    ))
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
