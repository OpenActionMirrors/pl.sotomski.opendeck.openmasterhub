use std::io::Cursor;
use std::net::SocketAddr;
use std::time::Duration;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::net::TcpListener;
use tokio::process::{Child, Command};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{accept_async, connect_async};

struct DeckHost {
    port: u16,
    to_plugin: mpsc::UnboundedSender<String>,
    from_plugin: mpsc::UnboundedReceiver<Value>,
}

async fn deck_host() -> DeckHost {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (to_plugin, mut plugin_inbox) = mpsc::unbounded_channel::<String>();
    let (from_plugin, host_inbox) = mpsc::unbounded_channel::<Value>();
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let socket = accept_async(stream).await.unwrap();
        let (mut write, mut read) = socket.split();
        let writer = tokio::spawn(async move {
            while let Some(text) = plugin_inbox.recv().await {
                if write.send(Message::Text(text.into())).await.is_err() {
                    break;
                }
            }
        });
        while let Some(Ok(message)) = read.next().await {
            if let Message::Text(text) = message {
                let value: Value = serde_json::from_str(&text).unwrap();
                if from_plugin.send(value).is_err() {
                    break;
                }
            }
        }
        writer.abort();
    });
    DeckHost {
        port,
        to_plugin,
        from_plugin: host_inbox,
    }
}

fn spawn_plugin(opendeck_port: u16, hub_addr: SocketAddr) -> Child {
    Command::new(env!("PLUGIN_BIN"))
        .args([
            "-port",
            &opendeck_port.to_string(),
            "-pluginUUID",
            "pl.sotomski.opendeck.openmasterhub",
            "-registerEvent",
            "registerPlugin",
        ])
        .env("MASTERHUBD_LISTEN", hub_addr.to_string())
        .kill_on_drop(true)
        .spawn()
        .unwrap()
}

#[tokio::test]
async fn plugin_starts_masterhubd_when_nothing_is_listening() {
    let reserved = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let hub_addr = reserved.local_addr().unwrap();
    drop(reserved);

    let DeckHost {
        port,
        from_plugin: mut host_inbox,
        ..
    } = deck_host().await;
    let mut plugin = spawn_plugin(port, hub_addr);

    let register = next_message(&mut host_inbox).await;
    assert_eq!(register["event"], "registerPlugin");

    let mut deck = connect_hub(hub_addr).await;
    let hello: Value = serde_json::from_str(&next_text(&mut deck).await).unwrap();
    assert_eq!(hello["type"], "hello");
    assert_eq!(hello["daemon"], "masterhubd");
    assert_eq!(hello["usb"], false);
    assert_eq!(hello["modules"], serde_json::json!([]));
    assert_no_registration(&mut host_inbox).await;
    drop(deck);

    let _ = plugin.kill().await;
    let closed = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if tokio::net::TcpStream::connect(hub_addr).await.is_err() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;
    assert!(
        closed.is_ok(),
        "masterhubd kept listening after the plugin exited"
    );
}

