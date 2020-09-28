#[macro_use]
extern crate static_assertions;

mod atoms;
mod bind_groups;
mod camera;
mod elements;
mod parts;
mod utils;
mod render;

// use crate::elements::Element;
use crate::{
    bind_groups::{AsBindingResource as _, BindGroupLayouts},
    elements::Element,
    utils::AsBytes as _,
    render::Renderer,
};

use std::{convert::TryInto as _, iter, mem};
use wgpu::util::DeviceExt as _;
use winit::{
    event::{DeviceEvent, Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::Window,
};

pub enum InputEvent<'a> {
    Window(WindowEvent<'a>),
    Device(DeviceEvent),
}

async fn run(event_loop: EventLoop<()>, window: Window, swapchain_format: wgpu::TextureFormat) {
    let (device, mut renderer) = Renderer::new(&window, swapchain_format).await;

    let parts =
        parts::Part::load_from_pdb(&device, &renderer.bind_group_layouts(), "Neon Pump", "data/neon_pump_imm.pdb")
        .map(|parts| parts.into_iter().collect()).unwrap();
    
    

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
                renderer.prepare_for_frame();
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
        futures::executor::block_on(run(event_loop, window, wgpu::TextureFormat::Bgra8UnormSrgb));
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
        wasm_bindgen_futures::spawn_local(run(event_loop, window, wgpu::TextureFormat::Bgra8Unorm));
    }
}
