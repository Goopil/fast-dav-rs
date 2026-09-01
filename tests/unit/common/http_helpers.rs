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

/// Serve exactly one HTTP/1.1 exchange on an ephemeral port: read the full
/// request (headers + `Content-Length` body), capture it, respond, close.
pub async fn serve_capture(head: String, body: Vec<u8>) -> (String, Arc<Mutex<Vec<u8>>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let captured: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let cap = captured.clone();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
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
        *cap.lock().unwrap() = seen;
        socket.write_all(head.as_bytes()).await.unwrap();
        socket.write_all(&body).await.unwrap();
    });
    (format!("http://127.0.0.1:{port}/"), captured)
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
