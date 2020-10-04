use crate::camera::ArcballCamera;
use common::InputEvent;
use render::{Interactions, Renderer, World};

use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::Window,
};

mod camera;
mod pdb;

async fn run(event_loop: EventLoop<()>, window: Window) {
    let (mut renderer, gpu_resources) = Renderer::new(&window).await;

    renderer.set_camera(ArcballCamera::new(100.0, 1.0));

    let mut world = World::new();

    let loaded_pdb = pdb::load_from_pdb(&gpu_resources, "Neon Pump", "data/neon_pump_imm.pdb")
        .expect("failed to load pdb");

    // let loaded_pdb = pdb::load_from_pdb(
    //     &gpu_resources,
    //     "Carbon Nanotube and DNA",
    //     "data/nanotube_and_dna.pdb",
    // )
    // .expect("failed to load pdb");

    // let fragment_id = loaded_pdb.fragments().next().unwrap().id();

    // for part in loaded_pdb.parts_mut() {
    //     part.move_to(ultraviolet::Vec3::new(0.0, 0.0, 10.0));
    // }

    let some_part = loaded_pdb.parts().next().unwrap().id();

    world.merge(loaded_pdb);

    let interations = Interactions::default();
    // interations.selected_fragments.insert(fragment_id);

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
            // test to show that modifying parts is working correctly.
            Event::WindowEvent {
                event:
                    winit::event::WindowEvent::MouseInput {
                        state: winit::event::ElementState::Pressed,
                        button: winit::event::MouseButton::Left,
                        ..
                    },
                ..
            } => {
                world.part_mut(some_part).offset_by(0.0, 0.0, 2.0);
            }
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
