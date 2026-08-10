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
/// The `MaybeProxied` connector is `pub(crate)` since it is an implementation
/// detail of the connector layer; external code consumes it through this
/// alias.
#[allow(private_interfaces)]
pub type HyperClient = Client<hyper_rustls::HttpsConnector<MaybeProxied>, Full<Bytes>>;

/// Default maximum number of idle pooled connections kept alive per host.
///
/// Lowered from the previous hardcoded `128`: typical CalDAV/CardDAV
/// deployments cap per-client connections well below that (often 10–50),
/// and keeping 128 idle sockets around needlessly exhausts client file
/// descriptors and server connection limits.
pub const DEFAULT_POOL_MAX_IDLE_PER_HOST: usize = 32;

/// Connector that is either direct or proxied via HTTP CONNECT tunnel.
///
/// Implements `tower_service::Service<Uri>` by delegating to the inner
/// connector. The future is boxed since `HttpConnector` and
/// `Tunnel<HttpConnector>` produce different future types.
#[derive(Clone)]
pub(crate) enum MaybeProxied {
    Direct(HttpConnector),
    Tunneled(Tunnel<HttpConnector>),
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
    fn default_pool_max_idle_is_32() {
        assert_eq!(DEFAULT_POOL_MAX_IDLE_PER_HOST, 32);
    }

    #[test]
    fn maybe_proxied_direct_construction() {
        let _ = MaybeProxied::Direct(HttpConnector::new());
    }
}
