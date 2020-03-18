extern crate nalgebra as na;
extern crate nalgebra_glm as glm;
#[macro_use]
extern crate log;

mod arcball;
mod settings;
mod fps;
mod logging;
mod init;
mod gpu;

pub use self::settings::Settings;

use anyhow::{Result, Context};
use std::convert::Infallible;
use winit::{
    event::{self, Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::Window,
};

fn run() -> Result<Infallible> {
    let (event_loop, window, surface) = init::initialize_display()
        .context("Failed to initialize display")?;
    
    let mut renderer_events = gpu::RendererEvents::Empty;
    let gpu_handle = gpu::Gpu::spawn(&window, &surface)?;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;

        match event {
            Event::MainEventsCleared => window.request_redraw(),
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::Resized(size) => {
                    renderer_events.resize(size);
                }
                WindowEvent::CloseRequested => {
                    renderer_events.shutdown();
                    *control_flow = ControlFlow::Exit;
                }
                _ => {}
            }
            Event::RedrawRequested(_) => {
                // Let's send the renderer events to the renderer and clear our copy.
                gpu_handle.send(renderer_events.take());
            }
            _ => {},
        }
    })
}
fn main() -> Result<()> {
    logging::setup();

    run().map(|_| ())
}
