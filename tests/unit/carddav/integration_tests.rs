use fast_dav_rs::CardDavClient;
use fast_dav_rs::carddav::Depth;

#[test]
fn test_client_creation() {
    let client = CardDavClient::new("https://example.com/dav/", Some("user"), Some("pass"));
    assert!(client.is_ok());
}

#[test]
fn test_client_without_auth() {
    let client = CardDavClient::new("https://example.com/dav/", None, None);
    assert!(client.is_ok());
}

#[test]
fn test_build_uri_relative() {
    let client = CardDavClient::new("https://example.com/dav/user/", None, None)
        .expect("Failed to create client");

    let uri = client
        .build_uri("addressbook/")
        .expect("Failed to build URI");
    assert_eq!(uri.to_string(), "https://example.com/dav/user/addressbook/");
}

#[test]
fn test_depth_values() {
    assert_eq!(Depth::Zero.as_str(), "0");
    assert_eq!(Depth::One.as_str(), "1");
    assert_eq!(Depth::Infinity.as_str(), "infinity");
}
