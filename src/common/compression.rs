//! Compression utilities for HTTP content encoding.
//!
//! This module provides support for automatic compression and decompression
//! of HTTP responses using various encoding formats.

use async_compression::tokio::bufread::{BrotliDecoder, GzipDecoder, ZstdDecoder};
use bytes::Bytes;
use futures::TryStreamExt;
use http_body_util::BodyStream;
use hyper::body::Incoming;
use hyper::{HeaderMap, header, http};
use std::io::Cursor;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncBufRead, AsyncRead, AsyncReadExt, BufReader, ReadBuf};
use tokio_util::io::StreamReader;

use crate::{Error, Result};

/// Maximum size (in bytes) a decompressed response body may reach before it
/// is rejected. Guards against decompression bombs.
pub(crate) const MAX_DECOMPRESSED_SIZE: u64 = 256 * 1024 * 1024;

/// Supported content encodings for streaming decompression.
///
/// These values correspond to the `Content-Encoding` header and are used by
/// the decompression functions to decide how to wrap the body reader.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentEncoding {
    Identity,
    Br,
    Gzip,
    Zstd,
}

impl ContentEncoding {
    pub fn as_str(&self) -> &'static str {
        match self {
            ContentEncoding::Identity => "identity",
            ContentEncoding::Br => "br",
            ContentEncoding::Gzip => "gzip",
            ContentEncoding::Zstd => "zstd",
        }
    }
}

/// Detect the response `Content-Encoding` header and return the ordered chain of encodings.
///
/// The vector is ordered from outermost encoding to innermost (as received). When empty, the
/// payload is identity encoded.
pub fn detect_encodings(headers: &HeaderMap) -> Vec<ContentEncoding> {
    let Some(val) = headers.get(header::CONTENT_ENCODING) else {
        return Vec::new();
    };

    let Ok(raw) = val.to_str() else {
        return Vec::new();
    };

    raw.split(',')
        .filter_map(|token| {
            let enc = token.trim().to_ascii_lowercase();
            Some(match enc.as_str() {
                "br" => ContentEncoding::Br,
                "gzip" => ContentEncoding::Gzip,
                "zstd" | "zst" => ContentEncoding::Zstd,
                "identity" => return None,
                _ => return None,
            })
        })
        .collect()
}

/// Insert an `Accept-Encoding` header (`br, zstd, gzip`) if not already present.
///
/// This hints to the server that the client supports compressed responses.
pub fn add_accept_encoding(h: &mut HeaderMap) {
    if !h.contains_key(header::ACCEPT_ENCODING) {
        h.insert(
            header::ACCEPT_ENCODING,
            http::HeaderValue::from_static("br, zstd, gzip"),
        );
    }
}

/// Detect the most efficient request compression supported by the server.
///
/// This inspects the server's `Accept-Encoding` response header and applies
/// quality factors (`q=` weights) to pick the optimal [`ContentEncoding`]
/// supported by both parties. Returns `None` when the header is absent or when
/// no mutually supported encoding is advertised.
pub fn detect_request_compression_preference(headers: &HeaderMap) -> Option<ContentEncoding> {
    let raw = headers.get(header::ACCEPT_ENCODING)?.to_str().ok()?;

    let mut wildcard_q: Option<f32> = None;
    let mut identity_q: f32 = 1.0;
    let mut identity_explicit = false;
    let mut entries: Vec<(String, f32)> = Vec::new();

    for part in raw.split(',') {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }

        let mut segments = trimmed.split(';');
        let token = segments.next().unwrap().trim().to_ascii_lowercase();
        if token.is_empty() {
            continue;
        }

        let mut weight = 1.0_f32;
        for param in segments {
            if let Some((key, value)) = param.split_once('=') {
                if key.trim().eq_ignore_ascii_case("q") {
                    if let Ok(parsed) = value.trim().parse::<f32>() {
                        weight = parsed.clamp(0.0, 1.0);
                    }
                }
            }
        }

        match token.as_str() {
            "identity" => {
                identity_q = weight;
                identity_explicit = true;
            }
            "*" => {
                wildcard_q = Some(weight);
            }
            other => entries.push((other.to_string(), weight)),
        }
    }

    if !identity_explicit {
        if let Some(q) = wildcard_q {
            identity_q = q;
        }
    }

    let mut best: Option<(ContentEncoding, f32)> = None;
    for candidate in [
        ContentEncoding::Br,
        ContentEncoding::Zstd,
        ContentEncoding::Gzip,
    ] {
        let direct_q = entries.iter().find_map(|(name, q)| {
            if name == candidate.as_str() {
                Some(*q)
            } else {
                None
            }
        });
        let effective_q = direct_q.or(wildcard_q);

        if let Some(q) = effective_q {
            if q <= 0.0 {
                continue;
            }

            let should_replace = best
                .map(|(_, best_q)| q > best_q + f32::EPSILON)
                .unwrap_or(true);
            if should_replace {
                best = Some((candidate, q));
            }
        }
    }

    if let Some((encoding, _)) = best {
        return Some(encoding);
    }

    if identity_q > 0.0 {
        return Some(ContentEncoding::Identity);
    }

    None
}

