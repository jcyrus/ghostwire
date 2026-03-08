use axum::http::HeaderMap;

pub fn render(client_count: usize, headers: &HeaderMap, local_mode: bool) -> String {
    let websocket_endpoint = websocket_endpoint(headers);
    let title_suffix = if local_mode && websocket_endpoint.starts_with("ws://localhost") {
        " (Local)"
    } else {
        ""
    };

    format!(
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
    <h1>👻 GhostWire Relay{}</h1>
    <div class="status">STATUS: ONLINE</div>
    <div class="info">
        <p>Connected Clients: {}</p>
        <p>WebSocket Endpoint: <code>{}</code></p>
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
        title_suffix, client_count, websocket_endpoint
    )
}

fn websocket_endpoint(headers: &HeaderMap) -> String {
    let host = headers
        .get("x-forwarded-host")
        .or_else(|| headers.get("host"))
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty());

    match host {
        Some(host) => {
            let forwarded_proto = headers
                .get("x-forwarded-proto")
                .and_then(|value| value.to_str().ok())
                .map(str::trim);
            let scheme = if forwarded_proto == Some("http") || is_local_host(host) {
                "ws"
            } else {
                "wss"
            };

            format!("{}://{}/ws", scheme, host)
        }
        None => "ws://localhost:8080/ws".to_string(),
    }
}

fn is_local_host(host: &str) -> bool {
    let host = host_without_port(host);
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

fn host_without_port(host: &str) -> &str {
    if let Some(end) = host
        .strip_prefix('[')
        .and_then(|stripped| stripped.find(']'))
    {
        return &host[1..1 + end];
    }

    match host.rsplit_once(':') {
        Some((name, port)) if !name.contains(':') && port.chars().all(|c| c.is_ascii_digit()) => {
            name
        }
        _ => host,
    }
}

#[cfg(test)]
mod tests {
    use super::render;
    use axum::http::{HeaderMap, HeaderValue};

    #[test]
    fn render_uses_tls_for_remote_hosts() {
        let mut headers = HeaderMap::new();
        headers.insert("host", HeaderValue::from_static("ghost.jcyrus.com"));

        let html = render(2, &headers, true);
        assert!(html.contains("wss://ghost.jcyrus.com/ws"));
    }

    #[test]
    fn render_uses_ws_for_local_hosts() {
        let mut headers = HeaderMap::new();
        headers.insert("host", HeaderValue::from_static("localhost:8080"));

        let html = render(0, &headers, true);
        assert!(html.contains("ws://localhost:8080/ws"));
    }

    #[test]
    fn render_prefers_forwarded_host_and_proto() {
        let mut headers = HeaderMap::new();
        headers.insert("host", HeaderValue::from_static("localhost:8080"));
        headers.insert(
            "x-forwarded-host",
            HeaderValue::from_static("ghostwire.fly.dev"),
        );
        headers.insert("x-forwarded-proto", HeaderValue::from_static("https"));

        let html = render(1, &headers, false);
        assert!(html.contains("wss://ghostwire.fly.dev/ws"));
    }
}
