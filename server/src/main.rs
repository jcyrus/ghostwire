// GhostWire Server - Fly.io Entry Point
// This is the "dumb relay" server that knows nothing about message content

mod relay;

use axum::{
    extract::{
        ws::WebSocketUpgrade,
        State,
    },
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use relay::RelayState;
use std::net::SocketAddr;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use tracing::info;
use tracing_subscriber::EnvFilter;

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
        <p>WebSocket Endpoint: <code>wss://ghostwire.fly.dev/ws</code></p>
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
async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<RelayState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| relay::handle_websocket(socket, state))
}

/// Redirect to the install script
async fn install_redirect() -> impl IntoResponse {
    axum::response::Redirect::temporary("https://raw.githubusercontent.com/jcyrus/GhostWire/main/install.sh")
}

/// Redirect to the PowerShell install script
async fn install_ps1_redirect() -> impl IntoResponse {
    axum::response::Redirect::temporary("https://raw.githubusercontent.com/jcyrus/GhostWire/main/install.ps1")
}

/// Main entry point for Fly.io deployment
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

    info!("🚀 Starting GhostWire Relay Server");
    
    // Create shared state
    let state = RelayState::new();

    // Build the router
    let app = Router::new()
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

    // Get port from environment variable (Fly.io injects PORT)
    let port = std::env::var("PORT")
        .unwrap_or_else(|_| "8080".to_string())
        .parse::<u16>()
        .unwrap_or_else(|e| {
            tracing::error!("Invalid PORT value: {}. Using default 8080", e);
            8080
        });
    
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("👻 GhostWire Relay listening on http://{}", addr);
    info!("📡 WebSocket endpoint: ws://{}/ws", addr);
    info!("🌐 Status page: http://{}", addr);

    // Start server
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!("Failed to bind to {}: {}", addr, e);
            std::process::exit(1);
        }
    };
    
    if let Err(e) = axum::serve(listener, app).await {
        tracing::error!("Server error: {}", e);
        std::process::exit(1);
    }
}
