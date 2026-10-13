use std::sync::{LazyLock, Mutex};

use openaction::{
    ActionEventHandler, AppearEvent, DialPressEvent, DialRotateEvent,
    DidReceiveGlobalSettingsEvent, DidReceiveSettingsEvent, EventHandlerResult, GlobalEventHandler,
    KeyEvent, OutboundEventManager, SetBrightnessEvent, SetImageEvent,
};
use serde_json::{json, Value};

mod actions;
mod dial_feedback;
mod icons;
mod map;
mod sim;

const PLUGIN_UUID: &str = "com.openmasterhub.opendeck";

static BRIGHTNESS: LazyLock<Mutex<actions::BrightnessBook>> =
    LazyLock::new(|| Mutex::new(actions::BrightnessBook::default()));

struct Handler;

impl GlobalEventHandler for Handler {
    async fn plugin_ready(&self, outbound: &mut OutboundEventManager) -> EventHandlerResult {
        tokio::spawn(sim::run());
        log::info!("Master Hub plugin ready");
        outbound
            .send_event(json!({
                "event": "getGlobalSettings",
                "context": PLUGIN_UUID,
            }))
            .await
            .map_err(anyhow::Error::from)?;
        Ok(())
    }

    async fn set_image(
        &self,
        event: SetImageEvent,
        _outbound: &mut OutboundEventManager,
    ) -> EventHandlerResult {
        if event.image.is_some()
            && event.device == map::DEVICE_ID
            && event.controller.as_deref() == Some("Encoder")
            && event.position == Some(map::BIG_KNOB_ENCODER)
            && BRIGHTNESS.lock().expect("brightness").shows_dial()
        {
            return Ok(());
        }
        for command in map::image_command(
            &event.device,
            event.controller.as_deref(),
            event.position,
            event.image.as_deref(),
        ) {
            sim::enqueue(command).await;
        }
        Ok(())
    }

    async fn set_brightness(
        &self,
        event: SetBrightnessEvent,
        _outbound: &mut OutboundEventManager,
    ) -> EventHandlerResult {
        if let Some(command) = map::brightness_command(&event.device, event.brightness) {
            sim::enqueue(command).await;
        }
        Ok(())
    }

    async fn did_receive_global_settings(
        &self,
        event: DidReceiveGlobalSettingsEvent,
        _outbound: &mut OutboundEventManager,
    ) -> EventHandlerResult {
        let frame = {
            let mut book = BRIGHTNESS.lock().expect("brightness");
            book.apply_global(&event.payload.settings)
        };
        if let Some(frame) = frame {
            show_dial(&frame).await;
        }
        Ok(())
    }
}

struct Actions;

impl ActionEventHandler for Actions {
    async fn key_down(
        &self,
        event: KeyEvent,
        outbound: &mut OutboundEventManager,
    ) -> EventHandlerResult {
        if event.action == actions::SET_BRIGHTNESS {
            return step_brightness(
                &event.context,
                &event.payload.settings,
                actions::button_delta(&event.payload.settings),
                false,
                outbound,
            )
            .await;
        }
        press_action(
            &event.context,
            &event.action,
            &event.payload.settings,
            outbound,
        )
        .await
    }

    async fn dial_down(
        &self,
        event: DialPressEvent,
        outbound: &mut OutboundEventManager,
    ) -> EventHandlerResult {
        if event.action == actions::SET_BRIGHTNESS {
            let dial =
                actions::is_dial(&event.payload.controller, event.payload.coordinates.column);
            return toggle_brightness(&event.context, &event.payload.settings, dial, outbound)
                .await;
        }
        press_action(
            &event.context,
            &event.action,
            &event.payload.settings,
            outbound,
        )
        .await
    }

    async fn dial_rotate(
        &self,
        event: DialRotateEvent,
        outbound: &mut OutboundEventManager,
    ) -> EventHandlerResult {
        if event.action != actions::SET_BRIGHTNESS {
            return Ok(());
        }
        let dial = actions::is_dial("Encoder", event.payload.coordinates.column);
        step_brightness(
            &event.context,
            &event.payload.settings,
            actions::turn_delta(&event.payload.settings, event.payload.ticks),
            dial,
            outbound,
        )
        .await
    }

    async fn will_appear(
        &self,
        event: AppearEvent,
        outbound: &mut OutboundEventManager,
    ) -> EventHandlerResult {
        let button = event.payload.controller == "Keypad";
        if event.action == actions::SET_BRIGHTNESS {
            let dial =
                actions::is_dial(&event.payload.controller, event.payload.coordinates.column);
            let screen = {
                let mut book = BRIGHTNESS.lock().expect("brightness");
                book.appear(&event.context, &event.payload.settings, dial, button)
            };
            if let Some(frame) = screen {
                show_dial(&frame).await;
            }
        }
        let placement = (event.action == actions::SET_BRIGHTNESS).then_some(button);
        show_action_image(
            &event.context,
            &event.action,
            &event.payload.settings,
            placement,
            outbound,
        )
        .await
    }

