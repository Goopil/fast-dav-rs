pub mod compression;
pub mod http;

pub use compression::{
    ContentEncoding, add_accept_encoding, add_content_encoding, compress_payload, decompress_body,
    decompress_stream, detect_encoding, detect_encodings,
};
pub use http::{HyperClient, build_hyper_client};
