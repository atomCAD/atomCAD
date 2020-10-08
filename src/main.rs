use crate::camera::ArcballCamera;
// use crate::rotating_camera::RotatingArcballCamera;
use common::InputEvent;
use render::{Interactions, RenderOptions, Renderer, World};

use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::Window,
};

mod camera;
// mod rotating_camera;
mod pdb;
// mod ti;

async fn run(event_loop: EventLoop<()>, window: Window) {
    let (mut renderer, gpu_resources) = Renderer::new(
        &window,
        RenderOptions {
            fxaa: Some(()), // placeholder
            attempt_gpu_driven: false,
        },
    )
    .await;

    renderer.set_camera(ArcballCamera::new(100.0, 1.0));

    let mut world = World::new();

    let mut neon_pump = pdb::load_from_pdb(&gpu_resources, "Neon Pump", "data/neon_pump_imm.pdb")
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
    //     include_str!("../data/neon_pump_imm.pdb"),
    // )
    // .unwrap();

    // let loaded_pdb = pdb::load_from_pdb(
    //     &gpu_resources,
    //     "Carbon Nanotube and DNA",
    //     "data/nanotube_and_dna.pdb",
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
    let event_loop = EventLoop::new();
    let window = Window::new(&event_loop).unwrap();

    #[cfg(not(target_arch = "wasm32"))]
    {
        subscriber::initialize_default_subscriber(None);
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
