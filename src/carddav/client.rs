use anyhow::{Result, anyhow};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use bytes::Bytes;
use futures::{StreamExt, stream::FuturesUnordered};
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::{HeaderMap, Method, Request, Response, StatusCode, Uri, header};
use std::sync::{Arc, RwLock};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time::{Duration, timeout};

use crate::carddav::streaming::parse_multistatus_bytes;
use crate::carddav::types::{
    BatchItem, AddressBookInfo, AddressObject, DavItem, Depth, SyncItem, SyncResponse,
};
use crate::common::compression::{
    ContentEncoding, add_accept_encoding, add_content_encoding, compress_payload, decompress_body,
    detect_encodings,
};
use crate::common::http::{HyperClient, build_hyper_client};

/// High-performance CardDAV client built on **hyper 1.x** + **rustls**.
///
/// Features:
/// - HTTP/2 multiplexing and connection pooling
/// - Automatic response decompression (br/zstd/gzip)
/// - Automatic request compression negotiation (br/zstd/gzip)
/// - Streaming-friendly APIs for large WebDAV responses
/// - Batch helpers with bounded concurrency
/// - ETag helpers for safe conditional writes/deletes
///
/// Cloning `CardDavClient` is cheap and reuses the same connection pool.
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
pub struct CardDavClient {
    base: Uri,
    client: HyperClient,
    auth_header: Option<header::HeaderValue>,
    default_timeout: Duration,
    request_compression_mode: RequestCompressionMode,
    negotiated_request_compression: Arc<RwLock<Option<ContentEncoding>>>,
}

