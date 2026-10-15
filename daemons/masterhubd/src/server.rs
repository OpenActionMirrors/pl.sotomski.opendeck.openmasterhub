use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc, Notify};
use tokio_tungstenite::accept_async_with_config;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::Message;

use crate::hub::Hub;
use crate::protocol;

const MAX_FRAME_BYTES: usize = 128 * 1024;

pub async fn serve(listener: TcpListener, hub: Arc<Hub>) -> std::io::Result<()> {
    serve_idle(listener, hub, None).await
}

pub(crate) async fn serve_idle(
    listener: TcpListener,
    hub: Arc<Hub>,
    idle: Option<Duration>,
) -> std::io::Result<()> {
    let clients = Arc::new(ClientCount::new());
    if let Some(idle) = idle {
        let mut idle_wait = std::pin::pin!(wait_for_idle(Arc::clone(&clients), idle));
        loop {
            tokio::select! {
                biased;
                _ = &mut idle_wait => {
                    eprintln!("masterhubd: no clients for {}, stopping", format_idle(idle));
                    return Ok(());
                }
                accepted = listener.accept() => {
                    let (stream, _) = accepted?;
                    spawn_client(stream, Arc::clone(&hub), Arc::clone(&clients));
                }
            }
        }
    } else {
        loop {
            let (stream, _) = listener.accept().await?;
            spawn_client(stream, Arc::clone(&hub), Arc::clone(&clients));
        }
    }
}

fn spawn_client(stream: TcpStream, hub: Arc<Hub>, clients: Arc<ClientCount>) {
    tokio::spawn(async move {
        handle_connection(stream, hub, clients).await;
    });
}

fn format_idle(idle: Duration) -> String {
    let ms = idle.as_millis();
    if ms % 1000 == 0 {
        format!("{}s", ms / 1000)
    } else {
        format!("{ms}ms")
    }
}

struct ClientCount {
    n: AtomicUsize,
    changed: Notify,
}

impl ClientCount {
    fn new() -> Self {
        Self {
            n: AtomicUsize::new(0),
            changed: Notify::new(),
        }
    }

    fn get(&self) -> usize {
        self.n.load(Ordering::Acquire)
    }

    fn hold(self: &Arc<Self>) -> ClientGuard {
        self.n.fetch_add(1, Ordering::AcqRel);
        self.changed.notify_one();
        ClientGuard {
            count: Arc::clone(self),
        }
    }
}

struct ClientGuard {
    count: Arc<ClientCount>,
}

impl Drop for ClientGuard {
    fn drop(&mut self) {
        self.count.n.fetch_sub(1, Ordering::AcqRel);
        self.count.changed.notify_one();
    }
}

/// Completes after `idle` with a client count of zero. A connection starts the
/// wait over when it ends.
async fn wait_for_idle(clients: Arc<ClientCount>, idle: Duration) {
    loop {
        let notified = clients.changed.notified();
        tokio::pin!(notified);
        if clients.get() == 0 {
            tokio::select! {
                biased;
                _ = &mut notified => {}
                _ = tokio::time::sleep(idle) => {
                    if clients.get() == 0 {
                        return;
                    }
                }
            }
        } else {
            notified.await;
        }
    }
}

