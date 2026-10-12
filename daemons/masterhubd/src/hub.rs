use std::sync::Arc;

use tokio::sync::broadcast;

use crate::protocol::{self, CommandSink, OutputCommand};

pub struct Hub {
    events: broadcast::Sender<String>,
    sink: Arc<dyn CommandSink>,
    debug: bool,
}

impl Hub {
    pub fn new(sink: Arc<dyn CommandSink>) -> Self {
        Self::with_debug(sink, debug_actions())
    }

    fn with_debug(sink: Arc<dyn CommandSink>, debug: bool) -> Self {
        let (events, _) = broadcast::channel(64);
        Self {
            events,
            sink,
            debug,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.events.subscribe()
    }

    /// Publish an input event to every connected client.
    ///
    /// Accepted triggers call this. A USB reader calls it with the same
    /// action names and payloads. When `MASTERHUBD_DEBUG` is `1` or `true`,
    /// the action is also printed on stderr.
    pub fn emit_event(&self, action: &str, payload: &serde_json::Value) {
        if self.debug {
            eprintln!("masterhubd: {action} {}", action_log_payload(payload));
        }
        let _ = self.events.send(protocol::event_json(action, payload));
    }

    pub fn handle_client_text(&self, text: &str) -> String {
        match protocol::parse_client_message(text) {
            Ok(protocol::ParsedMessage::Trigger {
                id,
                action,
                payload,
            }) => {
                self.emit_event(action, &payload);
                protocol::result_json(id.as_deref())
            }
            Ok(protocol::ParsedMessage::Command { id, command }) => {
                if let Some((action, payload)) = command.simulator_event() {
                    self.emit_event(action, &payload);
                }
                self.sink.accept(&command);
                protocol::result_json(id.as_deref())
            }
            Err(err) => protocol::error_json(err.id.as_deref(), &err.message),
        }
    }
}

fn debug_actions() -> bool {
    debug_flag(std::env::var("MASTERHUBD_DEBUG").ok().as_deref())
}

fn debug_flag(value: Option<&str>) -> bool {
    value.is_some_and(|value| {
        let value = value.to_ascii_lowercase();
        value == "1" || value == "true"
    })
}

/// Compact payload for the debug line. Image bytes stay off stderr.
fn action_log_payload(payload: &serde_json::Value) -> String {
    if payload.get("data_base64").is_none() {
        return serde_json::to_string(payload).unwrap_or_else(|_| "{}".to_string());
    }
    let mut value = payload.clone();
    if let Some(object) = value.as_object_mut() {
        if let Some(data) = object.get("data_base64").and_then(|item| item.as_str()) {
            let chars = data.len();
            object.insert(
                "data_base64".to_string(),
                serde_json::Value::String(format!("<{chars} chars>")),
            );
        }
    }
    serde_json::to_string(&value).unwrap_or_else(|_| "{}".to_string())
}

/// Ignores output. Tests use this when the assertion is about events, not USB.
#[cfg_attr(not(test), allow(dead_code))]
pub struct NoopCommandSink;

impl CommandSink for NoopCommandSink {
    fn accept(&self, _command: &OutputCommand) {}
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    struct RecordingSink {
        actions: Mutex<Vec<&'static str>>,
    }

    impl CommandSink for RecordingSink {
        fn accept(&self, command: &OutputCommand) {
            self.actions
                .lock()
                .expect("sink lock")
                .push(command.action());
        }
    }

    fn hub() -> (Hub, Arc<RecordingSink>) {
        let sink = Arc::new(RecordingSink {
            actions: Mutex::new(Vec::new()),
        });
        let hub = Hub::new(sink.clone());
        (hub, sink)
    }

    #[test]
    fn trigger_emits_one_event_and_does_not_touch_the_sink() {
        let (hub, sink) = hub();
        let mut events = hub.subscribe();
        let reply = hub.handle_client_text(
            r#"{"v":1,"type":"trigger","id":"t1","action":"knob_rotated","payload":{"module":{"model":"M003K","id":"knobs"},"control":1,"direction":"ccw"}}"#,
        );
        let reply: serde_json::Value = serde_json::from_str(&reply).unwrap();
        assert_eq!(reply["type"], "result");
        assert_eq!(reply["id"], "t1");
        assert_eq!(reply["ok"], true);

        let event: serde_json::Value = serde_json::from_str(&events.try_recv().unwrap()).unwrap();
        assert_eq!(event["type"], "event");
        assert_eq!(event["action"], "knob_rotated");
        assert_eq!(event["payload"]["direction"], "ccw");
        assert!(event.get("id").is_none());
        assert!(events.try_recv().is_err());
        assert!(sink.actions.lock().unwrap().is_empty());
    }

