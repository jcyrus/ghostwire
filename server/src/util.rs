use axum::extract::ConnectInfo;
use axum::http::HeaderMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tower_governor::errors::GovernorError;
use tower_governor::key_extractor::KeyExtractor;

/// Extract the real client IP from request headers and the TCP peer address.
///
/// Priority: `Fly-Client-IP` (Fly.io edge) → `X-Forwarded-For` first hop →
/// `peer_addr` → `127.0.0.1` fallback. The Fly header wins over XFF to prevent
/// spoofing on the public internet path; on Fly, only the Fly edge can set it.
pub fn real_ip(headers: &HeaderMap, peer_addr: Option<SocketAddr>) -> IpAddr {
    if let Some(ip) = headers
        .get("Fly-Client-IP")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<IpAddr>().ok())
    {
        return ip;
    }
    if let Some(ip) = headers
        .get("X-Forwarded-For")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .and_then(|s| s.trim().parse::<IpAddr>().ok())
    {
        return ip;
    }
    if let Some(addr) = peer_addr {
        return addr.ip();
    }
    tracing::warn!(
        "real_ip: no Fly-Client-IP, X-Forwarded-For, or peer address; \
         falling back to 127.0.0.1 — all rate-limit buckets will share one slot"
    );
    IpAddr::V4(Ipv4Addr::LOCALHOST)
}

/// Tower-governor `KeyExtractor` that calls [`real_ip`] to obtain the
/// rate-limit key.  Works in both the Shuttle deployment (no `ConnectInfo`;
/// falls back to headers) and local dev (has `ConnectInfo<SocketAddr>`).
#[derive(Clone)]
pub struct RealIpExtractor;

impl KeyExtractor for RealIpExtractor {
    type Key = IpAddr;

    fn extract<T>(&self, request: &axum::http::Request<T>) -> Result<Self::Key, GovernorError> {
        let peer = request
            .extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ci| ci.0);
        Ok(real_ip(request.headers(), peer))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers_with(name: &str, value: &str) -> HeaderMap {
        let mut m = HeaderMap::new();
        m.insert(
            axum::http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
            HeaderValue::from_str(value).unwrap(),
        );
        m
    }

    #[test]
    fn test_real_ip_fly_header() {
        let h = headers_with("fly-client-ip", "1.2.3.4");
        assert_eq!(real_ip(&h, None), "1.2.3.4".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn test_real_ip_xforwardedfor_first_hop() {
        let h = headers_with("x-forwarded-for", "10.0.0.1, 10.0.0.2");
        assert_eq!(real_ip(&h, None), "10.0.0.1".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn test_real_ip_peer_addr_fallback() {
        let addr: SocketAddr = "127.0.0.1:9999".parse().unwrap();
        assert_eq!(real_ip(&HeaderMap::new(), Some(addr)), addr.ip());
    }

    #[test]
    fn test_real_ip_fly_takes_precedence_over_xff() {
        let mut h = headers_with("fly-client-ip", "1.2.3.4");
        h.insert(
            axum::http::HeaderName::from_bytes(b"x-forwarded-for").unwrap(),
            HeaderValue::from_static("5.6.7.8"),
        );
        assert_eq!(real_ip(&h, None), "1.2.3.4".parse::<IpAddr>().unwrap());
    }
}