#[tokio::test]
async fn key_press_and_image_round_trip() {
    let hub_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let hub_addr = hub_listener.local_addr().unwrap();
    tokio::spawn(async move {
        masterhubd::serve_listener(hub_listener).await.unwrap();
    });

    let DeckHost {
        port: opendeck_port,
        to_plugin,
        from_plugin: mut host_inbox,
    } = deck_host().await;
    let mut plugin = spawn_plugin(opendeck_port, hub_addr);

    let register = next_message(&mut host_inbox).await;
    assert_eq!(register["event"], "registerPlugin");
    assert_eq!(register["uuid"], "pl.sotomski.opendeck.openmasterhub");

    let (mut deck, _) = connect_async(format!("ws://{hub_addr}")).await.unwrap();
    let hello: Value = serde_json::from_str(&next_text(&mut deck).await).unwrap();
    assert_eq!(hello["type"], "hello");
    assert_eq!(hello["modules"], serde_json::json!([]));

    let trigger = r#"{"v":1,"type":"trigger","action":"key_state","payload":{"module":{"model":"M4315","id":"keys"},"control":0,"pressed":true}}"#;
    let key_down = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            deck.send(Message::Text(trigger.to_string().into()))
                .await
                .unwrap();
            match tokio::time::timeout(Duration::from_millis(200), host_inbox.recv()).await {
                Ok(Some(message)) if message["event"] == "getGlobalSettings" => continue,
                Ok(Some(message)) => break message,
                Ok(None) => panic!("plugin closed the OpenDeck socket"),
                Err(_) => continue,
            }
        }
    })
    .await
    .expect("timed out waiting for keyDown");
    assert_ne!(key_down["event"], "registerDevice");
    assert_eq!(key_down["event"], "keyDown");
    assert_eq!(key_down["payload"]["device"], "masterhub");
    assert_eq!(key_down["payload"]["position"], 0);

    let image = tiny_jpeg_data_url();
    to_plugin
        .send(format!(
            r#"{{"event":"setImage","device":"masterhub","controller":"Keypad","position":0,"image":"{image}"}}"#
        ))
        .unwrap();

    let painted = next_event(&mut deck, "set_button_image").await;
    assert_eq!(painted["payload"]["control"], 0);
    assert_eq!(painted["payload"]["module"]["id"], "keys");
    let jpeg = STANDARD
        .decode(painted["payload"]["data_base64"].as_str().unwrap())
        .unwrap();
    assert!(jpeg.starts_with(&[0xFF, 0xD8]));

    to_plugin
        .send(
            r#"{"event":"setImage","device":"masterhub","controller":null,"position":null,"image":null}"#
                .to_string(),
        )
        .unwrap();
    let mut cleared = Vec::new();
    for _ in 0..15 {
        cleared.push(next_event(&mut deck, "set_button_image").await);
    }
    cleared.sort_by_key(|event| event["payload"]["control"].as_u64().unwrap());
    for (control, event) in cleared.iter().enumerate() {
        assert_eq!(event["payload"]["control"], control as u64);
        let bytes = STANDARD
            .decode(event["payload"]["data_base64"].as_str().unwrap())
            .unwrap();
        let image = image::load_from_memory(&bytes).unwrap();
        assert_eq!((image.width(), image.height()), (72, 72));
    }

    let _ = plugin.kill().await;
}

fn tiny_jpeg_data_url() -> String {
    let image = image::RgbImage::from_pixel(2, 2, image::Rgb([12, 34, 56]));
    let mut bytes = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Jpeg)
        .unwrap();
    format!("data:image/jpeg;base64,{}", STANDARD.encode(bytes))
}

/// The plugin reads hello and decides not to register before this returns.
async fn assert_no_registration(inbox: &mut mpsc::UnboundedReceiver<Value>) {
    let quiet = tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            let message = inbox
                .recv()
                .await
                .expect("plugin closed the OpenDeck socket");
            assert_ne!(
                message["event"], "registerDevice",
                "registered a device when no module is open"
            );
        }
    })
    .await;
    assert!(
        quiet.is_err(),
        "plugin inbox closed while checking registration"
    );
}

async fn connect_hub(
    addr: SocketAddr,
) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
    let url = format!("ws://{addr}");
    for _ in 0..50 {
        if let Ok(Ok((socket, _))) =
            tokio::time::timeout(Duration::from_millis(200), connect_async(&url)).await
        {
            return socket;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("masterhubd did not accept a connection");
}

async fn next_message(inbox: &mut mpsc::UnboundedReceiver<Value>) -> Value {
    tokio::time::timeout(Duration::from_secs(5), inbox.recv())
        .await
        .expect("timed out waiting for OpenDeck message")
        .expect("plugin closed the OpenDeck socket")
}

async fn next_text<S>(socket: &mut S) -> String
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    loop {
        let message = tokio::time::timeout(Duration::from_secs(5), socket.next())
            .await
            .expect("timed out waiting for hub frame")
            .expect("hub closed")
            .expect("hub error");
        if let Message::Text(text) = message {
            return text.to_string();
        }
    }
}

async fn next_event<S>(socket: &mut S, action: &str) -> Value
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    loop {
        let value: Value = serde_json::from_str(&next_text(socket).await).unwrap();
        if value["type"] == "event" && value["action"] == action {
            return value;
        }
    }
}
