mod hub;
mod protocol;
mod server;
mod usb;

use std::time::Duration;

use hub::Hub;
use tokio::net::TcpListener;

/// No idle shutdown unless `MASTERHUBD_IDLE_SECS` is a positive number of seconds.
fn idle_timeout() -> Option<Duration> {
    let Ok(value) = std::env::var("MASTERHUBD_IDLE_SECS") else {
        return None;
    };
    if value.parse::<u64>().is_err() {
        eprintln!(
            "masterhubd: ignoring MASTERHUBD_IDLE_SECS={value:?}; expected a positive number of seconds"
        );
    }
    parse_idle(Some(&value))
}

fn parse_idle(value: Option<&str>) -> Option<Duration> {
    let secs = value?.parse::<u64>().ok()?;
    (secs > 0).then_some(Duration::from_secs(secs))
}

fn listen_addr() -> String {
    std::env::var("MASTERHUBD_LISTEN").unwrap_or_else(|_| "127.0.0.1:9123".to_string())
}

enum Bind {
    Ready(TcpListener),
    AlreadyRunning,
}

/// `std` bind does not set `SO_REUSEADDR`, so a second process gets
/// `AddrInUse` while the first is still listening. That includes Windows,
/// where a Tokio bind would set `SO_REUSEADDR` and can share the port.
fn bind_exclusive(addr: &str) -> std::io::Result<Bind> {
    match std::net::TcpListener::bind(addr) {
        Ok(listener) => {
            listener.set_nonblocking(true)?;
            Ok(Bind::Ready(TcpListener::from_std(listener)?))
        }
        Err(err) if err.kind() == std::io::ErrorKind::AddrInUse => Ok(Bind::AlreadyRunning),
        Err(err) => Err(err),
    }
}

pub async fn run() -> std::io::Result<()> {
    let addr = listen_addr();
    let listener = match bind_exclusive(&addr)? {
        Bind::Ready(listener) => listener,
        Bind::AlreadyRunning => {
            eprintln!("masterhubd: {addr} is already in use");
            return Ok(());
        }
    };
    let local = listener.local_addr()?;
    eprintln!(
        "masterhubd {} listening on ws://{local}",
        env!("CARGO_PKG_VERSION")
    );
    let idle = idle_timeout();
    if let Some(idle) = idle {
        eprintln!("masterhubd: idle timeout {}s", idle.as_secs());
    }
    serve_bound(listener, idle).await
}

/// Serve an already-bound listener. Tests use this with `127.0.0.1:0`.
/// This path does not apply the idle timeout.
pub async fn serve_listener(listener: TcpListener) -> std::io::Result<()> {
    serve_bound(listener, None).await
}

async fn serve_bound(listener: TcpListener, idle: Option<Duration>) -> std::io::Result<()> {
    let (sink, start) = usb::prepare();
    let hub = std::sync::Arc::new(Hub::new(sink));
    start(std::sync::Arc::clone(&hub));
    match idle {
        Some(idle) => server::serve_idle(listener, hub, Some(idle)).await,
        None => server::serve(listener, hub).await,
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    #[test]
    fn idle_is_off_unless_the_value_is_a_positive_number_of_seconds() {
        assert_eq!(super::parse_idle(None), None);
        assert_eq!(super::parse_idle(Some("0")), None);
        assert_eq!(super::parse_idle(Some("")), None);
        assert_eq!(super::parse_idle(Some("nope")), None);
        assert_eq!(super::parse_idle(Some("45")), Some(Duration::from_secs(45)));
    }

    #[test]
    fn a_second_bind_reports_the_port_is_taken() {
        let first = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = first.local_addr().unwrap().to_string();
        let second = super::bind_exclusive(&addr).unwrap();
        assert!(matches!(second, super::Bind::AlreadyRunning));
    }
}
