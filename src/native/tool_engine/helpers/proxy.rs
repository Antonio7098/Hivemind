use super::*;

pub(crate) fn spawn_proxy_listener(
    stop: Arc<AtomicBool>,
    listener: TcpListener,
    is_socks5: bool,
    endpoint: String,
) -> JoinHandle<()> {
    thread::spawn(move || {
        while !stop.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    if is_socks5 {
                        let _ = stream.write_all(&[0x05, 0xFF]);
                    } else {
                        let _ = respond_proxy_denied(&mut stream, &endpoint);
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(25));
                }
                Err(_) => break,
            }
        }
    })
}

pub(crate) fn spawn_admin_listener(
    stop: Arc<AtomicBool>,
    listener: TcpListener,
    endpoint: String,
    http_proxy: String,
    socks_proxy: Option<String>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        while !stop.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let body = serde_json::to_string(&json!({
                        "ok": true,
                        "admin_endpoint": endpoint,
                        "http_proxy": http_proxy,
                        "socks5_proxy": socks_proxy,
                    }))
                    .unwrap_or_else(|_| "{\"ok\":false}".to_string());
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = stream.write_all(response.as_bytes());
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(25));
                }
                Err(_) => break,
            }
        }
    })
}

pub(crate) fn respond_proxy_denied(stream: &mut TcpStream, endpoint: &str) -> std::io::Result<()> {
    let body = format!(
        "{{\"ok\":false,\"code\":\"network_proxy_denied\",\"message\":\"managed network proxy denied request\",\"admin\":\"{endpoint}\"}}"
    );
    let response = format!(
        "HTTP/1.1 403 Forbidden\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(response.as_bytes())
}

pub(crate) fn clamp_bind_address(
    raw: &str,
    allow_non_loopback: bool,
    fallback: &str,
) -> (String, bool) {
    let candidate = if raw.trim().is_empty() {
        fallback.to_string()
    } else {
        raw.trim().to_string()
    };
    let mut parts = candidate.rsplitn(2, ':');
    let port_raw = parts.next().unwrap_or("0");
    let host_raw = parts.next().unwrap_or("127.0.0.1");
    let port = port_raw.parse::<u16>().unwrap_or(0);
    let mut host = host_raw.to_string();
    let clamped = if !allow_non_loopback && !is_loopback_host(&host) {
        host = "127.0.0.1".to_string();
        true
    } else {
        false
    };
    (format!("{host}:{port}"), clamped)
}

pub(crate) fn is_loopback_host(host: &str) -> bool {
    let normalized = host.trim().trim_matches('[').trim_matches(']');
    if normalized.eq_ignore_ascii_case("localhost") {
        return true;
    }
    normalized
        .parse::<IpAddr>()
        .is_ok_and(|ip| ip.is_loopback())
}
