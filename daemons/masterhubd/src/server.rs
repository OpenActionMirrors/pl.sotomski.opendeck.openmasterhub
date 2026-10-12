use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc};
use tokio_tungstenite::accept_async_with_config;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::Message;

use crate::hub::Hub;
use crate::protocol;

const MAX_FRAME_BYTES: usize = 128 * 1024;

pub async fn serve(listener: TcpListener, hub: Arc<Hub>) -> std::io::Result<()> {
    loop {
        let (stream, _) = listener.accept().await?;
        let hub = Arc::clone(&hub);
        tokio::spawn(async move {
            handle_connection(stream, hub).await;
        });
    }
}

async fn handle_connection(stream: TcpStream, hub: Arc<Hub>) {
    let config = WebSocketConfig {
        max_message_size: Some(MAX_FRAME_BYTES),
        max_frame_size: Some(MAX_FRAME_BYTES),
        ..WebSocketConfig::default()
    };
    let Ok(ws) = accept_async_with_config(stream, Some(config)).await else {
        return;
    };
    let (mut write, mut read) = ws.split();
    let mut events = hub.subscribe();
    let (reply_tx, mut reply_rx) = mpsc::unbounded_channel();

    if write
        .send(Message::Text(protocol::hello_json()))
        .await
        .is_err()
    {
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
}
