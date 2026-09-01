use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// HTTP/1.1 response head with `Content-Length` and `Connection: close`.
pub fn response_head(extra_headers: &str, body_len: usize) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {body_len}\r\n{extra_headers}Connection: close\r\n\r\n"
    )
}

/// Serve exactly one HTTP/1.1 response on an ephemeral local port.
pub async fn serve_once(head: String, body: Vec<u8>) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; 4096];
        let mut seen = Vec::new();
        loop {
            let n = socket.read(&mut buf).await.unwrap();
            if n == 0 {
                break;
            }
            seen.extend_from_slice(&buf[..n]);
            if seen.windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
        }
        socket.write_all(head.as_bytes()).await.unwrap();
        socket.write_all(&body).await.unwrap();
    });
    format!("http://127.0.0.1:{port}/")
}

/// Read a full HTTP/1.1 request (headers + `Content-Length` body) from `socket`.
async fn read_request(socket: &mut tokio::net::TcpStream) -> Vec<u8> {
    let mut buf = [0u8; 4096];
    let mut seen = Vec::new();
    let mut content_len = 0usize;
    loop {
        let n = socket.read(&mut buf).await.unwrap();
        if n == 0 {
            break;
        }
        seen.extend_from_slice(&buf[..n]);
        if let Some(pos) = seen.windows(4).position(|w| w == b"\r\n\r\n") {
            if content_len == 0 {
                let headers = String::from_utf8_lossy(&seen[..pos]);
                content_len = headers
                    .lines()
                    .find_map(|l| {
                        l.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|v| v.trim().parse().ok())
                    })
                    .unwrap_or(0);
            }
            if seen.len() >= pos + 4 + content_len {
                break;
            }
        }
    }
    seen
}

/// Serve exactly one HTTP/1.1 exchange on an ephemeral port: read the full
/// request (headers + `Content-Length` body), capture it, respond, close.
pub async fn serve_capture(head: String, body: Vec<u8>) -> (String, Arc<Mutex<Vec<u8>>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let captured: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let cap = captured.clone();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let seen = read_request(&mut socket).await;
        *cap.lock().unwrap() = seen;
        socket.write_all(head.as_bytes()).await.unwrap();
        socket.write_all(&body).await.unwrap();
    });
    (format!("http://127.0.0.1:{port}/"), captured)
}

/// Serve `responses` sequentially on one ephemeral port: connection *n*
/// receives response *n* (head + body). Every request (headers +
/// `Content-Length` body) is captured in order. Used for redirect tests that
/// need multiple responses from the same origin.
pub async fn serve_sequence(
    responses: Vec<(String, Vec<u8>)>,
) -> (String, Arc<Mutex<Vec<Vec<u8>>>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let captured: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(Vec::new()));
    let cap = captured.clone();
    tokio::spawn(async move {
        for (head, body) in responses {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let seen = read_request(&mut socket).await;
            cap.lock().unwrap().push(seen);
            let _ = socket.write_all(head.as_bytes()).await;
            let _ = socket.write_all(&body).await;
        }
    });
    (format!("http://127.0.0.1:{port}/"), captured)
}

/// Serve the same HTTP/1.1 response to every connection until the test ends.
/// Unlike `serve_once`, supports requests that trigger additional probes
/// (e.g. the request-compression probe) or sequential requests.
pub async fn serve_always(head: String, body: Vec<u8>) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let head = head.clone();
            let body = body.clone();
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                let mut seen = Vec::new();
                loop {
                    match socket.read(&mut buf).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            seen.extend_from_slice(&buf[..n]);
                            if seen.windows(4).any(|w| w == b"\r\n\r\n") {
                                break;
                            }
                        }
                    }
                }
                let _ = socket.write_all(head.as_bytes()).await;
                let _ = socket.write_all(&body).await;
            });
        }
    });
    format!("http://127.0.0.1:{port}/")
}

/// Bind an ephemeral port, immediately drop the listener, and return its URL:
/// connections are refused (transport error, not a timeout).
pub async fn unreachable_base() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    format!("http://127.0.0.1:{port}/")
}

/// Serve response head plus a partial body, then hold the connection open
/// (the response never completes). Used to exercise read timeouts.
pub async fn serve_stalled(head: String, partial_body: &[u8]) -> String {
    let partial_body = partial_body.to_vec();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; 4096];
        loop {
            let n = socket.read(&mut buf).await.unwrap();
            if n == 0 || buf[..n].windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
        }
        socket.write_all(head.as_bytes()).await.unwrap();
        socket.write_all(&partial_body).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
    });
    format!("http://127.0.0.1:{port}/")
}
