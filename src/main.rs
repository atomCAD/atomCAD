#[macro_use]
mod macros;
// mod arcball;
mod fps;
mod logging;
use log::error;

mod hub;
use self::hub::Hub;

mod ui;
mod scene;
mod debug_metrics;

use anyhow::Result;
use std::{
    convert::Infallible,
};
use winit::event_loop::EventLoop;

fn run() -> Result<Infallible> {
    let event_loop = EventLoop::new();

    Hub::new(&event_loop)?.run(event_loop)
}

fn main() {
    // Configure logging engine:
    logging::setup();

    if let Err(err) = run() {
        // We panic'd somewhere.  Report the error, and its causes, to the log.
        error!("Unhandled error: {}", err);
        for cause in err.chain().skip(1) {
            error!("because: {}", cause);
        }
        std::process::exit(1);
    }
}
