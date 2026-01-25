use anyhow::{Result, anyhow};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use bytes::Bytes;
use futures::{StreamExt, stream::FuturesOrdered};
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::{HeaderMap, Method, Request, Response, StatusCode, Uri, header};
use std::sync::{Arc, RwLock};
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};
use tokio::time::{Duration, timeout};

use crate::common::compression::{
    ContentEncoding, add_accept_encoding, add_content_encoding, compress_payload, decompress_body,
    detect_encodings,
};
use crate::common::http::{HyperClient, build_hyper_client};
use crate::webdav::types::{BatchItem, Depth};

/// Strategy for compressing outgoing request bodies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestCompressionMode {
    /// Negotiate automatically: attempt gzip on first use and cache the result; fall back to identity on 415/501.
    Auto,
    Disabled,
    Force(ContentEncoding),
}

impl RequestCompressionMode {
    fn is_auto(&self) -> bool {
        matches!(self, Self::Auto)
    }
}

const AUTO_DEFAULT_ENCODING: ContentEncoding = ContentEncoding::Gzip;
const PROBE_BODY: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:current-user-principal/>
  </D:prop>
</D:propfind>"#;

#[derive(Clone)]
pub struct WebDavClient {
    base: Uri,
    client: HyperClient,
    auth_header: Option<header::HeaderValue>,
    default_timeout: Duration,
    request_compression_mode: RequestCompressionMode,
    negotiated_request_compression: Arc<RwLock<Option<ContentEncoding>>>,
    request_compression_probe: Arc<Mutex<()>>,
}

impl WebDavClient {
    /// Create a new client from a **base URL** (collection/home-set) and optional **Basic** credentials.
    ///
    /// The base may be `https://` **or** `http://` (both are supported by the connector).
    pub fn new(base_url: &str, basic_user: Option<&str>, basic_pass: Option<&str>) -> Result<Self> {
        let client = build_hyper_client()?;

        let base: Uri = base_url.parse()?;
        let auth_header = if let (Some(u), Some(p)) = (basic_user, basic_pass) {
            let token = format!("{}:{}", u, p);
            let val = format!("Basic {}", B64.encode(token));
            Some(header::HeaderValue::from_str(&val)?)
        } else {
            None
        };

        Ok(Self {
            base,
            client,
            auth_header,
            default_timeout: Duration::from_secs(20),
            request_compression_mode: RequestCompressionMode::Auto,
            negotiated_request_compression: Arc::new(RwLock::new(None)),
            request_compression_probe: Arc::new(Mutex::new(())),
        })
    }

    /// Configure request compression for this client.
    pub fn set_request_compression(&mut self, encoding: ContentEncoding) {
        self.set_request_compression_mode(RequestCompressionMode::Force(encoding));
    }

    /// Configure the request compression strategy.
    pub fn set_request_compression_mode(&mut self, mode: RequestCompressionMode) {
        self.request_compression_mode = mode;
        match mode {
            RequestCompressionMode::Auto => self.set_negotiated_encoding(None),
            RequestCompressionMode::Disabled => {
                self.set_negotiated_encoding(Some(ContentEncoding::Identity))
            }
            RequestCompressionMode::Force(enc) => self.set_negotiated_encoding(Some(enc)),
        }
    }

    /// Enable adaptive request compression (default behaviour).
    pub fn set_request_compression_auto(&mut self) {
        self.set_request_compression_mode(RequestCompressionMode::Auto);
    }

    /// Disable request compression entirely.
    pub fn disable_request_compression(&mut self) {
        self.set_request_compression_mode(RequestCompressionMode::Disabled);
    }

    /// Get the current request compression strategy.
    pub fn request_compression_mode(&self) -> RequestCompressionMode {
        self.request_compression_mode
    }

    /// Get the currently resolved request compression encoding.
    pub fn request_compression(&self) -> ContentEncoding {
        self.resolve_request_encoding()
    }

