
use winit::{
    event::{Event, WindowEvent, ModifiersState},
    event_loop::{ControlFlow, EventLoop},
    window::Window,
    dpi::LogicalSize,
};

use anyhow::{Result, Context};

use crate::fps::Fps;
use crate::scene::Scene;

struct State {
    logical_size: LogicalSize<u32>,
    modifiers: ModifiersState,
}

pub struct Hub {
    window: Window,
    surface: wgpu::Surface,

    device: wgpu::Device,
    queue: wgpu::Queue,

    swapchain_desc: wgpu::SwapChainDescriptor,
    swapchain: wgpu::SwapChain,

    fps: Fps,
    state: State,

    scene: Scene,
}

impl Hub {
    pub fn new(event_loop: &EventLoop<()>) -> Result<Hub> {
        let window = Window::new(&event_loop)?;

        let size = window.inner_size();
        let surface = wgpu::Surface::create(&window);

        let (device, queue) = futures::executor::block_on(get_device_and_queue())?;

        let swapchain_desc = wgpu::SwapChainDescriptor {
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Mailbox,
        };

        let swapchain = device.create_swap_chain(&surface, &swapchain_desc);

        let state = State {
            logical_size: window.inner_size().to_logical(window.scale_factor()),
            modifiers: ModifiersState::default(),
        };

        let scene = Scene::new(&device);

        Ok(Hub {
            window,
            surface,

            device,
            queue,

            swapchain_desc,
            swapchain,

            fps: Fps::new(),
            state,

            scene,
        })
    }

    pub fn run(mut self, event_loop: EventLoop<()>) -> ! {
        let mut resized = false;

        event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Wait; // TODO: change this to `Poll`.

            match event {
                Event::WindowEvent { event, .. } => self.on_window_event(event, control_flow, &mut resized),
                Event::MainEventsCleared => self.window.request_redraw(),
                Event::RedrawRequested(_) => {
                    if resized {
                        resized = false;
                        self.rebuild_swapchain();
                    }

                    let current_fps = self.fps.tick();

                    info!("current fps: {}", current_fps);

                    // This blocks until ~16ms have passed since the last time it returned.
                    let frame = self.swapchain.get_next_texture()
                        .expect("timeout when acquiring next swapchain texture");

                    let mut encoder = self.device.create_command_encoder(&Default::default());

                    // Draw the scene.
                    self.scene.draw(&mut encoder, &frame.view);
                    // TODO: draw the UI.

                    // Finally, submit everything to the GPU to draw!
                    self.queue.submit(&[encoder.finish()]);
                }
                _ => {}
            }
        });
    }

    fn rebuild_swapchain(&mut self) {
        let new_size = self.window.inner_size();

        self.swapchain_desc = wgpu::SwapChainDescriptor {
            width: new_size.width,
            height: new_size.height,
            .. self.swapchain_desc
        };

        self.swapchain = self.device.create_swap_chain(&self.surface, &self.swapchain_desc)
    }

    fn on_window_event(&mut self, event: WindowEvent, control_flow: &mut ControlFlow, resized: &mut bool) {
        match event {
            WindowEvent::Resized(new_size) => {
                self.state.logical_size = new_size.to_logical(self.window.scale_factor());
                *resized = true
            }
            WindowEvent::ModifiersChanged(new_modifiers) => self.state.modifiers = new_modifiers,
            WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
            _ => {},
        }
    }
}

async fn get_device_and_queue() -> Result<(wgpu::Device, wgpu::Queue)> {
    let adapter = wgpu::Adapter::request(
        &wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::Default,
        },
        wgpu::BackendBit::PRIMARY,
    ).await.context("Unable to request a webgpu adapter")?;

    Ok(adapter.request_device(&wgpu::DeviceDescriptor {
        extensions: wgpu::Extensions {
            anisotropic_filtering: false,
        },
        limits: wgpu::Limits::default(),
    }).await)
}