async fn handle_connection(stream: TcpStream, hub: Arc<Hub>, clients: Arc<ClientCount>) {
    let config = WebSocketConfig {
        max_message_size: Some(MAX_FRAME_BYTES),
        max_frame_size: Some(MAX_FRAME_BYTES),
        ..WebSocketConfig::default()
    };
    let Ok(ws) = accept_async_with_config(stream, Some(config)).await else {
        return;
    };
    let _client = clients.hold();
    let (mut write, mut read) = ws.split();
    let mut events = hub.subscribe();
    let (reply_tx, mut reply_rx) = mpsc::unbounded_channel();

    if write.send(Message::Text(hub.hello_json())).await.is_err() {
        return;
    }

    loop {
        tokio::select! {
            biased;
            reply = reply_rx.recv() => {
                let Some(reply) = reply else { break };
                if write.send(Message::Text(reply)).await.is_err() {
                    break;
                }
            }
            event = events.recv() => {
                match event {
                    Ok(event) => {
                        if write.send(Message::Text(event)).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            incoming = read.next() => {
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        let reply = hub.handle_client_text(&text);
                        if reply_tx.send(reply).is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Binary(_))) => {
                        let reply = protocol::error_json(None, "binary frames are not supported");
                        if reply_tx.send(reply).is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        if write.send(Message::Pong(payload)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Pong(_))) => {}
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Frame(_))) => {}
                    Some(Err(_)) => break,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use futures_util::{SinkExt, StreamExt};
    use tokio::net::TcpListener;
    use tokio_tungstenite::connect_async;
    use tokio_tungstenite::tungstenite::Message;

    use super::*;
    use crate::hub::{Hub, NoopCommandSink};

    async fn next_text<S>(socket: &mut S) -> String
    where
        S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
    {
        loop {
            match socket
                .next()
                .await
                .expect("socket closed")
                .expect("socket error")
            {
                Message::Text(text) => return text,
                Message::Ping(_) | Message::Pong(_) => {}
                other => panic!("unexpected frame {other:?}"),
            }
        }
    }

    #[tokio::test]
    async fn trigger_reaches_another_client_and_bad_input_does_not() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let hub = Arc::new(Hub::new(Arc::new(NoopCommandSink)));
        tokio::spawn(async move {
            serve(listener, hub).await.unwrap();
        });

        let url = format!("ws://{addr}");
        let (mut client_a, _) = connect_async(&url).await.unwrap();
        let (mut client_b, _) = connect_async(&url).await.unwrap();

        let hello_a: serde_json::Value =
            serde_json::from_str(&next_text(&mut client_a).await).unwrap();
        let hello_b: serde_json::Value =
            serde_json::from_str(&next_text(&mut client_b).await).unwrap();
        for hello in [hello_a, hello_b] {
            assert_eq!(hello["v"], 1);
            assert_eq!(hello["type"], "hello");
            assert_eq!(hello["daemon"], "masterhubd");
            assert_eq!(hello["usb"], false);
            assert_eq!(hello["modules"], serde_json::json!([]));
        }

        client_a
            .send(Message::Text(
                r#"{"v":1,"type":"trigger","id":"t0","action":"knob_rotated","payload":{"module":{"model":"M003K","id":"knobs"},"control":0,"direction":"cw"}}"#
                    .to_string(),
            ))
            .await
            .unwrap();

        let seen_by_b: serde_json::Value =
            serde_json::from_str(&next_text(&mut client_b).await).unwrap();
        assert_eq!(seen_by_b["type"], "event");
        assert_eq!(seen_by_b["action"], "knob_rotated");
        assert_eq!(seen_by_b["payload"]["control"], 0);
        assert!(seen_by_b.get("id").is_none());

        let result_a: serde_json::Value =
            serde_json::from_str(&next_text(&mut client_a).await).unwrap();
        assert_eq!(result_a["type"], "result");
        assert_eq!(result_a["id"], "t0");
        assert_eq!(result_a["ok"], true);
        let echo_a: serde_json::Value =
            serde_json::from_str(&next_text(&mut client_a).await).unwrap();
        assert_eq!(echo_a["type"], "event");
        assert_eq!(echo_a["payload"]["control"], 0);

        client_a
            .send(Message::Text(
                r#"{"v":1,"type":"command","id":"light","action":"set_led_mode","payload":{"module":{"model":"M003K","id":"knobs"},"control":"all","mode":"static"}}"#
                    .to_string(),
            ))
            .await
            .unwrap();
        let command_result: serde_json::Value =
            serde_json::from_str(&next_text(&mut client_a).await).unwrap();
        assert_eq!(command_result["type"], "result");
        assert_eq!(command_result["id"], "light");
        assert_eq!(command_result["ok"], true);

        client_a
            .send(Message::Text(
                r#"{"v":1,"type":"command","id":"bad","action":"set_led_color","payload":{"module":{"model":"M003K","id":"knobs"},"control":9,"r":1,"g":2,"b":3}}"#
                    .to_string(),
            ))
            .await
            .unwrap();
        let bad: serde_json::Value = serde_json::from_str(&next_text(&mut client_a).await).unwrap();
        assert_eq!(bad["type"], "error");
        assert_eq!(bad["id"], "bad");
        assert!(bad["message"].as_str().unwrap().contains("out of range"));

        client_a.send(Message::Binary(vec![1, 2, 3])).await.unwrap();
        let binary: serde_json::Value =
            serde_json::from_str(&next_text(&mut client_a).await).unwrap();
        assert_eq!(binary["type"], "error");
        assert_eq!(binary["message"], "binary frames are not supported");
        assert!(binary.get("id").is_none());

        client_a
            .send(Message::Text(
                r#"{"v":1,"type":"trigger","action":"slider_moved","payload":{"module":{"model":"M005S","id":"faders"},"control":1,"value":40}}"#
                    .to_string(),
            ))
            .await
            .unwrap();

        let next_b: serde_json::Value =
            serde_json::from_str(&next_text(&mut client_b).await).unwrap();
        assert_eq!(next_b["type"], "event");
        assert_eq!(next_b["action"], "slider_moved");
        assert_eq!(next_b["payload"]["control"], 1);
        assert_eq!(next_b["payload"]["value"], 40);
    }

    #[tokio::test]
    async fn idle_timeout_stops_the_listener_when_no_client_is_connected() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let hub = Arc::new(Hub::new(Arc::new(NoopCommandSink)));
        let serve = tokio::spawn(serve_idle(listener, hub, Some(Duration::from_millis(400))));

        tokio::time::sleep(Duration::from_secs(2)).await;
        assert!(
            tokio::net::TcpStream::connect(addr).await.is_err(),
            "listener stayed open with no clients"
        );
        tokio::time::timeout(Duration::from_secs(2), serve)
            .await
            .expect("server did not stop")
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn a_connected_client_holds_off_the_idle_timeout() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let hub = Arc::new(Hub::new(Arc::new(NoopCommandSink)));
        let serve = tokio::spawn(serve_idle(listener, hub, Some(Duration::from_millis(400))));

        let (socket, _) = connect_async(format!("ws://{addr}")).await.unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert!(
            tokio::net::TcpStream::connect(addr).await.is_ok(),
            "listener closed while a client was connected"
        );
        drop(socket);
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert!(
            tokio::net::TcpStream::connect(addr).await.is_err(),
            "listener stayed open after the last client left"
        );
        tokio::time::timeout(Duration::from_secs(2), serve)
            .await
            .expect("server did not stop")
            .unwrap()
            .unwrap();
    }
}
