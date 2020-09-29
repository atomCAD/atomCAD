use crate::camera::ArcballCamera;
use render::{Renderer, Parts};
use common::InputEvent;

use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::Window,
};

mod camera;

async fn run(event_loop: EventLoop<()>, window: Window) {
    let (device, mut renderer) = Renderer::new(&window).await;

    renderer.set_camera(ArcballCamera::new(
        100.0,
        1.0,
    ));

    let parts = Parts::load_from_pdb(
        &device,
        &renderer.bind_group_layouts(),
        "Neon Pump",
        "data/neon_pump_imm.pdb"
    ).unwrap();

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
                renderer.render(&parts);
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
