// Copyright (c) 2020 by Lachlan Sneff <lachlan@charted.space>
// Copyright (c) 2020 by Mark Friedenbach <mark@friedenbach.org>

#[macro_use]
mod macros;
// mod arcball;
mod fps;
mod logging;
use log::error;

mod hub;
use self::hub::Hub;

mod compositor;
mod debug_metrics;
mod most_recent;
mod scene;
mod ui;

use anyhow::Result;
use std::convert::Infallible;
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

// End of File
