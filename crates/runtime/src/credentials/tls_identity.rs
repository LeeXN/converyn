//! Attach the verified TLS peer leaf fingerprint to each request. HTTP headers
//! cannot supply or override this connection-owned identity.
use axum_server::{accept::Accept, tls_rustls::RustlsAcceptor};
use sha2::{Digest, Sha256};
use std::{
    future::Future,
    io,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::net::TcpStream;
use tokio_rustls::server::TlsStream;
#[derive(Clone)]
pub struct PeerCertificate(pub String, pub u64);
#[derive(Clone)]
pub struct Acceptor(pub RustlsAcceptor);
#[derive(Clone)]
pub struct BoundService<S> {
    inner: S,
    peer: PeerCertificate,
}
impl<S: Send + 'static> Accept<TcpStream, S> for Acceptor {
    type Stream = TlsStream<TcpStream>;
    type Service = BoundService<S>;
    type Future = Pin<Box<dyn Future<Output = io::Result<(Self::Stream, Self::Service)>> + Send>>;
    fn accept(&self, stream: TcpStream, service: S) -> Self::Future {
        let future = self.0.accept(stream, service);
        Box::pin(async move {
            let (stream, service) = future.await?;
            let leaf = stream
                .get_ref()
                .1
                .peer_certificates()
                .and_then(|c| c.first())
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "client certificate missing",
                    )
                })?;
            let info = crate::credentials::pki::info_der(leaf.as_ref()).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "invalid client certificate metadata",
                )
            })?;
            let peer = PeerCertificate(hex::encode(Sha256::digest(leaf.as_ref())), info.expires_ms);
            Ok((
                stream,
                BoundService {
                    inner: service,
                    peer,
                },
            ))
        })
    }
}
impl<S, B> tower::Service<axum::http::Request<B>> for BoundService<S>
where
    S: tower::Service<axum::http::Request<B>>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }
    fn call(&mut self, mut request: axum::http::Request<B>) -> Self::Future {
        request.extensions_mut().insert(self.peer.clone());
        self.inner.call(request)
    }
}
