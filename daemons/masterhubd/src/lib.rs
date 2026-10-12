mod hub;
mod protocol;
mod server;
mod usb;

use std::sync::Arc;

use hub::Hub;
use tokio::net::TcpListener;

pub async fn run() -> std::io::Result<()> {
    let addr = std::env::var("MASTERHUBD_LISTEN").unwrap_or_else(|_| "127.0.0.1:9123".to_string());
    let listener = TcpListener::bind(&addr).await?;
    let local = listener.local_addr()?;
    eprintln!(
        "masterhubd {} listening on ws://{local}",
        env!("CARGO_PKG_VERSION")
    );
    serve_listener(listener).await
}

/// Serve an already-bound listener. Tests use this with `127.0.0.1:0`.
pub async fn serve_listener(listener: TcpListener) -> std::io::Result<()> {
    let (sink, start) = usb::prepare();
    let hub = Arc::new(Hub::new(sink));
    start(Arc::clone(&hub));
    server::serve(listener, hub).await
}
