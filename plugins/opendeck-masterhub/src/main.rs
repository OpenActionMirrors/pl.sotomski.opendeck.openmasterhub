use openaction::{
    ActionEventHandler, EventHandlerResult, GlobalEventHandler, OutboundEventManager,
    SetBrightnessEvent, SetImageEvent,
};

mod map;
mod sim;

struct Handler;

impl GlobalEventHandler for Handler {
    async fn plugin_ready(&self, _outbound: &mut OutboundEventManager) -> EventHandlerResult {
        tokio::spawn(sim::run());
        log::info!("Master Hub plugin ready");
        Ok(())
    }

    async fn set_image(
        &self,
        event: SetImageEvent,
        _outbound: &mut OutboundEventManager,
    ) -> EventHandlerResult {
        if let Some(command) = map::image_command(
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
}

struct Actions;

impl ActionEventHandler for Actions {}

#[tokio::main]
async fn main() {
    let _ =
        simplelog::SimpleLogger::init(simplelog::LevelFilter::Info, simplelog::Config::default());
    if let Err(error) = openaction::init_plugin(Handler, Actions).await {
        log::error!("plugin stopped: {error}");
        std::process::exit(1);
    }
}
