use std::process::{Child, Command, Stdio};
use std::sync::LazyLock;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::sync::{mpsc, Mutex};
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
    let mut daemon: Option<Child> = None;
    loop {
        match session().await {
            Ok(()) => {
                reap(&mut daemon);
                log::info!("simulator session ended");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            Err(error) => {
                log::debug!("simulator connection failed: {error}");
                ensure_daemon(&mut daemon).await;
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }
    }
}

async fn ensure_daemon(daemon: &mut Option<Child>) {
    reap(daemon);
    if port_open().await {
        return;
    }
    if daemon.is_none() {
        log::info!("starting masterhubd on {}", listen_addr());
        *daemon = launch_daemon();
    }
    if !wait_for_port().await {
        log::error!("masterhubd is not listening on {}", listen_addr());
    }
    reap(daemon);
}

fn reap(daemon: &mut Option<Child>) {
    let Some(child) = daemon.as_mut() else {
        return;
    };
    match child.try_wait() {
        Ok(Some(status)) => {
            if !status.success() {
                log::error!("masterhubd exited: {status}");
            }
            *daemon = None;
        }
        Ok(None) => {}
        Err(error) => {
            log::error!("masterhubd status: {error}");
            *daemon = None;
        }
    }
}

/// Start a detached copy of this executable. A second copy exits 0 when the
/// port is already taken, so two plugins can launch at once. The daemon is
/// reparented away from this process, so quitting OpenDeck does not stop it.
fn launch_daemon() -> Option<Child> {
    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(error) => {
            log::error!("cannot find the plugin executable: {error}");
            return None;
        }
    };
    let mut cmd = Command::new(exe);
    cmd.arg("--masterhubd")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    #[cfg(unix)]
    detach_from_parent(&mut cmd);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
    }
    match cmd.spawn() {
        Ok(child) => Some(child),
        Err(error) => {
            log::error!("failed to start masterhubd: {error}");
            None
        }
    }
}

/// Double-fork and start a new session. Quitting OpenDeck kills its process
/// tree; a grandchild whose parent has already exited is reparented and stays up.
#[cfg(unix)]
fn detach_from_parent(cmd: &mut Command) {
    use std::os::unix::process::CommandExt;

    extern "C" {
        fn fork() -> i32;
        fn setsid() -> i32;
        fn _exit(code: i32) -> !;
    }

    // SAFETY: runs in the forked child before exec, and only calls
    // async-signal-safe functions.
    unsafe {
        cmd.pre_exec(|| match fork() {
            -1 => Err(std::io::Error::last_os_error()),
            0 => {
                if setsid() == -1 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(())
                }
            }
            _ => _exit(0),
        });
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

    let mut shown: Option<map::ConnectedModules> = None;
    let mut slider_levels = [None; 5];
    let result = async {
        let hello = next_text(&mut read).await?;
        let hello: serde_json::Value = serde_json::from_str(&hello)?;
        if hello["type"] != "hello" {
            anyhow::bail!("expected masterhubd hello");
        }
        sync_modules(&mut shown, map::ConnectedModules::from_report(&hello)).await?;

        while let Some(incoming) = read.next().await {
            match incoming {
                Ok(Message::Text(text)) => {
                    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
                        continue;
                    };
                    if value["type"] == "event" && value["action"] == "modules" {
                        slider_levels = [None; 5];
                        sync_modules(
                            &mut shown,
                            map::ConnectedModules::from_report(&value["payload"]),
                        )
                        .await?;
                        continue;
                    }
                    if let Some(event) = map::map_event(&value) {
                        match event {
                            map::DeviceEvent::SliderMoved { control, value } => {
                                let previous = slider_levels[usize::from(control)];
                                slider_levels[usize::from(control)] = Some(value);
                                if let Some(ticks) = map::slider_delta(previous, value) {
                                    forward(map::DeviceEvent::EncoderChange {
                                        position: map::SLIDER_ENCODER + control,
                                        ticks,
                                    })
                                    .await?;
                                }
                            }
                            other => forward(other).await?,
                        }
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
    if shown.is_some() {
        deregister_device().await;
    }
    result
}

async fn sync_modules(
    shown: &mut Option<map::ConnectedModules>,
    next: map::ConnectedModules,
) -> Result<(), anyhow::Error> {
    if !next.visible() {
        if shown.is_some() {
            deregister_device().await;
            *shown = None;
        }
        return Ok(());
    }
    if *shown == Some(next) {
        return Ok(());
    }
    register_device(next).await?;
    *shown = Some(next);
    log::info!("registered {DEVICE_ID}");
    Ok(())
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

async fn register_device(modules: map::ConnectedModules) -> Result<(), anyhow::Error> {
    let mut guard = openaction::OUTBOUND_EVENT_MANAGER.lock().await;
    let Some(outbound) = guard.as_mut() else {
        anyhow::bail!("OpenDeck connection is not ready");
    };
    outbound
        .send_event(map::registration_event(modules))
        .await?;
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
        map::DeviceEvent::SliderMoved { .. } => {}
    }
    Ok(())
}