    pub fn build_uri(&self, path: &str) -> Result<Uri> {
        if path.starts_with("http://") || path.starts_with("https://") {
            return Ok(path.parse()?);
        }

        let mut parts = self.base.clone().into_parts();
        let existing_path = parts
            .path_and_query
            .as_ref()
            .map(|pq| pq.path())
            .unwrap_or("/");

        let (path_only, query) = if let Some((p, q)) = path.split_once('?') {
            (p, Some(q))
        } else {
            (path, None)
        };

        let mut combined = if path_only.is_empty() {
            existing_path.to_string()
        } else if path_only.starts_with('/') {
            path_only.to_string()
        } else {
            let mut base = existing_path.trim_end_matches('/').to_string();
            if base.is_empty() {
                base.push('/');
            }
            if !base.ends_with('/') {
                base.push('/');
            }
            base.push_str(path_only);
            base
        };

        if combined.is_empty() {
            combined.push('/');
        }

        let path_and_query = if let Some(q) = query {
            format!("{}?{}", combined, q).parse()?
        } else {
            combined.parse()?
        };

        parts.path_and_query = Some(path_and_query);
        Ok(Uri::from_parts(parts)?)
    }

    fn resolve_request_encoding(&self) -> ContentEncoding {
        match self.request_compression_mode {
            RequestCompressionMode::Disabled => ContentEncoding::Identity,
            RequestCompressionMode::Force(enc) => enc,
            RequestCompressionMode::Auto => {
                if let Ok(guard) = self.negotiated_request_compression.read() {
                    guard.unwrap_or(AUTO_DEFAULT_ENCODING)
                } else {
                    AUTO_DEFAULT_ENCODING
                }
            }
        }
    }

    fn set_negotiated_encoding(&self, encoding: Option<ContentEncoding>) {
        if let Ok(mut guard) = self.negotiated_request_compression.write() {
            *guard = encoding;
        }
    }

    async fn probe_request_compression_support(&self) {
        if !self.request_compression_mode.is_auto() {
            return;
        }

        if let Ok(guard) = self.negotiated_request_compression.read()
            && guard.is_some()
        {
            return;
        }

        let propfind = match Method::from_bytes(b"PROPFIND") {
            Ok(m) => m,
            Err(_) => {
                self.set_negotiated_encoding(Some(ContentEncoding::Identity));
                return;
            }
        };

        let uri = match self.build_uri("") {
            Ok(u) => u,
            Err(_) => {
                self.set_negotiated_encoding(Some(ContentEncoding::Identity));
                return;
            }
        };

        let mut headers = HeaderMap::new();
        headers.insert("Depth", header::HeaderValue::from_static("0"));
        headers.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/xml; charset=utf-8"),
        );

        let mut req_builder = Request::builder().method(propfind).uri(uri);
        if let Some(auth) = &self.auth_header {
            req_builder = req_builder.header(header::AUTHORIZATION, auth);
        }

        add_accept_encoding(&mut headers);

        let probe_payload = Bytes::from_static(PROBE_BODY.as_bytes());
        let mut encoded_body = probe_payload.clone();
        if let Ok(compressed) = compress_payload(probe_payload.clone(), AUTO_DEFAULT_ENCODING).await
        {
            encoded_body = compressed;
            add_content_encoding(&mut headers, AUTO_DEFAULT_ENCODING);
        }

        for (k, v) in headers.iter() {
            req_builder = req_builder.header(k, v);
        }

        let req = match req_builder.body(Full::new(encoded_body)) {
            Ok(r) => r,
            Err(_) => {
                self.set_negotiated_encoding(Some(ContentEncoding::Identity));
                return;
            }
        };

        let fut = self.client.request(req);
        let result = timeout(Duration::from_secs(5), fut).await;