/// Backwards-compatible helper that returns the first encoding in the chain or identity when none.
pub fn detect_encoding(headers: &HeaderMap) -> ContentEncoding {
    detect_encodings(headers)
        .into_iter()
        .next()
        .unwrap_or(ContentEncoding::Identity)
}

/// Turn an HTTP body into a buffered async reader, skipping non-data frames
/// (e.g. HTTP/2 trailers).
pub(crate) fn body_stream_reader(body: Incoming) -> Box<dyn AsyncBufRead + Unpin + Send> {
    let stream = BodyStream::new(body)
        .try_filter_map(|frame| std::future::ready(Ok(frame.into_data().ok())))
        .map_err(std::io::Error::other);
    Box::new(BufReader::new(StreamReader::new(stream)))
}

/// Wrap a reader with the decoders for the given encodings (outermost first).
pub(crate) fn stack_decoders(
    mut reader: Box<dyn AsyncBufRead + Unpin + Send>,
    encodings: &[ContentEncoding],
) -> Box<dyn AsyncBufRead + Unpin + Send> {
    for encoding in encodings.iter().rev() {
        reader = match encoding {
            ContentEncoding::Identity => reader,
            ContentEncoding::Br => Box::new(BufReader::new(BrotliDecoder::new(reader))),
            ContentEncoding::Gzip => Box::new(BufReader::new(GzipDecoder::new(reader))),
            ContentEncoding::Zstd => Box::new(BufReader::new(ZstdDecoder::new(reader))),
        };
    }
    reader
}

/// Decompress a response body based on the content encoding.
///
/// This function takes an aggregated response body and decompresses it according
/// to the specified encoding.
///
/// # Errors
///
/// Returns [`Error::BodyTooLarge`] when the decompressed payload exceeds
/// `MAX_DECOMPRESSED_SIZE` (256 MiB).
pub async fn decompress_body(body: Incoming, encodings: &[ContentEncoding]) -> Result<Bytes> {
    let decoder = stack_decoders(body_stream_reader(body), encodings);
    let mut out = Vec::with_capacity(32 * 1024);
    decoder
        .take(MAX_DECOMPRESSED_SIZE + 1)
        .read_to_end(&mut out)
        .await?;
    cap_check(out.len(), MAX_DECOMPRESSED_SIZE as usize)?;

    Ok(Bytes::from(out))
}

/// Reject a decompressed payload length above `limit`.
///
/// Split out from [`decompress_body`] so tests can exercise the size cap
/// cheaply without inflating a 256 MiB body.
#[doc(hidden)]
pub fn cap_check(len: usize, limit: usize) -> Result<()> {
    if len > limit {
        Err(Error::BodyTooLarge { limit })
    } else {
        Ok(())
    }
}

/// Async reader that errors instead of producing more than `limit` bytes.
///
/// Wraps decompressed streams so a decompression bomb cannot grow beyond the
/// configured cap; reading past the limit yields an I/O error rather than
/// silently truncating the stream.
struct SizeCapped<R> {
    inner: R,
    remaining: u64,
}

impl<R> SizeCapped<R> {
    fn new(inner: R, limit: u64) -> Self {
        Self {
            inner,
            remaining: limit,
        }
    }
}

impl<R: AsyncBufRead + Unpin> AsyncBufRead for SizeCapped<R> {
    fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<&[u8]>> {
        let this = self.get_mut();
        let buf = std::task::ready!(Pin::new(&mut this.inner).poll_fill_buf(cx))?;
        if buf.is_empty() {
            return Poll::Ready(Ok(buf));
        }
        let max = (buf.len() as u64).min(this.remaining) as usize;
        if max == 0 {
            return Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "decompressed stream exceeds the size limit",
            )));
        }
        Poll::Ready(Ok(&buf[..max]))
    }

    fn consume(self: Pin<&mut Self>, amt: usize) {
        let this = self.get_mut();
        this.remaining -= amt as u64;
        Pin::new(&mut this.inner).consume(amt);
    }
}