    #[test]
    fn command_reaches_the_sink_and_is_not_broadcast() {
        let (hub, sink) = hub();
        let mut events = hub.subscribe();
        let reply = hub.handle_client_text(
            r#"{"v":1,"type":"command","action":"set_backlight","payload":{"module":{"model":"M4315","id":"keys"},"brightness":10}}"#,
        );
        let reply: serde_json::Value = serde_json::from_str(&reply).unwrap();
        assert_eq!(reply["type"], "result");
        assert!(reply.get("id").is_none());
        assert_eq!(reply["ok"], true);
        assert_eq!(sink.actions.lock().unwrap().as_slice(), ["set_backlight"]);
        assert!(events.try_recv().is_err());
    }

    #[test]
    fn button_image_is_broadcast_to_simulator_clients() {
        let (hub, sink) = hub();
        let mut events = hub.subscribe();
        let reply = hub.handle_client_text(
            r#"{"v":1,"type":"command","id":"img","action":"set_button_image","payload":{"module":{"model":"M4315","id":"keys"},"control":3,"data_base64":"/9j/2Q=="}}"#,
        );
        let reply: serde_json::Value = serde_json::from_str(&reply).unwrap();
        assert_eq!(reply["type"], "result");
        assert_eq!(reply["id"], "img");
        assert_eq!(reply["ok"], true);
        assert_eq!(
            sink.actions.lock().unwrap().as_slice(),
            ["set_button_image"]
        );

        let event: serde_json::Value = serde_json::from_str(&events.try_recv().unwrap()).unwrap();
        assert_eq!(event["type"], "event");
        assert_eq!(event["action"], "set_button_image");
        assert!(event.get("id").is_none());
        assert_eq!(event["payload"]["control"], 3);
        assert_eq!(event["payload"]["module"]["model"], "M4315");
        assert_eq!(event["payload"]["module"]["id"], "keys");
        assert_eq!(event["payload"]["data_base64"], "/9j/2Q==");
        assert!(events.try_recv().is_err());
    }

    #[test]
    fn debug_flag_accepts_one_and_true() {
        assert!(!debug_flag(None));
        assert!(!debug_flag(Some("")));
        assert!(!debug_flag(Some("0")));
        assert!(!debug_flag(Some("false")));
        assert!(debug_flag(Some("1")));
        assert!(debug_flag(Some("true")));
        assert!(debug_flag(Some("TRUE")));
    }

    #[test]
    fn action_log_keeps_module_fields_and_hides_image_bytes() {
        let slider = action_log_payload(&serde_json::json!({
            "module": { "model": "M005S", "id": "faders" },
            "control": 1,
            "value": 40
        }));
        assert_eq!(
            slider,
            r#"{"control":1,"module":{"id":"faders","model":"M005S"},"value":40}"#
        );

        let key = action_log_payload(&serde_json::json!({
            "module": { "model": "M4315", "id": "keys" },
            "control": 3,
            "pressed": true
        }));
        assert!(key.contains(r#""model":"M4315""#));
        assert!(key.contains(r#""pressed":true"#));

        let image = action_log_payload(&serde_json::json!({
            "module": { "model": "M4315", "id": "keys" },
            "control": 3,
            "data_base64": "/9j/2Q=="
        }));
        assert!(image.contains(r#""data_base64":"<8 chars>""#));
        assert!(!image.contains("/9j/"));
    }

    #[test]
    fn debug_logging_does_not_change_the_published_event() {
        let hub = Hub::with_debug(Arc::new(NoopCommandSink), true);
        let mut events = hub.subscribe();
        hub.emit_event(
            "slider_moved",
            &serde_json::json!({
                "module": { "model": "M005S", "id": "faders" },
                "control": 1,
                "value": 40
            }),
        );
        let event: serde_json::Value = serde_json::from_str(&events.try_recv().unwrap()).unwrap();
        assert_eq!(event["action"], "slider_moved");
        assert_eq!(event["payload"]["value"], 40);
        assert_eq!(event["payload"]["module"]["model"], "M005S");
    }

    #[test]
    fn invalid_trigger_emits_nothing() {
        let (hub, sink) = hub();
        let mut events = hub.subscribe();
        let reply = hub.handle_client_text(
            r#"{"v":1,"type":"trigger","id":"bad","action":"key_state","payload":{"module":{"model":"M4315","id":"keys"},"control":15,"pressed":true}}"#,
        );
        let reply: serde_json::Value = serde_json::from_str(&reply).unwrap();
        assert_eq!(reply["type"], "error");
        assert_eq!(reply["id"], "bad");
        assert!(events.try_recv().is_err());
        assert!(sink.actions.lock().unwrap().is_empty());
    }
}
