use fast_dav_rs::compression::*;
use bytes::Bytes;

#[tokio::test]
async fn test_compress_payload_identity() {
    let data = Bytes::from("Hello, world!");
    let compressed = compress_payload(data.clone(), ContentEncoding::Identity)
        .await
        .expect("Identity compression should succeed");
    assert_eq!(compressed, data);
}

#[tokio::test]
async fn test_compress_payload_gzip() {
    let data = Bytes::from("Hello, world! This is test data for compression. It needs to be long enough to actually compress effectively with gzip. Let's add more text to make sure compression works properly.");
    let compressed = compress_payload(data.clone(), ContentEncoding::Gzip)
        .await
        .expect("Gzip compression should succeed");
    assert_ne!(compressed, data); // Should be different
    // For longer text, compressed should be smaller
    assert!(compressed.len() < data.len(), "Compressed size: {}, Original size: {}", compressed.len(), data.len());
}

#[tokio::test]
async fn test_compress_payload_br() {
    let data = Bytes::from("Hello, world! This is test data for compression with Brotli.");
    let compressed = compress_payload(data.clone(), ContentEncoding::Br)
        .await
        .expect("Brotli compression should succeed");
    assert_ne!(compressed, data); // Should be different
    assert!(compressed.len() < data.len()); // Should be smaller
}

#[tokio::test]
async fn test_compress_payload_zstd() {
    let data = Bytes::from("Hello, world! This is test data for compression with Zstd. It needs to be long enough to actually compress effectively with zstd. Let's add more text to make sure compression works properly.");
    let compressed = compress_payload(data.clone(), ContentEncoding::Zstd)
        .await
        .expect("Zstd compression should succeed");
    assert_ne!(compressed, data); // Should be different
    // For longer text, compressed should be smaller
    assert!(compressed.len() < data.len(), "Compressed size: {}, Original size: {}", compressed.len(), data.len());
}

#[tokio::test]
async fn test_compress_large_data() {
    // Test with larger data
    let large_data = Bytes::from(vec![b'A'; 10000]); // 10KB of 'A'
    
    // Test brotli compression
    let compressed = compress_payload(large_data.clone(), ContentEncoding::Br)
        .await
        .expect("Brotli compression should succeed");
    assert!(compressed.len() < large_data.len()); // Should compress significantly
    
    // Test gzip compression
    let compressed_gzip = compress_payload(large_data.clone(), ContentEncoding::Gzip)
        .await
        .expect("Gzip compression should succeed");
    assert!(compressed_gzip.len() < large_data.len()); // Should compress significantly
    
    // Test zstd compression
    let compressed_zstd = compress_payload(large_data.clone(), ContentEncoding::Zstd)
        .await
        .expect("Zstd compression should succeed");
    assert!(compressed_zstd.len() < large_data.len()); // Should compress significantly
}

#[tokio::test]
async fn test_compress_empty_data() {
    let empty_data = Bytes::from("");
    
    // Test identity (should remain empty)
    let compressed_identity = compress_payload(empty_data.clone(), ContentEncoding::Identity)
        .await
        .expect("Identity compression should succeed");
    assert_eq!(compressed_identity, empty_data);
    
    // Test other compression types with empty data
    // Note: Even empty data will have compression headers, so won't be completely empty
    for encoding in &[ContentEncoding::Gzip, ContentEncoding::Br, ContentEncoding::Zstd] {
        let compressed = compress_payload(empty_data.clone(), *encoding)
            .await
            .expect(&format!("Compression with {:?} should succeed", encoding));
        // Just verify it doesn't panic and returns some data
        println!("Empty data compressed with {:?} resulted in {} bytes", encoding, compressed.len());
    }
}