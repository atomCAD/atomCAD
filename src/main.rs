#[macro_use]
mod macros;
// mod arcball;
mod fps;
mod logging;

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
    logging::setup();

    if let Err(err) = run() {
        eprintln!("Error: {}", err);
        for cause in err.chain().skip(1) {
            eprintln!("because: {}", cause);
        }
        std::process::exit(1);
    }
}
