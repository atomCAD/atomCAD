// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::camera::ArcballCamera;
use crate::menubar::setup_menu_bar;
// use crate::rotating_camera::RotatingArcballCamera;
use common::InputEvent;
use render::{Interactions, RenderOptions, Renderer, World};

#[cfg(target_os = "macos")]
use winit::platform::macos::EventLoopBuilderExtMacOS;
use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop, EventLoopBuilder},
    window::{Window, WindowBuilder},
};

mod camera;
mod menubar;
// mod rotating_camera;
mod pdb;
mod platform;
mod platform_impl;
// mod ti;

pub const APP_NAME: &str = "atomCAD";

async fn run(event_loop: EventLoop<()>, window: Window) {
    let (mut renderer, gpu_resources) = Renderer::new(
        &window,
        RenderOptions {
            fxaa: Some(()), // placeholder
            attempt_gpu_driven: true,
        },
    )
    .await;

    renderer.set_camera(ArcballCamera::new(100.0, 1.0));

    let mut world = World::new();

    let mut neon_pump = pdb::load_from_pdb(&gpu_resources, "Neon Pump", "assets/neon_pump_imm.pdb")
        .expect("failed to load pdb");

    println!(
        "Loaded {} parts and {} fragments",
        neon_pump.parts().len(),
        neon_pump.fragments().len()
    );

    for part in neon_pump.parts_mut() {
        // This doesn't let the world now that this part is going to be updated,
        // but we're adding them for the first time, so it'll work anyhow.
        part.move_to(0.0, 0.0, 0.0);
    }

    world.merge(neon_pump);

    // let loaded_pdb = pdb::load_from_pdb_str(
    //     &gpu_resources,
    //     "Neon Pump",
    //     include_str!("../assets/neon_pump_imm.pdb"),
    // )
    // .unwrap();

    // let loaded_pdb = pdb::load_from_pdb(
    //     &gpu_resources,
    //     "Carbon Nanotube and DNA",
    //     "assets/nanotube_and_dna.pdb",
    // )
    // .expect("failed to load pdb");

    let interations = Interactions::default();

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;
        match event {
            Event::WindowEvent {
                event: WindowEvent::Resized(new_size),
                ..
            } => {
                renderer.resize(new_size);
            }
            Event::MainEventsCleared => {
                renderer.render(&mut world, &interations);
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => *control_flow = ControlFlow::Exit,
            Event::WindowEvent { event, .. } => {
                renderer.camera().update(InputEvent::Window(event));
            }
            Event::DeviceEvent { event, .. } => {
                renderer.camera().update(InputEvent::Device(event));
            }
            _ => {}
        }
    })
}

fn main() {
    // Create the event loop.
    let mut event_loop = EventLoopBuilder::new();
    #[cfg(target_os = "macos")]
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
    setup_menu_bar(&window);

    #[cfg(not(target_arch = "wasm32"))]
    {
        futures::executor::block_on(run(event_loop, window));
    }
    #[cfg(target_arch = "wasm32")]
    {
        std::panic::set_hook(Box::new(console_error_panic_hook::hook));
        console_log::init().expect("could not initialize logger");
        use winit::platform::web::WindowExtWebSys;
        // On wasm, append the canvas to the document body
        web_sys::window()
            .and_then(|win| win.document())
            .and_then(|doc| doc.body())
            .and_then(|body| {
                body.append_child(&web_sys::Element::from(window.canvas()))
                    .ok()
            })
            .expect("couldn't append canvas to document body");
        wasm_bindgen_futures::spawn_local(run(event_loop, window));
    }
}

// End of File