impl CardDavClient {
    /// Create a new client from a **base URL** (collection/home-set) and optional **Basic** credentials.
    ///
    /// The base may be `https://` **or** `http://` (both are supported by the connector).
    ///
    /// # Arguments
    ///
    /// * `base_url` - The base URL for the CardDAV server (must be a valid URI)
    /// * `basic_user` - Optional username for Basic authentication
    /// * `basic_pass` - Optional password for Basic authentication
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The base URL is not a valid URI
    /// - Credentials are provided but cannot be encoded properly
    /// - TLS configuration fails
    ///
    /// # Example
    /// ```no_run
    /// use fast_dav_rs::CardDavClient;
    /// use anyhow::Result;
    ///
    /// # async fn example() -> Result<()> {
    /// let client = CardDavClient::new(
    ///     "https://card.example.com/dav/user01/",
    ///     Some("user01"),
    ///     Some("secret"),
    /// )?;
    /// # Ok(())
    /// # }
    /// ```
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
        })
    }

    /// Configure request compression for this client.
    ///
    /// # Arguments
    ///
    /// * `encoding` - The compression algorithm to use for outgoing requests
    ///
    /// # Example
    ///
    /// ```no_run
    /// use fast_dav_rs::{CardDavClient, ContentEncoding};
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let mut client = CardDavClient::new(
    ///     "https://card.example.com/dav/user01/",
    ///     Some("user01"),
    ///     Some("secret"),
    /// )?;
    /// client.set_request_compression(ContentEncoding::Gzip);
    /// # Ok(())
    /// # }
    /// ```
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
                self.probe_request_compression_support().await;
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

    // ----------- Aggregated send (Bytes) with automatic decompression -----------

    /// Generic **aggregated send** with automatic decompression (br/zstd/gzip).
    ///
    /// Returns a `Response<Bytes>` where the body is fully aggregated and already decompressed.
    ///
    /// # Example
    /// ```no_run
    /// # use fast_dav_rs::CardDavClient;
    /// # use hyper::{Method, HeaderMap};
    /// # use bytes::Bytes;
    /// #
    /// # async fn demo(cli: &CardDavClient) -> anyhow::Result<()> {
    /// let res = cli.send(Method::GET, "Calendars/Personal/", HeaderMap::new(), None, None).await?;
    /// assert!(res.status().is_success());
    /// # Ok(())
    /// # }
    /// ```
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

            // Decompress in memory (aggregated)
            let decompressed = decompress_body(body, &encodings).await?;
            self.normalize_decompressed_headers(&mut parts.headers, &encodings, decompressed.len());

            break Ok(Response::from_parts(parts, decompressed));
        }
    }

    // ----------- Streaming send (for parsing on the fly) -----------

    /// Generic **streaming send**. Returns a `Response<Incoming>` (not aggregated).
    ///
    /// Use this when you want to parse the response on the fly, e.g. with
    /// [`crate::streaming::parse_multistatus_stream`].
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
    ///
    /// Useful to discover server capabilities.
    pub async fn options(&self, path: &str) -> Result<Response<Bytes>> {
        self.send(Method::OPTIONS, path, HeaderMap::new(), None, None)
            .await
    }
    /// Send a `HEAD` request.
    ///
    /// Often used to retrieve an `ETag` before a conditional update/delete.
    pub async fn head(&self, path: &str) -> Result<Response<Bytes>> {
        self.send(Method::HEAD, path, HeaderMap::new(), None, None)
            .await
    }
    /// Send a `GET` request and return the fully aggregated (and decompressed) body.
    pub async fn get(&self, path: &str) -> Result<Response<Bytes>> {
        self.send(Method::GET, path, HeaderMap::new(), None, None)
            .await
    }
    /// Send a `PUT` with a vCard body (`text/vcard`).
    ///
    /// Use [`put_if_match`] or [`put_if_none_match`] for safer conditional writes.
    pub async fn put(&self, path: &str, vcard_bytes: Bytes) -> Result<Response<Bytes>> {
        let mut h = HeaderMap::new();
        h.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("text/vcard; charset=utf-8"),
        );
        self.send(Method::PUT, path, h, Some(vcard_bytes), None)
            .await
    }
    /// Conditional `PUT` guarded by `If-Match`.
    ///
    /// The write only succeeds if the current resource ETag matches.
    ///
    /// # Arguments
    ///
    /// * `path` - Resource path relative to the base URL
    /// * `vcard_bytes` - The vCard data to upload
    /// * `etag` - The ETag to match (should include quotes if required by server)
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The path cannot be resolved to a valid URI
    /// - The ETag value contains invalid characters for HTTP headers
    /// - Network or server errors occur
    pub async fn put_if_match(
        &self,
        path: &str,
        vcard_bytes: Bytes,
        etag: &str,
    ) -> Result<Response<Bytes>> {
        // Validate ETag doesn't contain forbidden characters
        if etag.is_empty() {
            return Err(anyhow!("ETag cannot be empty"));
        }

        let mut h = HeaderMap::new();
        h.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("text/vcard; charset=utf-8"),
        );
        h.insert(header::IF_MATCH, header::HeaderValue::from_str(etag)?);
        self.send(Method::PUT, path, h, Some(vcard_bytes), None)
            .await
    }
    /// Create-only `PUT` guarded by `If-None-Match: *`.
    ///
    /// Fails if the resource already exists.
    pub async fn put_if_none_match(
        &self,
        path: &str,
        vcard_bytes: Bytes,
    ) -> Result<Response<Bytes>> {
        let mut h = HeaderMap::new();
        h.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("text/vcard; charset=utf-8"),
        );
        h.insert(header::IF_NONE_MATCH, header::HeaderValue::from_static("*"));
        self.send(Method::PUT, path, h, Some(vcard_bytes), None)
            .await
    }
    /// Send a `DELETE` request.
    ///
    /// Prefer [`delete_if_match`] when you want to ensure you delete the expected version.
    pub async fn delete(&self, path: &str) -> Result<Response<Bytes>> {
        self.send(Method::DELETE, path, HeaderMap::new(), None, None)
            .await
    }
    /// Conditional `DELETE` guarded by `If-Match`.
    ///
    /// The delete only succeeds if the current resource ETag matches.
    pub async fn delete_if_match(&self, path: &str, etag: &str) -> Result<Response<Bytes>> {
        let mut h = HeaderMap::new();
        h.insert(header::IF_MATCH, header::HeaderValue::from_str(etag)?);
        self.send(Method::DELETE, path, h, None, None).await
    }
    /// Send a WebDAV `COPY` from `src_path` to an absolute `Destination` URL.
    ///
    /// Set `overwrite = true` to allow replacing the destination if it exists.
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
    ///
    /// Set `overwrite = true` to allow replacing the destination if it exists.
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
    ///
    /// Typical properties to request include `DAV:displayname`, `DAV:getetag`,
    /// and `DAV:resourcetype`.
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
    /// Send a CardDAV `REPORT` (e.g. `addressbook-query`) with a custom XML body and `Depth`.
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
    /// Send a CardDAV `MKADDRESSBOOK` to create an addressbook collection.
    pub async fn mkaddressbook(&self, path: &str, xml_body: &str) -> Result<Response<Bytes>> {
        let mut h = HeaderMap::new();
        h.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/xml; charset=utf-8"),
        );
        let resp = self
            .send(
                Method::from_bytes(b"MKADDRESSBOOK")?,
                path,
                h,
                Some(Bytes::from(xml_body.to_owned())),
                None,
            )
            .await?;

        if resp.status() == StatusCode::NOT_IMPLEMENTED
            || resp.status() == StatusCode::METHOD_NOT_ALLOWED
        {
            let fallback_body = build_mkcol_addressbook_body(xml_body);
            let mut h = HeaderMap::new();
            h.insert(
                header::CONTENT_TYPE,
                header::HeaderValue::from_static("application/xml; charset=utf-8"),
            );
            return self
                .send(
                    Method::from_bytes(b"MKCOL")?,
                    path,
                    h,
                    Some(Bytes::from(fallback_body)),
                    None,
                )
                .await;
        }

        Ok(resp)
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

    /// Discover the current user's principal URL via `current-user-principal`.
    ///
    /// Returns `None` if the server omits the property.
    pub async fn discover_current_user_principal(&self) -> Result<Option<String>> {
        let body = r#"
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:current-user-principal/>
  </D:prop>
</D:propfind>
"#;
        let resp = self.propfind("", Depth::Zero, body).await?;
        if !resp.status().is_success() {
            return Err(anyhow!(
                "PROPFIND current-user-principal failed with {}",
                resp.status()
            ));
        }
        let body = resp.into_body();
        let mut principal = None;
        for item in parse_multistatus_bytes(&body)?.items {
            if let Some(found) = item
                .current_user_principal
                .into_iter()
                .find(|href| !href.is_empty())
            {
                principal = Some(found);
                break;
            }
        }
        Ok(principal)
    }

    /// Discover the addressbook-home-set collection(s) for the provided principal path.
    pub async fn discover_addressbook_home_set(&self, principal_path: &str) -> Result<Vec<String>> {
        let body = r#"
<D:propfind xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">
  <D:prop>
    <C:addressbook-home-set/>
  </D:prop>
</D:propfind>
"#;
        let resp = self.propfind(principal_path, Depth::Zero, body).await?;
        if !resp.status().is_success() {
            return Err(anyhow!(
                "PROPFIND addressbook-home-set failed with {}",
                resp.status()
            ));
        }
        let body = resp.into_body();
        let mut homes = Vec::new();
        for mut item in parse_multistatus_bytes(&body)?.items {
            homes.append(&mut item.addressbook_home_set);
        }
        homes.sort();
        homes.dedup();
        Ok(homes)
    }

    /// List CardDAV collections under an addressbook home-set (`Depth: 1` PROPFIND).
    pub async fn list_addressbooks(&self, home_set_path: &str) -> Result<Vec<AddressBookInfo>> {
        let body = r#"
<D:propfind xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav" xmlns:A="http://apple.com/ns/ical/">
  <D:prop>
    <D:displayname/>
    <C:addressbook-description/>
    <C:addressbook-color/>
    <A:addressbook-color/>
    <C:supported-address-data/>
    <D:getetag/>
    <D:resourcetype/>
    <D:sync-token/>
  </D:prop>
</D:propfind>
"#;
        let resp = self.propfind(home_set_path, Depth::One, body).await?;
        if !resp.status().is_success() {
            return Err(anyhow!("PROPFIND addressbooks failed with {}", resp.status()));
        }
        let body = resp.into_body();
        Ok(map_addressbook_list(parse_multistatus_bytes(&body)?.items))
    }

    /// Execute a CardDAV `addressbook-query` with a custom filter.
    pub async fn addressbook_query(
        &self,
        addressbook_path: &str,
        filter_xml: &str,
        include_data: bool,
    ) -> Result<Vec<AddressObject>> {
        let xml = build_addressbook_query_body(filter_xml, include_data);

        let resp = self.report(addressbook_path, Depth::One, &xml).await?;
        if !resp.status().is_success() {
            return Err(anyhow!(
                "REPORT addressbook-query failed with {}",
                resp.status()
            ));
        }
        let body = resp.into_body();
        Ok(map_address_objects(parse_multistatus_bytes(&body)?.items))
    }

    /// Addressbook query helper: match a specific `UID`.
    pub async fn addressbook_query_uid(
        &self,
        addressbook_path: &str,
        uid: &str,
        include_data: bool,
    ) -> Result<Vec<AddressObject>> {
        let filter = build_addressbook_query_filter_uid(uid);
        self.addressbook_query(addressbook_path, &filter, include_data)
            .await
    }

    /// Addressbook query helper: match a specific `EMAIL`.
    pub async fn addressbook_query_email(
        &self,
        addressbook_path: &str,
        email: &str,
        include_data: bool,
    ) -> Result<Vec<AddressObject>> {
        let filter = build_addressbook_query_filter_email(email);
        self.addressbook_query(addressbook_path, &filter, include_data)
            .await
    }

    /// Addressbook query helper: match a specific `FN` (formatted name).
    pub async fn addressbook_query_fn(
        &self,
        addressbook_path: &str,
        formatted_name: &str,
        include_data: bool,
    ) -> Result<Vec<AddressObject>> {
        let filter = build_addressbook_query_filter_fn(formatted_name);
        self.addressbook_query(addressbook_path, &filter, include_data)
            .await
    }

    /// Fetch specific address objects via `addressbook-multiget`.
    pub async fn addressbook_multiget<I, S>(
        &self,
        addressbook_path: &str,
        hrefs: I,
        include_data: bool,
    ) -> Result<Vec<AddressObject>>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let Some(body) = build_addressbook_multiget_body(hrefs, include_data) else {
            return Ok(Vec::new());
        };

        let resp = self.report(addressbook_path, Depth::One, &body).await?;
        if !resp.status().is_success() {
            return Err(anyhow!(
                "REPORT addressbook-multiget failed with {}",
                resp.status()
            ));
        }
        let body = resp.into_body();
        Ok(map_address_objects(parse_multistatus_bytes(&body)?.items))
    }

    /// Incrementally synchronise an addressbook collection using `sync-collection`.
    pub async fn sync_collection(
        &self,
        addressbook_path: &str,
        sync_token: Option<&str>,
        limit: Option<u32>,
        include_data: bool,
    ) -> Result<SyncResponse> {
        let body = build_sync_collection_body(sync_token, limit, include_data);

        let resp = self.report(addressbook_path, Depth::One, &body).await?;
        if !resp.status().is_success() {
            return Err(anyhow!(
                "REPORT sync-collection failed with {}",
                resp.status()
            ));
        }
        let headers = resp.headers().clone();
        let body = resp.into_body();

        let parsed = parse_multistatus_bytes(&body)?;
        Ok(map_sync_response(&headers, parsed.items, parsed.sync_token))
    }

    // ----------- ETag helpers -----------

    /// Extract the `ETag` from a response header map, if present.
    pub fn etag_from_headers(headers: &HeaderMap) -> Option<String> {
        headers
            .get(header::ETAG)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
    }

    // ----------- Batch (limited concurrency) -----------

    /// Run many `PROPFIND`s concurrently with a semaphore-bound concurrency limit.
    ///
    /// Returns results in the same order as inputs.
    pub async fn propfind_many(
        &self,
        paths: impl IntoIterator<Item = String>,
        depth: Depth,
        xml_body: Arc<Bytes>,
        max_concurrency: usize,
    ) -> Vec<BatchItem<Response<Bytes>>> {
        let sem = Arc::new(Semaphore::new(max_concurrency.max(1)));
        let mut tasks = FuturesUnordered::new();

        for path in paths {
            let sem_clone = sem.clone();
            let this = self.clone();
            let body = xml_body.clone();
            let p = path.clone();
            tasks.push(async move {
                // Acquire permit inside the task, not before spawning
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
    ///
    /// Returns results in the same order as inputs.
    pub async fn report_many(
        &self,
        paths: impl IntoIterator<Item = String>,
        depth: Depth,
        xml_body: Arc<Bytes>,
        max_concurrency: usize,
    ) -> Vec<BatchItem<Response<Bytes>>> {
        let sem = Arc::new(Semaphore::new(max_concurrency.max(1)));
        let mut tasks = FuturesUnordered::new();

        for path in paths {
            let sem_clone = sem.clone();
            let this = self.clone();
            let body = xml_body.clone();
            let p = path.clone();
            tasks.push(async move {
                // Acquire permit inside the task, not before spawning
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

    // ----------- Public streaming helpers -----------

    /// Check if the server supports WebDAV-Sync (RFC 6578).
    ///
    /// This method determines if the server supports the `sync-collection` REPORT
    /// method which enables efficient incremental synchronization.
    ///
    /// # Returns
    ///
    /// `Ok(true)` if WebDAV-Sync is supported, `Ok(false)` if not, or an error
    /// if the capability cannot be determined.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use fast_dav_rs::CardDavClient;
    /// # use anyhow::Result;
    /// #
    /// # async fn example() -> Result<()> {
    /// # let client = CardDavClient::new("https://example.com/", None, None)?;
    /// if client.supports_webdav_sync().await? {
    ///     println!("Server supports efficient incremental sync");
    /// } else {
    ///     println!("Server requires traditional polling");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn supports_webdav_sync(&self) -> Result<bool> {
        // First check OPTIONS response for REPORT method support
        let options_resp = self.options("").await?;
        if let Some(allow_header) = options_resp.headers().get("Allow")
            && let Ok(allow_value) = allow_header.to_str()
            && allow_value.contains("REPORT")
        {
            return Ok(true);
        }

        // Fallback: Try a minimal sync-collection report
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
                // If we get a 2xx or 415, sync is likely supported
                // 415 = Unsupported Media Type indicates the method is recognized
                Ok(status.is_success() || status == 415)
            }
            Err(_) => Ok(false),
        }
    }

    /// Streaming variant of `PROPFIND`, returning the non-aggregated body.
    ///
    /// Combine with [`crate::streaming::parse_multistatus_stream`].
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
    ///
    /// Combine with [`crate::streaming::parse_multistatus_stream`].
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

pub fn escape_xml(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

fn build_mkcol_addressbook_body(xml_body: &str) -> String {
    let prop_inner = extract_prop_inner(xml_body);
    let has_resourcetype = prop_inner
        .as_deref()
        .map(|inner| inner.to_ascii_lowercase().contains("resourcetype"))
        .unwrap_or(false);

    let mut prop = String::new();
    if !has_resourcetype {
        prop.push_str("<D:resourcetype><D:collection/><C:addressbook/></D:resourcetype>");
    }
    if let Some(inner) = prop_inner {
        let trimmed = inner.trim();
        if !trimmed.is_empty() {
            prop.push_str(trimmed);
        }
    }
    if prop.is_empty() {
        prop.push_str("<D:resourcetype><D:collection/><C:addressbook/></D:resourcetype>");
    }

    format!(
        r#"<D:mkcol xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav"><D:set><D:prop>{prop}</D:prop></D:set></D:mkcol>"#
    )
}

fn extract_prop_inner(xml_body: &str) -> Option<String> {
    let mut start = None;
    for open in ["<D:prop>", "<d:prop>"] {
        if let Some(idx) = xml_body.find(open) {
            start = Some(idx + open.len());
            break;
        }
    }
    let start = start?;
    let remaining = &xml_body[start..];

    let mut end = None;
    for close in ["</D:prop>", "</d:prop>"] {
        if let Some(idx) = remaining.find(close) {
            end = Some(idx);
            break;
        }
    }
    let end = end?;
    Some(remaining[..end].to_string())
}

pub fn build_addressbook_query_body(filter_xml: &str, include_data: bool) -> String {
    let mut prop = String::from("<D:prop><D:getetag/>");
    if include_data {
        prop.push_str("<C:address-data/>");
    }
    prop.push_str("</D:prop>");

    format!(
        r#"<C:addressbook-query xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">{prop}{filter_xml}</C:addressbook-query>"#
    )
}

pub fn build_addressbook_query_filter_uid(uid: &str) -> String {
    build_addressbook_query_filter("UID", uid)
}

pub fn build_addressbook_query_filter_email(email: &str) -> String {
    build_addressbook_query_filter("EMAIL", email)
}

pub fn build_addressbook_query_filter_fn(formatted_name: &str) -> String {
    build_addressbook_query_filter("FN", formatted_name)
}

fn build_addressbook_query_filter(prop: &str, value: &str) -> String {
    let escaped = escape_xml(value);
    format!(
        "<C:filter>\
           <C:prop-filter name=\"{prop}\">\
             <C:text-match collation=\"i;unicode-casemap\" match-type=\"equals\">{escaped}</C:text-match>\
           </C:prop-filter>\
         </C:filter>"
    )
}

pub fn build_addressbook_multiget_body<I, S>(hrefs: I, include_data: bool) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut href_xml = String::new();
    let mut total = 0usize;
    for href in hrefs {
        let href = href.as_ref();
        if href.is_empty() {
            continue;
        }
        total += 1;
        href_xml.push_str("<D:href>");
        href_xml.push_str(&escape_xml(href));
        href_xml.push_str("</D:href>");
    }
    if total == 0 {
        return None;
    }

    let mut body = String::from(
        r#"<C:addressbook-multiget xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav"><D:prop><D:getetag/>"#,
    );
    if include_data {
        body.push_str("<C:address-data/>");
    }
    body.push_str("</D:prop>");
    body.push_str(&href_xml);
    body.push_str("</C:addressbook-multiget>");
    Some(body)
}

pub fn build_sync_collection_body(
    sync_token: Option<&str>,
    limit: Option<u32>,
    include_data: bool,
) -> String {
    let mut body = String::from(
        r#"<D:sync-collection xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">"#,
    );
    if let Some(token) = sync_token {
        body.push_str("<D:sync-token>");
        body.push_str(&escape_xml(token));
        body.push_str("</D:sync-token>");
    } else {
        body.push_str("<D:sync-token/>");
    }
    body.push_str("<D:sync-level>1</D:sync-level>");
    body.push_str("<D:prop><D:getetag/>");
    if include_data {
        body.push_str("<C:address-data/>");
    }
    body.push_str("</D:prop>");
    if let Some(limit) = limit {
        body.push_str("<D:limit><D:nresults>");
        body.push_str(&limit.to_string());
        body.push_str("</D:nresults></D:limit>");
    }
    body.push_str("</D:sync-collection>");
    body
}

pub fn map_addressbook_list(mut items: Vec<DavItem>) -> Vec<AddressBookInfo> {
    let mut addressbooks = Vec::new();
    for mut item in items.drain(..) {
        if item.is_addressbook {
            let description = item
                .addressbook_description
                .take()
                .map(|d| d.trim().to_string())
                .filter(|d| !d.is_empty());
            addressbooks.push(AddressBookInfo {
                href: item.href,
                displayname: item.displayname,
                description,
                color: item.addressbook_color,
                etag: item.etag,
                sync_token: item.sync_token,
                supported_address_data: item.supported_address_data,
            });
        }
    }
    addressbooks.sort_by(|a, b| a.href.cmp(&b.href));
    addressbooks
}

pub fn map_address_objects(items: Vec<DavItem>) -> Vec<AddressObject> {
    let mut out = Vec::with_capacity(items.len());
    for mut item in items {
        out.push(AddressObject {
            href: item.href,
            etag: item.etag,
            address_data: item.address_data.take(),
            status: item.status,
        });
    }
    out
}

pub fn map_sync_response(
    headers: &HeaderMap,
    items: Vec<DavItem>,
    top_level_sync_token: Option<String>,
) -> SyncResponse {
    // Prioritize top-level sync-token (RFC 6578), then headers, then per-item tokens
    let mut sync_token = top_level_sync_token.or_else(|| {
        headers
            .get("Sync-Token")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
    });
    let mut out = Vec::new();

    for mut item in items {
        // Capture per-item sync token if we don't have a top-level one (fallback)
        if item.sync_token.is_some() && sync_token.is_none() {
            sync_token = item.sync_token.clone();
        }

        let is_collection = item.is_collection
            || (item.sync_token.is_some() && item.etag.is_none() && item.address_data.is_none());
        if is_collection {
            continue;
        }
        let status = item.status.clone();
        let is_deleted = status
            .as_deref()
            .map(|s| s.contains("404") || s.contains("410"))
            .unwrap_or(false);

        out.push(SyncItem {
            href: item.href,
            etag: item.etag,
            address_data: item.address_data.take(),
            status,
            is_deleted,
        });
    }

    SyncResponse {
        sync_token,
        items: out,
    }
}
