use std::sync::LazyLock;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

use crate::map::{self, DEVICE_ID};

static COMMANDS: LazyLock<Mutex<Option<mpsc::UnboundedSender<String>>>> =
    LazyLock::new(|| Mutex::new(None));

fn listen_addr() -> String {
    std::env::var("MASTERHUBD_LISTEN").unwrap_or_else(|_| "127.0.0.1:9123".to_string())
}

pub fn hub_url() -> String {
    format!("ws://{}", listen_addr())
}

pub async fn enqueue(text: String) {
    if let Some(sender) = COMMANDS.lock().await.as_ref() {
        let _ = sender.send(text);
    }
}

pub async fn run() {
    let mut daemon: Option<JoinHandle<()>> = None;
    loop {
        if !port_open().await {
            let running = daemon.as_ref().is_some_and(|task| !task.is_finished());
            if !running {
                log::info!("starting masterhubd on {}", listen_addr());
                daemon = Some(tokio::spawn(async {
                    match masterhubd::run().await {
                        Ok(()) => log::info!("masterhubd stopped"),
                        Err(error) => log::error!("masterhubd: {error}"),
                    }
                }));
            }
            if !wait_for_port().await {
                log::error!("masterhubd is not listening on {}", listen_addr());
            }
        }

        match session().await {
            Ok(()) => log::info!("simulator session ended"),
            Err(error) => log::debug!("simulator connection failed: {error}"),
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn port_open() -> bool {
    tokio::net::TcpStream::connect(listen_addr()).await.is_ok()
}

async fn wait_for_port() -> bool {
    for _ in 0..50 {
        if port_open().await {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    false
}

async fn session() -> Result<(), anyhow::Error> {
    let (socket, _) = connect_async(hub_url()).await?;
    let (mut write, mut read) = socket.split();
    let (commands, mut outgoing) = mpsc::unbounded_channel();
    *COMMANDS.lock().await = Some(commands);

    let writer = tokio::spawn(async move {
        while let Some(text) = outgoing.recv().await {
            if write.send(Message::Text(text.into())).await.is_err() {
                break;
            }
        }
    });

    let mut registered = false;
    let result = async {
        let hello = next_text(&mut read).await?;
        let hello: serde_json::Value = serde_json::from_str(&hello)?;
        if hello["type"] != "hello" {
            anyhow::bail!("expected masterhubd hello");
        }
        register_device().await?;
        registered = true;
        log::info!("registered {DEVICE_ID}");

        while let Some(incoming) = read.next().await {
            match incoming {
                Ok(Message::Text(text)) => {
                    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
                        continue;
                    };
                    if let Some(event) = map::map_event(&value) {
                        forward(event).await?;
                    }
                }
                Ok(Message::Ping(_) | Message::Pong(_)) => {}
                Ok(_) => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }
    .await;

    let _ = COMMANDS.lock().await.take();
    writer.abort();
    if registered {
        deregister_device().await;
    }
    result
}

async fn next_text<S>(read: &mut S) -> Result<String, anyhow::Error>
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    loop {
        match read.next().await {
            Some(Ok(Message::Text(text))) => return Ok(text.to_string()),
            Some(Ok(Message::Ping(_) | Message::Pong(_))) => {}
            Some(Ok(_)) => anyhow::bail!("unexpected websocket frame"),
            Some(Err(error)) => return Err(error.into()),
            None => anyhow::bail!("masterhubd closed before hello"),
        }
    }
}

async fn register_device() -> Result<(), anyhow::Error> {
    let mut guard = openaction::OUTBOUND_EVENT_MANAGER.lock().await;
    let Some(outbound) = guard.as_mut() else {
        anyhow::bail!("OpenDeck connection is not ready");
    };
    outbound.send_event(map::registration_event()).await?;
    Ok(())
}

async fn deregister_device() {
    let mut guard = openaction::OUTBOUND_EVENT_MANAGER.lock().await;
    let Some(outbound) = guard.as_mut() else {
        return;
    };
    if let Err(error) = outbound.deregister_device(DEVICE_ID.to_string()).await {
        log::error!("failed to deregister {DEVICE_ID}: {error}");
    } else {
        log::info!("deregistered {DEVICE_ID}");
    }
}

async fn forward(event: map::DeviceEvent) -> Result<(), anyhow::Error> {
    let mut guard = openaction::OUTBOUND_EVENT_MANAGER.lock().await;
    let Some(outbound) = guard.as_mut() else {
        anyhow::bail!("OpenDeck connection is not ready");
    };
    let device = DEVICE_ID.to_string();
    match event {
        map::DeviceEvent::KeyDown(position) => outbound.key_down(device, position).await?,
        map::DeviceEvent::KeyUp(position) => outbound.key_up(device, position).await?,
        map::DeviceEvent::EncoderDown(position) => outbound.encoder_down(device, position).await?,
        map::DeviceEvent::EncoderUp(position) => outbound.encoder_up(device, position).await?,
        map::DeviceEvent::EncoderChange { position, ticks } => {
            outbound
                .send_event(serde_json::json!({
                    "event": "encoderChange",
                    "payload": {
                        "device": device,
                        "position": position,
                        "ticks": ticks,
                    },
                }))
                .await?
        }
    }
    Ok(())
}
