use bytes::Bytes;
use http_body_util::Full;
use hyper::Uri;
use hyper_util::client::legacy::connect::proxy::Tunnel;
use hyper_util::client::legacy::{Client, connect::HttpConnector};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tower_service::Service;

/// Type alias for the Hyper client used across CalDAV/CardDAV modules.
///
/// Public so callers of
/// [`WebDavClientBuilder::with_hyper_client`](crate::webdav::WebDavClientBuilder::with_hyper_client)
/// can build and inject a client of the exact expected shape. The connector
/// type [`MaybeProxied`] is an implementation detail kept nameable for this
/// alias.
pub type HyperClient = Client<hyper_rustls::HttpsConnector<MaybeProxied>, Full<Bytes>>;

/// Connector that is either direct or proxied via HTTP CONNECT tunnel.
///
/// Implementation detail, public so the [`HyperClient`] alias is nameable.
/// Use [`MaybeProxied::direct`] for the direct form; the tunneled form
/// (`MaybeProxied::Tunneled`) is constructed internally from proxy settings.
///
/// Implements `tower_service::Service<Uri>` by delegating to the inner
/// connector. The future is boxed since `HttpConnector` and
/// `Tunnel<HttpConnector>` produce different future types.
#[derive(Clone)]
#[non_exhaustive]
pub enum MaybeProxied {
    Direct(HttpConnector),
    Tunneled(Tunnel<HttpConnector>),
}

impl MaybeProxied {
    /// Create a direct connector (no proxy) wrapping an `HttpConnector`.
    ///
    /// Configure the connector (e.g. `enforce_http(false)` to allow `https://`
    /// URIs) before wrapping it.
    pub fn direct(http: HttpConnector) -> Self {
        Self::Direct(http)
    }
}

impl Service<Uri> for MaybeProxied {
    type Response = <HttpConnector as Service<Uri>>::Response;
    type Error = Box<dyn std::error::Error + Send + Sync + 'static>;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match self {
            MaybeProxied::Direct(c) => c.poll_ready(cx).map_err(|e| Box::new(e) as Self::Error),
            MaybeProxied::Tunneled(c) => c.poll_ready(cx).map_err(|e| Box::new(e) as Self::Error),
        }
    }

    fn call(&mut self, dst: Uri) -> Self::Future {
        match self {
            MaybeProxied::Direct(c) => {
                let fut = c.call(dst);
                Box::pin(async move { fut.await.map_err(|e| Box::new(e) as Self::Error) })
            }
            MaybeProxied::Tunneled(c) => {
                let fut = c.call(dst);
                Box::pin(async move { fut.await.map_err(|e| Box::new(e) as Self::Error) })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maybe_proxied_direct_construction() {
        let _ = MaybeProxied::Direct(HttpConnector::new());
    }
}
