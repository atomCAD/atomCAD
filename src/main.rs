extern crate nalgebra as na;
extern crate nalgebra_glm as glm;
#[macro_use]
extern crate log;
extern crate anyhow;

#[macro_use]
mod macros;
// mod arcball;
mod fps;
mod logging;

mod hub;
use self::hub::Hub;

mod ui;
mod scene;

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
    logging::setup();

    if let Err(err) = run() {
        eprintln!("Error: {}", err);
        for cause in err.chain().skip(1) {
            eprintln!("because: {}", cause);
        }
        std::process::exit(1);
    }
}