impl<R: AsyncBufRead + Unpin> AsyncRead for SizeCapped<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let data = std::task::ready!(self.as_mut().poll_fill_buf(cx))?;
        let n = data.len().min(buf.remaining());
        buf.put_slice(&data[..n]);
        self.consume(n);
        Poll::Ready(Ok(()))
    }
}

/// Bound a decompressed reader so it errors past `limit` bytes.
///
/// Split out from [`decompress_stream`] so tests can exercise the cap
/// cheaply with a small limit.
#[doc(hidden)]
pub fn cap_stream(
    reader: Box<dyn AsyncBufRead + Unpin + Send>,
    limit: u64,
) -> Box<dyn AsyncBufRead + Unpin + Send> {
    Box::new(SizeCapped::new(reader, limit))
}

/// Create a buffered reader with decompression support for streaming.
///
/// This function wraps a stream with the appropriate decompression decoder
/// based on the content encoding.
///
/// Reading past `MAX_DECOMPRESSED_SIZE` (256 MiB) yields an I/O error to
/// guard against decompression bombs.
pub fn decompress_stream(
    body: Incoming,
    encodings: &[ContentEncoding],
) -> Result<Box<dyn AsyncBufRead + Unpin + Send>> {
    let decoder = stack_decoders(body_stream_reader(body), encodings);
    Ok(cap_stream(decoder, MAX_DECOMPRESSED_SIZE))
}

/// Compress a byte payload using the specified encoding.
///
/// This function takes a byte payload and compresses it according to the
/// specified encoding algorithm.
///
/// # Arguments
///
/// * `data` - The data to compress
/// * `encoding` - The compression algorithm to use
///
/// # Returns
///
/// The compressed data as Bytes, or the original data if encoding is Identity
///
/// # Example
///
/// ```
/// use fast_dav_rs::common::compression::{compress_payload, ContentEncoding};
/// use bytes::Bytes;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let data = Bytes::from("Hello, compressed world!");
/// let compressed = compress_payload(data, ContentEncoding::Gzip).await?;
/// // compressed now contains gzipped data
/// # Ok(())
/// # }
/// ```
pub async fn compress_payload(data: Bytes, encoding: ContentEncoding) -> Result<Bytes> {
    match encoding {
        ContentEncoding::Identity => Ok(data),
        ContentEncoding::Br => {
            use async_compression::tokio::bufread::BrotliEncoder;

            let mut encoder = BrotliEncoder::new(BufReader::new(Cursor::new(data)));
            let mut compressed = Vec::new();
            encoder.read_to_end(&mut compressed).await?;
            Ok(Bytes::from(compressed))
        }
        ContentEncoding::Gzip => {
            use async_compression::tokio::bufread::GzipEncoder;

            let mut encoder = GzipEncoder::new(BufReader::new(Cursor::new(data)));
            let mut compressed = Vec::new();
            encoder.read_to_end(&mut compressed).await?;
            Ok(Bytes::from(compressed))
        }
        ContentEncoding::Zstd => {
            use async_compression::tokio::bufread::ZstdEncoder;

            let mut encoder = ZstdEncoder::new(BufReader::new(Cursor::new(data)));
            let mut compressed = Vec::new();
            encoder.read_to_end(&mut compressed).await?;
            Ok(Bytes::from(compressed))
        }
    }
}

/// Add a Content-Encoding header for outgoing requests that will be compressed.
///
/// This function adds the appropriate Content-Encoding header to indicate
/// to the server how the request body is compressed.
///
/// # Arguments
///
/// * `headers` - The header map to modify
/// * `encoding` - The compression algorithm being used
///
/// # Example
///
/// ```
/// use fast_dav_rs::common::compression::{add_content_encoding, ContentEncoding};
/// use hyper::HeaderMap;
///
/// let mut headers = HeaderMap::new();
/// add_content_encoding(&mut headers, ContentEncoding::Gzip);
/// assert_eq!(headers.get("Content-Encoding").unwrap(), "gzip");
/// ```
pub fn add_content_encoding(headers: &mut HeaderMap, encoding: ContentEncoding) {
    if encoding != ContentEncoding::Identity {
        if let Ok(value) = http::HeaderValue::from_str(encoding.as_str()) {
            headers.insert("Content-Encoding", value);
        }
    }
}