        match result {
            Ok(Ok(resp)) if resp.status().is_success() => {
                self.set_negotiated_encoding(Some(AUTO_DEFAULT_ENCODING));
            }
            _ => {
                self.set_negotiated_encoding(Some(ContentEncoding::Identity));
            }
        }
    }

    fn handle_request_compression_outcome(
        &self,
        attempted: Option<ContentEncoding>,
        status: StatusCode,
    ) -> bool {
        if !self.request_compression_mode.is_auto() {
            return false;
        }

        let Some(encoding) = attempted else {
            return false;
        };

        if matches!(
            status,
            StatusCode::UNSUPPORTED_MEDIA_TYPE
                | StatusCode::NOT_IMPLEMENTED
                | StatusCode::BAD_REQUEST
        ) {
            if let Ok(mut guard) = self.negotiated_request_compression.write() {
                *guard = Some(ContentEncoding::Identity);
            }
            return true;
        }

        if let Ok(mut guard) = self.negotiated_request_compression.write() {
            *guard = Some(encoding);
        }
        false
    }

    async fn prepare_request_body(
        &self,
        payload: Bytes,
        headers: &mut HeaderMap,
    ) -> (Bytes, Option<ContentEncoding>) {
        if self.request_compression_mode.is_auto() {
            let negotiated = self
                .negotiated_request_compression
                .read()
                .ok()
                .and_then(|g| *g);
            if negotiated.is_none() {
                let _probe_guard = self.request_compression_probe.lock().await;
                let negotiated = self
                    .negotiated_request_compression
                    .read()
                    .ok()
                    .and_then(|g| *g);
                if negotiated.is_none() {
                    self.probe_request_compression_support().await;
                }
            }
        }

        headers.remove(header::CONTENT_ENCODING);

        let encoding = self.resolve_request_encoding();
        if encoding == ContentEncoding::Identity {
            return (payload, None);
        }

        match compress_payload(payload.clone(), encoding).await {
            Ok(compressed) => {
                add_content_encoding(headers, encoding);
                (compressed, Some(encoding))
            }
            Err(_) => (payload, None),
        }
    }

    fn normalize_decompressed_headers(
        &self,
        headers: &mut HeaderMap,
        encodings: &[ContentEncoding],
        body_len: usize,
    ) {
        if encodings.is_empty() {
            return;
        }

        headers.remove(header::CONTENT_ENCODING);
        if let Ok(value) = header::HeaderValue::from_str(&body_len.to_string()) {
            headers.insert(header::CONTENT_LENGTH, value);
        } else {
            headers.remove(header::CONTENT_LENGTH);
        }
    }

    // ----------- Aggregated send (Bytes) with automatic decompression -----------

    /// Generic **aggregated send** with automatic decompression (br/zstd/gzip).
    pub async fn send(
        &self,
        method: Method,
        path: &str,
        headers: HeaderMap,
        body_bytes: Option<Bytes>,
        per_req_timeout: Option<Duration>,
    ) -> Result<Response<Bytes>> {
        let uri = self.build_uri(path)?;
        let auth = self.auth_header.clone();
        let base_headers = headers;
        let base_body = body_bytes.clone();
        let mut attempt = 0;

        loop {
            let mut headers = base_headers.clone();
            add_accept_encoding(&mut headers);

            let mut req_builder = Request::builder().method(method.clone()).uri(uri.clone());

            if let Some(ref auth_header) = auth {
                req_builder = req_builder.header(header::AUTHORIZATION, auth_header);
            }

            let mut final_body: Option<Bytes> = None;
            let mut attempted_encoding: Option<ContentEncoding> = None;

            if let Some(body) = base_body.clone() {
                if !headers.contains_key(header::CONTENT_TYPE) {
                    req_builder = req_builder.header(
                        header::CONTENT_TYPE,
                        header::HeaderValue::from_static("application/xml; charset=utf-8"),
                    );
                }

                let (payload, encoding) = self.prepare_request_body(body, &mut headers).await;
                attempted_encoding = encoding;
                final_body = Some(payload);
            }

            for (k, v) in headers.iter() {
                req_builder = req_builder.header(k, v);
            }

            let req = match final_body {
                Some(b) => req_builder.body(Full::new(b))?,
                None => req_builder.body(Full::new(Bytes::new()))?,
            };

            let fut = self.client.request(req);
            let resp = timeout(per_req_timeout.unwrap_or(self.default_timeout), fut)
                .await
                .map_err(|_| anyhow!("request timed out"))??;

            let should_retry =
                self.handle_request_compression_outcome(attempted_encoding, resp.status());
            if should_retry && attempt == 0 && base_body.is_some() {
                attempt += 1;
                continue;
            }

            let encodings = detect_encodings(resp.headers());
            let (mut parts, body) = resp.into_parts();

            let decompressed = decompress_body(body, &encodings).await?;
            self.normalize_decompressed_headers(&mut parts.headers, &encodings, decompressed.len());

            break Ok(Response::from_parts(parts, decompressed));
        }
    }

    // ----------- Streaming send (for parsing on the fly) -----------

    /// Generic **streaming send**. Returns a `Response<Incoming>` (not aggregated).
    pub async fn send_stream(
        &self,
        method: Method,
        path: &str,
        headers: HeaderMap,
        body_bytes: Option<Bytes>,
        per_req_timeout: Option<Duration>,
    ) -> Result<Response<Incoming>> {
        let uri = self.build_uri(path)?;
        let auth = self.auth_header.clone();
        let base_headers = headers;
        let base_body = body_bytes.clone();
        let mut attempt = 0;

        loop {
            let mut headers = base_headers.clone();
            add_accept_encoding(&mut headers);

            let mut req_builder = Request::builder().method(method.clone()).uri(uri.clone());

            if let Some(ref auth_header) = auth {
                req_builder = req_builder.header(header::AUTHORIZATION, auth_header);
            }

            let mut final_body: Option<Bytes> = None;
            let mut attempted_encoding: Option<ContentEncoding> = None;

            if let Some(body) = base_body.clone() {
                if !headers.contains_key(header::CONTENT_TYPE) {
                    req_builder = req_builder.header(
                        header::CONTENT_TYPE,
                        header::HeaderValue::from_static("application/xml; charset=utf-8"),
                    );
                }

                let (payload, encoding) = self.prepare_request_body(body, &mut headers).await;
                attempted_encoding = encoding;
                final_body = Some(payload);
            }

            for (k, v) in headers.iter() {
                req_builder = req_builder.header(k, v);
            }

            let req = match final_body {
                Some(body) => req_builder.body(Full::new(body))?,
                None => req_builder.body(Full::new(Bytes::new()))?,
            };

            let fut = self.client.request(req);
            let resp = timeout(per_req_timeout.unwrap_or(self.default_timeout), fut)
                .await
                .map_err(|_| anyhow!("request timed out"))??;

            let should_retry =
                self.handle_request_compression_outcome(attempted_encoding, resp.status());
            if should_retry && attempt == 0 && base_body.is_some() {
                attempt += 1;
                continue;
            }

            break Ok(resp);
        }
    }

    // ----------- HTTP/WebDAV Verbs -----------

    /// Send an `OPTIONS` request.
    pub async fn options(&self, path: &str) -> Result<Response<Bytes>> {
        self.send(Method::OPTIONS, path, HeaderMap::new(), None, None)
            .await
    }

    /// Send a `HEAD` request.
    pub async fn head(&self, path: &str) -> Result<Response<Bytes>> {
        self.send(Method::HEAD, path, HeaderMap::new(), None, None)
            .await
    }

    /// Send a `GET` request and return the fully aggregated (and decompressed) body.
    pub async fn get(&self, path: &str) -> Result<Response<Bytes>> {
        self.send(Method::GET, path, HeaderMap::new(), None, None)
            .await
    }

    /// Send a `DELETE` request.
    pub async fn delete(&self, path: &str) -> Result<Response<Bytes>> {
        self.send(Method::DELETE, path, HeaderMap::new(), None, None)
            .await
    }

    /// Conditional `DELETE` guarded by `If-Match`.
    pub async fn delete_if_match(&self, path: &str, etag: &str) -> Result<Response<Bytes>> {
        let mut h = HeaderMap::new();
        h.insert(header::IF_MATCH, header::HeaderValue::from_str(etag)?);
        self.send(Method::DELETE, path, h, None, None).await
    }

    /// Send a WebDAV `COPY` from `src_path` to an absolute `Destination` URL.
    pub async fn copy(
        &self,
        src_path: &str,
        dest_absolute_url: &str,
        overwrite: bool,
    ) -> Result<Response<Bytes>> {
        let mut h = HeaderMap::new();
        h.insert(
            "Destination",
            header::HeaderValue::from_str(dest_absolute_url)?,
        );
        h.insert(
            "Overwrite",
            header::HeaderValue::from_static(if overwrite { "T" } else { "F" }),
        );
        self.send(Method::from_bytes(b"COPY")?, src_path, h, None, None)
            .await
    }

    /// Send a WebDAV `MOVE` from `src_path` to an absolute `Destination` URL.
    pub async fn r#move(
        &self,
        src_path: &str,
        dest_absolute_url: &str,
        overwrite: bool,
    ) -> Result<Response<Bytes>> {
        let mut h = HeaderMap::new();
        h.insert(
            "Destination",
            header::HeaderValue::from_str(dest_absolute_url)?,
        );
        h.insert(
            "Overwrite",
            header::HeaderValue::from_static(if overwrite { "T" } else { "F" }),
        );
        self.send(Method::from_bytes(b"MOVE")?, src_path, h, None, None)
            .await
    }

    /// Send a WebDAV `PROPFIND` with a custom XML body and `Depth` header.
    pub async fn propfind(
        &self,
        path: &str,
        depth: Depth,
        xml_body: &str,
    ) -> Result<Response<Bytes>> {
        let mut h = HeaderMap::new();
        h.insert("Depth", header::HeaderValue::from_str(depth.as_str())?);
        h.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/xml; charset=utf-8"),
        );
        self.send(
            Method::from_bytes(b"PROPFIND")?,
            path,
            h,
            Some(Bytes::from(xml_body.to_owned())),
            None,
        )
        .await
    }

    /// Send a WebDAV `PROPPATCH` with a custom XML body.
    pub async fn proppatch(&self, path: &str, xml_body: &str) -> Result<Response<Bytes>> {
        let mut h = HeaderMap::new();
        h.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/xml; charset=utf-8"),
        );
        self.send(
            Method::from_bytes(b"PROPPATCH")?,
            path,
            h,
            Some(Bytes::from(xml_body.to_owned())),
            None,
        )
        .await
    }

    /// Send a WebDAV `REPORT` with a custom XML body and `Depth`.
    pub async fn report(
        &self,
        path: &str,
        depth: Depth,
        xml_body: &str,
    ) -> Result<Response<Bytes>> {
        let mut h = HeaderMap::new();
        h.insert("Depth", header::HeaderValue::from_str(depth.as_str())?);
        h.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/xml; charset=utf-8"),
        );
        self.send(
            Method::from_bytes(b"REPORT")?,
            path,
            h,
            Some(Bytes::from(xml_body.to_owned())),
            None,
        )
        .await
    }

    /// Send a WebDAV `MKCOL` to create a generic collection. Some servers accept an optional XML body.
    pub async fn mkcol(&self, path: &str, xml_body: Option<&str>) -> Result<Response<Bytes>> {
        let mut h = HeaderMap::new();
        let body = xml_body.map(|s| {
            h.insert(
                header::CONTENT_TYPE,
                header::HeaderValue::from_static("application/xml; charset=utf-8"),
            );
            Bytes::from(s.to_owned())
        });
        self.send(Method::from_bytes(b"MKCOL")?, path, h, body, None)
            .await
    }

    /// Extract the `ETag` from a response header map, if present.
    pub fn etag_from_headers(headers: &HeaderMap) -> Option<String> {
        headers
            .get(header::ETAG)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
    }

    /// Run many `PROPFIND`s concurrently with a semaphore-bound concurrency limit.
    pub async fn propfind_many(
        &self,
        paths: impl IntoIterator<Item = String>,
        depth: Depth,
        xml_body: Arc<Bytes>,
        max_concurrency: usize,
    ) -> Vec<BatchItem<Response<Bytes>>> {
        let sem = Arc::new(Semaphore::new(max_concurrency.max(1)));
        let mut tasks = FuturesOrdered::new();

        for path in paths {
            let sem_clone = sem.clone();
            let this = self.clone();
            let body = xml_body.clone();
            let p = path.clone();
            tasks.push_back(async move {
                let _permit: OwnedSemaphorePermit =
                    sem_clone.acquire_owned().await.expect("semaphore closed");
                let mut h = HeaderMap::new();
                h.insert(
                    "Depth",
                    header::HeaderValue::from_str(depth.as_str()).unwrap(),
                );
                h.insert(
                    header::CONTENT_TYPE,
                    header::HeaderValue::from_static("application/xml; charset=utf-8"),
                );
                let res = this
                    .send(
                        Method::from_bytes(b"PROPFIND").unwrap(),
                        &p,
                        h,
                        Some((*body).clone()),
                        None,
                    )
                    .await;
                BatchItem {
                    pub_path: p,
                    result: res,
                }
            });
        }

        let mut out = Vec::new();
        while let Some(item) = tasks.next().await {
            out.push(item);
        }
        out
    }

    /// Run many `REPORT`s concurrently with a semaphore-bound concurrency limit.
    pub async fn report_many(
        &self,
        paths: impl IntoIterator<Item = String>,
        depth: Depth,
        xml_body: Arc<Bytes>,
        max_concurrency: usize,
    ) -> Vec<BatchItem<Response<Bytes>>> {
        let sem = Arc::new(Semaphore::new(max_concurrency.max(1)));
        let mut tasks = FuturesOrdered::new();

        for path in paths {
            let sem_clone = sem.clone();
            let this = self.clone();
            let body = xml_body.clone();
            let p = path.clone();
            tasks.push_back(async move {
                let _permit: OwnedSemaphorePermit =
                    sem_clone.acquire_owned().await.expect("semaphore closed");
                let mut h = HeaderMap::new();
                h.insert(
                    "Depth",
                    header::HeaderValue::from_str(depth.as_str()).unwrap(),
                );
                h.insert(
                    header::CONTENT_TYPE,
                    header::HeaderValue::from_static("application/xml; charset=utf-8"),
                );
                let res = this
                    .send(
                        Method::from_bytes(b"REPORT").unwrap(),
                        &p,
                        h,
                        Some((*body).clone()),
                        None,
                    )
                    .await;
                BatchItem {
                    pub_path: p,
                    result: res,
                }
            });
        }

        let mut out = Vec::new();
        while let Some(item) = tasks.next().await {
            out.push(item);
        }
        out
    }

    /// Check if the server supports WebDAV-Sync (RFC 6578).
    pub async fn supports_webdav_sync(&self) -> Result<bool> {
        let options_resp = self.options("").await?;
        if let Some(allow_header) = options_resp.headers().get("Allow")
            && let Ok(allow_value) = allow_header.to_str()
            && allow_value.contains("REPORT")
        {
            return Ok(true);
        }

        let test_sync = r#"<D:sync-collection xmlns:D="DAV:">
            <D:sync-token/>
            <D:sync-level>1</D:sync-level>
            <D:prop>
                <D:getetag/>
            </D:prop>
        </D:sync-collection>"#;

        match self.report("", Depth::One, test_sync).await {
            Ok(response) => {
                let status = response.status();
                Ok(status.is_success() || status == 415)
            }
            Err(_) => Ok(false),
        }
    }

    /// Streaming variant of `PROPFIND`, returning the non-aggregated body.
    pub async fn propfind_stream(
        &self,
        path: &str,
        depth: Depth,
        xml_body: &str,
    ) -> Result<Response<Incoming>> {
        let mut h = HeaderMap::new();
        h.insert("Depth", header::HeaderValue::from_str(depth.as_str())?);
        h.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/xml; charset=utf-8"),
        );
        self.send_stream(
            Method::from_bytes(b"PROPFIND")?,
            path,
            h,
            Some(Bytes::from(xml_body.to_owned())),
            None,
        )
        .await
    }

    /// Streaming variant of `REPORT`, returning the non-aggregated body.
    pub async fn report_stream(
        &self,
        path: &str,
        depth: Depth,
        xml_body: &str,
    ) -> Result<Response<Incoming>> {
        let mut h = HeaderMap::new();
        h.insert("Depth", header::HeaderValue::from_str(depth.as_str())?);
        h.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/xml; charset=utf-8"),
        );
        self.send_stream(
            Method::from_bytes(b"REPORT")?,
            path,
            h,
            Some(Bytes::from(xml_body.to_owned())),
            None,
        )
        .await
    }
}
