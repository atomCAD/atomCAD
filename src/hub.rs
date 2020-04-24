use iced_wgpu::{Primitive, Renderer, Settings, Target, Viewport};
use iced_winit::{Cache, Clipboard, MouseCursor, Size, UserInterface, Event as IcedEvent};
use winit::{
    event::{Event, WindowEvent, ModifiersState},
    event_loop::{ControlFlow, EventLoop},
    window::Window,
    dpi::LogicalSize,
};

use anyhow::{Result, Context};

use std::time::Instant;

use crate::fps::Fps;
use crate::scene::Scene;
use crate::ui::Ui;
use crate::debug_metrics::DebugMetrics;

/// TODO: Think about whether `Iced` and `Ui` can be combined in some way.
struct Iced {
    events: Vec<IcedEvent>,
    cache: Option<Cache>,
    clipboard: Option<Clipboard>,
    draw_output: (Primitive, MouseCursor),
    renderer: Renderer,
    viewport: Viewport,
}

struct State {
    logical_size: LogicalSize<f32>,
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
    iced: Iced,

    ui: Ui,
    scene: Scene,

    debug_metrics: DebugMetrics,
}

impl Hub {
    pub fn new(event_loop: &EventLoop<()>) -> Result<Hub> {
        let window = Window::new(&event_loop)?;

        let size = window.inner_size();
        let surface = wgpu::Surface::create(&window);

        let (device, queue) = futures::executor::block_on(get_device_and_queue(&surface))?;

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

        let scene = Scene::new(&device, &swapchain_desc);
        
        let iced = Iced::new(&device, &window);
        let ui = Ui::new();

        Ok(Hub {
            window,
            surface,

            device,
            queue,

            swapchain_desc,
            swapchain,

            fps: Fps::new(),
            state,
            iced,

            ui,
            scene,

            debug_metrics: Default::default(),
        })
    }

    pub fn run(mut self, event_loop: EventLoop<()>) -> ! {
        let mut resized = false;

        // Spin up the UI before we get any events.
        self.iced.update(&mut self.ui, &mut self.scene, &self.state);

        event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Poll; // TODO: change this to `Poll`.

            match event {
                Event::WindowEvent { event, .. } => self.on_window_event(event, control_flow, &mut resized),
                Event::MainEventsCleared => self.on_events_cleared(),
                Event::RedrawRequested(_) => {
                    // Tick the FPS counter.
                    self.fps.tick();

                    let mut metrics = DebugMetrics::default();

                    let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: None,
                    });

                    if resized {
                        resized = false;
                        self.rebuild_swapchain(&mut encoder);
                    }

                    let now = Instant::now();
                    let frame = self.swapchain.get_next_texture()
                        .expect("timeout when acquiring next swapchain texture");
                    metrics.frame = Some(now.elapsed());

                    let now = Instant::now();
                    // Draw the scene first.
                    self.scene.draw(&mut encoder, &frame.view);
                    metrics.scene_draw = Some(now.elapsed());

                    let now = Instant::now();
                    // Then draw the ui.
                    let mouse_cursor = self.iced.renderer.draw(
                        &self.device,
                        &mut encoder,
                        Target {
                            texture: &frame.view,
                            viewport: &self.iced.viewport,
                        },
                        &self.iced.draw_output,
                        self.window.scale_factor(),
                        &self.debug_output(),
                    );
                    metrics.ui_draw = Some(now.elapsed());

                    let now = Instant::now();
                    // Finally, submit everything to the GPU to draw!
                    self.queue.submit(&[encoder.finish()]);
                    metrics.queue = Some(now.elapsed());

                    self.window.set_cursor_icon(iced_winit::conversion::mouse_cursor(mouse_cursor));

                    self.debug_metrics = metrics;
                }
                _ => {}
            }
        });
    }

    fn rebuild_swapchain(&mut self, encoder: &mut wgpu::CommandEncoder) {
        let new_size = self.window.inner_size();
        self.swapchain_desc.width = new_size.width;
        self.swapchain_desc.height = new_size.height;

        self.iced.viewport = Viewport::new(new_size.width, new_size.height);
        self.swapchain = self.device.create_swap_chain(&self.surface, &self.swapchain_desc);
        self.scene.resize(&self.device, &self.swapchain_desc, encoder);
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

        if let Some(event) = iced_winit::conversion::window_event(&event, self.window.scale_factor(), self.state.modifiers) {
            self.iced.events.push(event);
        }
    }

    fn on_events_cleared(&mut self) {
        if !self.iced.events.is_empty() {
            self.iced.update(&mut self.ui, &mut self.scene, &self.state);
        }
        
        self.window.request_redraw()
    }

    fn debug_output(&self) -> Vec<std::borrow::Cow<'static, str>> {
        if cfg!(feature = "dev-output") {
            let mut list = vec![
                concat!(env!("CARGO_PKG_NAME"), " ", env!("CARGO_PKG_VERSION"), " ", env!("CARGO_PKG_REPOSITORY")).into(),
                format!("fps: {}", self.fps.get()).into(),
                "metrics:".into(),
            ];
            
            list.extend(self.debug_metrics.output().map(|s| s.into()));

            list
        } else {
            vec![]
        }
    }
}

impl Iced {
    pub fn new(device: &wgpu::Device, window: &Window) -> Self {
        let size = window.inner_size();
        Self {
            events: Vec::new(),
            cache: Some(Cache::default()),
            clipboard: Clipboard::new(window),
            draw_output: (Primitive::None, MouseCursor::OutOfBounds),
            renderer: Renderer::new(device, Settings::default()),
            viewport: Viewport::new(size.width, size.height),
        }
    }

    pub fn update(&mut self, ui: &mut Ui, scene: &mut Scene, state: &State) {
        let mut user_interface = UserInterface::build(
            ui.view(scene),
            Size::new(state.logical_size.width, state.logical_size.height),
            self.cache.take().unwrap(),
            &mut self.renderer,
        );

        let messages = user_interface.update(
            self.events.drain(..),
            self.clipboard.as_ref().map(|c| c as _),
            &self.renderer,
        );

        let user_interface = if messages.is_empty() {
            user_interface
        } else {
            // We need to update our state.
            self.cache = Some(user_interface.into_cache());

            // Send all the messages to the Ui.
            for msg in messages {
                ui.update(msg, scene);
            }

            UserInterface::build(
                ui.view(&scene),
                Size::new(state.logical_size.width, state.logical_size.height),
                self.cache.take().unwrap(),
                &mut self.renderer,
            )
        };

        // Finally, draw new output for our renderer.
        self.draw_output = user_interface.draw(&mut self.renderer);

        // Update our cache.
        self.cache = Some(user_interface.into_cache());
    }
}

async fn get_device_and_queue(surface: &wgpu::Surface) -> Result<(wgpu::Device, wgpu::Queue)> {
    let adapter = wgpu::Adapter::request(
        &wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::Default,
            compatible_surface: Some(surface),
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