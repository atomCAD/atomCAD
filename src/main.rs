// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

extern crate nalgebra as na;

#[macro_use]
mod macros;
// mod arcball;
mod fps;
mod logging;
use log::error;
mod camera;

mod hub;
use self::hub::Hub;

mod command_encoder;
mod compositor;
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
