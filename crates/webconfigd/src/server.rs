//! TLS listener and HTTP connection serving.

use std::path::Path;

use axum::Router;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder as ConnBuilder;
use hyper_util::service::TowerToHyperService;
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use tower::ServiceExt as _;
use webconfig_engine::manifest::BoardManifest;

use crate::tls;

/// Ensure the device certificate exists, then serve the application over TLS.
pub(crate) async fn serve(
    app: Router,
    webconfig_dir: &Path,
    manifest: &BoardManifest,
) -> anyhow::Result<()> {
    let cert = webconfig_dir.join("cert.pem");
    let key = webconfig_dir.join("key.pem");
    let hostname = std::env::var("WEBCONFIG_HOSTNAME").ok();
    tls::ensure_self_signed(&cert, &key, hostname.as_deref())
        .map_err(|error| anyhow::anyhow!("tls init: {error}"))?;
    let acceptor =
        tls::acceptor(&cert, &key).map_err(|error| anyhow::anyhow!("tls acceptor: {error}"))?;
    let acceptor = TlsAcceptor::from(acceptor);

    let port: u16 = std::env::var("WEBCONFIG_PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(8443);
    let addr = format!("0.0.0.0:{port}");
    tracing::info!(
        "webconfigd listening on https://{addr} (platform={})",
        manifest.platform
    );

    let listener = TcpListener::bind(&addr).await?;
    loop {
        let (stream, _peer) = listener.accept().await?;
        let acceptor = acceptor.clone();
        let app = app.clone();
        tokio::spawn(async move {
            let Ok(tls) = acceptor.accept(stream).await else {
                return;
            };
            serve_connection(app, tls).await;
        });
    }
}

async fn serve_connection(
    app: Router,
    tls: tokio_rustls::server::TlsStream<tokio::net::TcpStream>,
) {
    let io = TokioIo::new(tls);
    let tower_service =
        app.map_request(|req: http::Request<hyper::body::Incoming>| req.map(axum::body::Body::new));
    let hyper_service = TowerToHyperService::new(tower_service);
    let _ = ConnBuilder::new(TokioExecutor::new())
        .serve_connection_with_upgrades(io, hyper_service)
        .await;
}
