//! hud binary: composition root, terminal lifecycle and event loop.

mod logging;
mod terminal;

use std::time::Duration;

use anyhow::Context;
use crossterm::event::EventStream;
use futures::StreamExt;
use hud_application::AppEvent;
use hud_tui::{Model, Msg, update, view};
use tokio::sync::mpsc;
use tokio::time::MissedTickBehavior;

/// Capacity of the channel from use cases to the UI (bounded for backpressure).
const APP_EVENT_CAPACITY: usize = 256;
const RENDER_TICK: Duration = Duration::from_millis(250);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _log_guard = logging::init().context("failed to initialise logging")?;
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "starting hud");

    // The sender is handed to use cases once they exist; keeping it alive keeps
    // the receiving branch of the event loop active.
    let (_app_events_tx, mut app_events) = mpsc::channel::<AppEvent>(APP_EVENT_CAPACITY);

    let mut session = terminal::Session::enter().context("failed to prepare the terminal")?;
    let result = run(session.terminal(), &mut app_events).await;
    drop(session);

    tracing::info!("stopping hud");
    result
}

async fn run(
    terminal: &mut terminal::Tui,
    app_events: &mut mpsc::Receiver<AppEvent>,
) -> anyhow::Result<()> {
    let mut model = Model::default();
    let mut input = EventStream::new();
    let mut render_tick = tokio::time::interval(RENDER_TICK);
    render_tick.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        terminal.draw(|frame| view(&model, frame))?;

        // All three branches are cancel-safe: EventStream::next, Receiver::recv
        // and Interval::tick lose nothing when another branch wins.
        let msg = tokio::select! {
            Some(event) = input.next() => match Msg::from_terminal(&event?) {
                Some(msg) => msg,
                None => continue,
            },
            Some(event) = app_events.recv() => Msg::App(event),
            _ = render_tick.tick() => Msg::Tick,
        };

        #[expect(
            clippy::never_loop,
            reason = "Command has no variants until the first use case exists"
        )]
        for command in update(&mut model, msg) {
            match command {}
        }
        if model.should_quit() {
            return Ok(());
        }
    }
}
