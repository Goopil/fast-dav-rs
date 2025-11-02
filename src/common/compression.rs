//! Compression utilities for HTTP content encoding.
//!
//! This module provides support for automatic compression and decompression
//! of HTTP responses using various encoding formats.

use anyhow::Result;
use async_compression::tokio::bufread::{BrotliDecoder, GzipDecoder, ZstdDecoder};
use bytes::Bytes;
use futures_util::TryStreamExt;
use http_body_util::BodyStream;
use hyper::body::Incoming;
use hyper::{HeaderMap, header, http};
use std::io::Cursor;
use tokio::io::{AsyncBufRead, AsyncReadExt, BufReader};
use tokio_util::io::StreamReader;

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
            if let Some((key, value)) = param.split_once('=')
                && key.trim().eq_ignore_ascii_case("q")
                && let Ok(parsed) = value.trim().parse::<f32>()
            {
                weight = parsed.clamp(0.0, 1.0);
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

    if !identity_explicit && let Some(q) = wildcard_q {
        identity_q = q;
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

/// Decompress a response body based on the content encoding.
///
/// This function takes an aggregated response body and decompresses it according
/// to the specified encoding.
pub async fn decompress_body(body: Incoming, encodings: &[ContentEncoding]) -> Result<Bytes> {
    let stream = BodyStream::new(body)
        .map_ok(|frame| frame.into_data().unwrap_or_default())
        .map_err(std::io::Error::other);
    let reader = StreamReader::new(stream);
    let reader = BufReader::new(reader);
    let mut out = Vec::with_capacity(32 * 1024);
    let mut current: Box<dyn AsyncBufRead + Unpin + Send> = Box::new(reader);

    for encoding in encodings.iter().rev() {
        current = match encoding {
            ContentEncoding::Identity => current,
            ContentEncoding::Br => Box::new(BufReader::new(BrotliDecoder::new(current))),
            ContentEncoding::Gzip => Box::new(BufReader::new(GzipDecoder::new(current))),
            ContentEncoding::Zstd => Box::new(BufReader::new(ZstdDecoder::new(current))),
        };
    }

    let mut decoder = current;
    decoder.read_to_end(&mut out).await?;

    Ok(Bytes::from(out))
}

/// Create a buffered reader with decompression support for streaming.
///
/// This function wraps a stream with the appropriate decompression decoder
/// based on the content encoding.
pub fn decompress_stream(
    body: Incoming,
    encodings: &[ContentEncoding],
) -> Result<Box<dyn AsyncBufRead + Unpin + Send>> {
    let stream = BodyStream::new(body)
        .map_ok(|frame| frame.into_data().unwrap_or_default())
        .map_err(std::io::Error::other);
    let reader: Box<dyn AsyncBufRead + Unpin + Send> =
        Box::new(BufReader::new(StreamReader::new(stream)));

    let mut current = reader;
    for encoding in encodings.iter().rev() {
        current = match encoding {
            ContentEncoding::Identity => current,
            ContentEncoding::Br => Box::new(BufReader::new(BrotliDecoder::new(current))),
            ContentEncoding::Gzip => Box::new(BufReader::new(GzipDecoder::new(current))),
            ContentEncoding::Zstd => Box::new(BufReader::new(ZstdDecoder::new(current))),
        };
    }

    Ok(current)
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
/// use fast_dav_rs::compression::{compress_payload, ContentEncoding};
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
/// use fast_dav_rs::compression::{add_content_encoding, ContentEncoding};
/// use hyper::HeaderMap;
///
/// let mut headers = HeaderMap::new();
/// add_content_encoding(&mut headers, ContentEncoding::Gzip);
/// assert_eq!(headers.get("Content-Encoding").unwrap(), "gzip");
/// ```
pub fn add_content_encoding(headers: &mut HeaderMap, encoding: ContentEncoding) {
    if encoding != ContentEncoding::Identity
        && let Ok(value) = http::HeaderValue::from_str(encoding.as_str())
    {
        headers.insert("Content-Encoding", value);
    }
}
