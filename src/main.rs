// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use atomcad::camera::ArcballCamera;
use atomcad::menubar;
use atomcad::pdb;
use atomcad::APP_NAME;
use common::InputEvent;
use render::{Interactions, RenderOptions, Renderer, World};

use ultraviolet::Vec3;
#[cfg(target_os = "macos")]
use winit::platform::macos::EventLoopBuilderExtMacOS;
use winit::{
    event::{Event, StartCause, WindowEvent},
    event_loop::{ControlFlow, EventLoop, EventLoopBuilder},
    window::{Window, WindowBuilder},
};

async fn run(event_loop: EventLoop<()>, mut window: Option<Window>) {
    window.as_ref().expect("window should exist");

    let (mut renderer, gpu_resources) = Renderer::new(
        window.as_ref().unwrap(),
        RenderOptions {
            fxaa: Some(()), // placeholder
            attempt_gpu_driven: true,
        },
    )
    .await;

    renderer.set_camera(ArcballCamera::new(Vec3::zero(), 100.0, 1.0));

    let mut world = World::new();

    let mut neon_pump = pdb::load_from_pdb_str(
        &gpu_resources,
        "Neon Pump",
        include_str!("../assets/neon_pump_imm.pdb"),
    )
    .expect("failed to load pdb");

    println!(
        "Loaded {} parts and {} fragments",
        neon_pump.parts().len(),
        neon_pump.fragments().len()
    );

    // Center the neon pump around the origin, so that the rotating arcball
    // camera will be centered on it.
    for part in neon_pump.parts_mut() {
        // This doesn't let the world know that this part is going to be
        // updated, but we're adding them for the first time, so it'll work
        // anyhow.
        part.move_to(0.0, 0.0, 0.0);
    }

    world.merge(neon_pump);

    let interations = Interactions::default();

    // Run the event loop.
    event_loop.run(move |event, _, control_flow| {
        // When we are done handling this event, suspend until the next event.
        *control_flow = ControlFlow::Wait;

        // Handle events.
        match event {
            Event::NewEvents(StartCause::Init) => {
                // Will be called once when the event loop starts.
            }
            Event::WindowEvent {
                event: WindowEvent::Resized(new_size),
                ..
            } => {
                renderer.resize(new_size);
            }
            Event::MainEventsCleared => {
                // The event queue is empty, so we can safely redraw the window.
                if window.is_some() {
                    // Winit prevents sizing with CSS, so we have to set
                    // the size manually when on web.
                    #[cfg(target_arch = "wasm32")]
                    (|| {
                        use winit::dpi::PhysicalSize;
                        log::error!("Resizing window");
                        let win = web_sys::window()?;
                        let width = win.inner_width().ok()?.as_f64()?;
                        let height = win.inner_height().ok()?.as_f64()?;
                        window.as_ref().map(|window| {
                            let scale_factor = window.scale_factor();
                            let new_size = PhysicalSize::new(
                                (width * scale_factor) as u32,
                                (height * scale_factor) as u32,
                            );
                            window.set_inner_size(new_size);
                            renderer.resize(new_size);
                            Some(())
                        })
                    })();
                    renderer.render(&mut world, &interations);
                }
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                // The user has requested to close the window.
                // Drop the window to fire the `Destroyed` event.
                window = None;
            }
            Event::WindowEvent {
                event: WindowEvent::Destroyed,
                ..
            } => {
                // The window has been destroyed, time to exit stage left.
                *control_flow = ControlFlow::ExitWithCode(0);
            }
            Event::LoopDestroyed => {
                // The event loop has been destroyed, so we can safely terminate
                // the application.  This is the very last event we will ever
                // receive, so we can safely perform final rites.
            }
            Event::WindowEvent { event, .. } => {
                renderer.camera().update(InputEvent::Window(event));
            }
            Event::DeviceEvent { event, .. } => {
                renderer.camera().update(InputEvent::Device(event));
            }
            _ => {
                // Unknown event; do nothing.
            }
        }
    })
}

fn main() {
    // Create the event loop.
    let mut event_loop = EventLoopBuilder::new();
    #[cfg(target_os = "macos")] // We will create our own menu bar.
    event_loop.with_default_menu(false);
    let event_loop = event_loop.build();

    // Create the main window.
    let window = match WindowBuilder::new().with_title(APP_NAME).build(&event_loop) {
        Err(e) => {
            println!("Failed to create window: {}", e);
            std::process::exit(1);
        }
        Ok(window) => window,
    };

    // Add the menu bar to the window / application instance, using native
    // APIs.
    menubar::setup_menu_bar(&window);

    #[cfg(not(target_arch = "wasm32"))]
    {
        env_logger::init();
        futures::executor::block_on(run(event_loop, Some(window)));
    }
    #[cfg(target_arch = "wasm32")]
    {
        std::panic::set_hook(Box::new(console_error_panic_hook::hook));
        console_log::init().expect("could not initialize logger");
        // Winit prevents sizing with CSS, so we have to set
        // the size manually when on web.
        use winit::dpi::PhysicalSize;
        let width = web_sys::window()
            .and_then(|win| win.inner_width().ok())
            .and_then(|w| w.as_f64())
            .unwrap_or(800.0);
        let height = web_sys::window()
            .and_then(|win| win.inner_height().ok())
            .and_then(|h| h.as_f64())
            .unwrap_or(600.0);
        let scale_factor = window.scale_factor();
        window.set_inner_size(PhysicalSize::new(
            width * scale_factor,
            height * scale_factor,
        ));
        // On wasm, append the canvas to the document body
        use winit::platform::web::WindowExtWebSys;
        web_sys::window()
            .and_then(|win| win.document())
            .and_then(|doc| {
                let dst = doc.get_element_by_id("app-container")?;
                let canvas = web_sys::Element::from(window.canvas()?);
                dst.append_child(&canvas).ok()?;
                Some(())
            })
            .expect("Couldn't append canvas to document body.");
        wasm_bindgen_futures::spawn_local(run(event_loop, Some(window)));
    }
}

// End of File
