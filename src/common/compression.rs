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

/// Detect the response `Content-Encoding` header and map it to [`ContentEncoding`].
///
/// Returns [`ContentEncoding::Identity`] if the header is missing or not recognized.
///
/// # Example
/// ```
/// use fast_dav_rs::compression::{detect_encoding, ContentEncoding};
/// use hyper::{header, HeaderMap};
///
/// let mut headers = HeaderMap::new();
/// headers.insert(header::CONTENT_ENCODING, "gzip".parse().unwrap());
/// assert_eq!(detect_encoding(&headers), ContentEncoding::Gzip);
/// ```
pub fn detect_encoding(headers: &HeaderMap) -> ContentEncoding {
    if let Some(val) = headers.get(header::CONTENT_ENCODING)
        && let Ok(s) = val.to_str()
    {
        let enc = s
            .split(',')
            .next()
            .map(|t| t.trim().to_ascii_lowercase())
            .unwrap_or_default();
        return match enc.as_str() {
            "br" => ContentEncoding::Br,
            "gzip" => ContentEncoding::Gzip,
            "zstd" | "zst" => ContentEncoding::Zstd,
            _ => ContentEncoding::Identity,
        };
    }
    ContentEncoding::Identity
}

/// Insert an `Accept-Encoding` header (`br, zstd, gzip`) if not already present.
///
/// This hints to the server that the client supports compressed responses.
pub fn add_accept_encoding(h: &mut HeaderMap) {
    if !h.contains_key(http::header::ACCEPT_ENCODING) {
        h.insert(
            http::header::ACCEPT_ENCODING,
            http::HeaderValue::from_static("br, zstd, gzip"),
        );
    }
}

/// Decompress a response body based on the content encoding.
///
/// This function takes an aggregated response body and decompresses it according
/// to the specified encoding.
pub async fn decompress_body(body: Incoming, encoding: ContentEncoding) -> Result<Bytes> {
    let stream = BodyStream::new(body)
        .map_ok(|frame| frame.into_data().unwrap_or_default())
        .map_err(std::io::Error::other);
    let reader = StreamReader::new(stream);
    let reader = BufReader::new(reader);
    let mut out = Vec::with_capacity(32 * 1024);

    match encoding {
        ContentEncoding::Identity => {
            let mut r = reader;
            r.read_to_end(&mut out).await?;
        }
        ContentEncoding::Br => {
            let mut dec = BrotliDecoder::new(reader);
            dec.read_to_end(&mut out).await?;
        }
        ContentEncoding::Gzip => {
            let mut dec = GzipDecoder::new(reader);
            dec.read_to_end(&mut out).await?;
        }
        ContentEncoding::Zstd => {
            let mut dec = ZstdDecoder::new(reader);
            dec.read_to_end(&mut out).await?;
        }
    }

    Ok(Bytes::from(out))
}

/// Create a buffered reader with decompression support for streaming.
///
/// This function wraps a stream with the appropriate decompression decoder
/// based on the content encoding.
pub fn decompress_stream(
    body: Incoming,
    encoding: ContentEncoding,
) -> Result<Box<dyn AsyncBufRead + Unpin + Send>> {
    let stream = BodyStream::new(body)
        .map_ok(|frame| frame.into_data().unwrap_or_default())
        .map_err(std::io::Error::other);
    let reader: Box<dyn AsyncBufRead + Unpin + Send> =
        Box::new(BufReader::new(StreamReader::new(stream)));

    let decompressed_reader: Box<dyn AsyncBufRead + Unpin + Send> = match encoding {
        ContentEncoding::Identity => reader,
        ContentEncoding::Br => Box::new(BufReader::new(BrotliDecoder::new(reader))),
        ContentEncoding::Gzip => Box::new(BufReader::new(GzipDecoder::new(reader))),
        ContentEncoding::Zstd => Box::new(BufReader::new(ZstdDecoder::new(reader))),
    };

    Ok(decompressed_reader)
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

/// Add Content-Encoding header for outgoing requests that will be compressed.
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