    async fn will_disappear(
        &self,
        event: AppearEvent,
        _outbound: &mut OutboundEventManager,
    ) -> EventHandlerResult {
        if event.action == actions::SET_BRIGHTNESS {
            BRIGHTNESS
                .lock()
                .expect("brightness")
                .disappear(&event.context);
        }
        Ok(())
    }

    async fn did_receive_settings(
        &self,
        event: DidReceiveSettingsEvent,
        outbound: &mut OutboundEventManager,
    ) -> EventHandlerResult {
        let settings = event.payload.settings;
        let placement = if event.action == actions::SET_BRIGHTNESS {
            let mut book = BRIGHTNESS.lock().expect("brightness");
            let placement = book.slot_button(&event.context);
            book.note(&event.context, settings.clone());
            placement
        } else {
            None
        };
        show_action_image(
            &event.context,
            &event.action,
            &settings,
            placement,
            outbound,
        )
        .await
    }
}

async fn step_brightness(
    context: &str,
    settings: &Value,
    delta: Option<i32>,
    dial: bool,
    outbound: &mut OutboundEventManager,
) -> EventHandlerResult {
    let Some(delta) = delta else {
        return Ok(());
    };
    let outcome = {
        let mut book = BRIGHTNESS.lock().expect("brightness");
        let change = book.step(context, settings, delta, dial);
        let Some(change) = change else {
            return Ok(());
        };
        let mut save = change.source_settings.clone();
        if book.slot_button(context) == Some(true) {
            if let Some(flipped) = actions::flip_brightness_source(&save) {
                save = flipped.clone();
                book.note(context, flipped);
            }
        }
        let globals = book.global_settings();
        (change, save, globals)
    };
    let (change, save, globals) = outcome;
    for command in actions::brightness_output(&change.source_settings, change.screen, change.label)
    {
        sim::enqueue(command).await;
    }
    outbound
        .set_settings(context.to_string(), save)
        .await
        .map_err(anyhow::Error::from)?;
    for (dial_context, settings) in change.dials {
        outbound
            .set_settings(dial_context, settings)
            .await
            .map_err(anyhow::Error::from)?;
    }
    outbound
        .send_event(json!({
            "event": "setGlobalSettings",
            "context": PLUGIN_UUID,
            "payload": globals,
        }))
        .await
        .map_err(anyhow::Error::from)?;
    Ok(())
}

async fn toggle_brightness(
    context: &str,
    settings: &Value,
    dial: bool,
    outbound: &mut OutboundEventManager,
) -> EventHandlerResult {
    let flipped = {
        let mut book = BRIGHTNESS.lock().expect("brightness");
        book.flip(context, settings, dial)
    };
    let Some(flipped) = flipped else {
        return Ok(());
    };
    if let Some(frame) = &flipped.screen {
        show_dial(frame).await;
    }
    outbound
        .set_settings(context.to_string(), flipped.settings)
        .await
        .map_err(anyhow::Error::from)?;
    Ok(())
}

async fn show_dial(frame: &actions::DialFrame) {
    if let Some(command) = actions::dial_screen_command(frame.level, frame.label) {
        sim::enqueue(command).await;
    }
}

async fn press_action(
    context: &str,
    action: &str,
    settings: &Value,
    outbound: &mut OutboundEventManager,
) -> EventHandlerResult {
    if action == actions::SET_MODE {
        if let Some(updated) = actions::stepped_mode(settings) {
            dispatch(action, &updated).await;
            outbound
                .set_settings(context.to_string(), updated)
                .await
                .map_err(anyhow::Error::from)?;
            return Ok(());
        }
    }
    dispatch(action, settings).await;
    Ok(())
}

async fn show_action_image(
    context: &str,
    action: &str,
    settings: &Value,
    button: Option<bool>,
    outbound: &mut OutboundEventManager,
) -> EventHandlerResult {
    let Some(image) = actions::action_image(action, settings, button) else {
        return Ok(());
    };
    outbound
        .set_image(context.to_string(), Some(image), Some(0))
        .await
        .map_err(anyhow::Error::from)?;
    Ok(())
}

async fn dispatch(action: &str, settings: &Value) {
    for command in actions::commands(action, settings) {
        sim::enqueue(command).await;
    }
}

#[tokio::main]
async fn main() {
    let _ =
        simplelog::SimpleLogger::init(simplelog::LevelFilter::Info, simplelog::Config::default());
    if let Err(error) = openaction::init_plugin(Handler, Actions).await {
        log::error!("plugin stopped: {error}");
        std::process::exit(1);
    }
}
