extern crate nalgebra as na;
extern crate nalgebra_glm as glm;
#[macro_use]
extern crate log;
#[macro_use]
extern crate anyhow;

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
    
    let mut renderer_events = gpu::RendererEvents::default();

    let mut gpu_handle = gpu::Gpu::spawn(&window, surface)?;

    event_loop.run(move |event, _, control_flow| {
        let original_control_flow = *control_flow;
        *control_flow = ControlFlow::Wait;

        if let Err(err) = || -> Result<()> {
            match event {
                Event::MainEventsCleared => window.request_redraw(),
                Event::WindowEvent { event, .. } => match event {
                    WindowEvent::Resized(size) => {
                        renderer_events.resize = Some(size);
                    }
                    WindowEvent::CloseRequested => {
                        info!("Window close requested");

                        gpu_handle.shutdown()?;
                        
                        *control_flow = ControlFlow::Exit;
                    }
                    _ => {}
                }
                Event::RedrawRequested(_) => {
                    if original_control_flow != ControlFlow::Exit {
                        // Let's send the renderer events to the renderer and clear our copy.
                        gpu_handle.send(renderer_events.take())?;
                    }
                }
                _ => {},
            }

            Ok(())
        }() {
            handle_fatal_error(err)
        }
    })
}

fn handle_fatal_error(err: anyhow::Error) -> ! {
    eprintln!("Error: {}", err);
    for cause in err.chain().skip(1) {
        eprintln!("because: {}", cause);
    }
    std::process::exit(1);
}
fn main() {
    logging::setup();

    if let Err(err) = run() {
        handle_fatal_error(err)
    }
}
