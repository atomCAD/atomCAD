// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use iced_wgpu::{Backend, Primitive, Renderer, Settings, Viewport};
use iced_winit::{mouse, Cache, Clipboard, Event as IcedEvent, Size, UserInterface};
use winit::{
    dpi::{LogicalSize, PhysicalSize},
    event::{Event, ModifiersState, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::Window,
};

use anyhow::{Context, Result};

use std::{convert::TryInto, iter, mem, sync::Arc};

use crate::compositor::Compositor;
use crate::fps::Fps;
use crate::scene::{Event as SceneEvent, SceneHandle};
use crate::ui;

struct Iced {
    cache: Option<Cache>,
    clipboard: Option<Clipboard>,
    draw_output: (Primitive, mouse::Interaction),
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

    instance: wgpu::Instance,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,

    swapchain_desc: wgpu::SwapChainDescriptor,
    swapchain: wgpu::SwapChain,

    fps: Fps,
    state: State,
    iced: Iced,
    iced_events: Vec<IcedEvent>,

    ui: ui::Root,
    scene: SceneHandle,
    compositor: Compositor,
}

impl Hub {
    pub fn new(event_loop: &EventLoop<()>) -> Result<Hub> {
        let window = Window::new(&event_loop)?;

        let instance = wgpu::Instance::new();

        let surface = unsafe { instance.create_surface(&window) };

        let (device, queue) = futures::executor::block_on(get_wgpu_objects(&instance, &surface))?;

        let device = Arc::new(device);
        let queue = Arc::new(queue);

        let size = window.inner_size();

        let swapchain_desc = wgpu::SwapChainDescriptor {
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Fifo,
        };

        let swapchain = device.create_swap_chain(&surface, &swapchain_desc);

        let state = State {
            logical_size: window.inner_size().to_logical(window.scale_factor()),
            modifiers: ModifiersState::default(),
        };

        let (scene, scene_render_view) =
            SceneHandle::create_scene(Arc::clone(&device), Arc::clone(&queue), size);
        let compositor = Compositor::new(&device, scene_render_view, size);

        let iced = Iced::new(&device, &window);
        let ui = ui::Root::new();

        Ok(Hub {
            window,
            surface,

            instance,
            device,
            queue,

            swapchain_desc,
            swapchain,

            fps: Fps::new(),
            state,
            iced,
            iced_events: Vec::new(),

            ui,
            scene,
            compositor,
        })
    }

    pub fn run(mut self, event_loop: EventLoop<()>) -> ! {
        let mut scene_events = vec![];
        let mut maybe_resize = None;

        // Be careful here, only items moved into this closure will be dropped at the end of program execution.
        event_loop.run(move |event, _, control_flow| {
            // This may be able to be `Wait` because the scene rendering
            // is independent from the UI rendering.
            *control_flow = ControlFlow::Poll;

            match event {
                Event::WindowEvent { event, .. } => {
                    self.on_window_event(event, control_flow, &mut scene_events, &mut maybe_resize)
                }
                Event::MainEventsCleared => self.on_events_cleared(&mut scene_events),
                Event::RedrawRequested(_) => {
                    // Tick the FPS counter.
                    // self.ui.fps.set_fps(self.fps.tick());

                    // NOTE:
                    // Send all the current scene events to the scene thread.
                    // This doesn't queue batches of events, it
                    // replaces the next batch.
                    // When the scene thread pulls new events, it'll
                    // get the most recent ones.
                    //
                    // This is an attempt at allowing the scene and UI
                    // to render at different framerates.
                    //
                    // However, when we resize, we have to
                    // fake it by adding empty (black most likely) space
                    // if larger or cropping if smaller. (The other option
                    // is blocking the frame until the scene renders, but
                    // that would be a bad user experience.)
                    if let Some((new_size, new_scene_target)) = self
                        .scene
                        .apply_events(
                            &self.device,
                            mem::replace(&mut scene_events, vec![]),
                            maybe_resize.take(),
                        )
                        .unwrap()
                    {
                        self.compositor
                            .resize(&self.device, new_scene_target, new_size);
                    }

                    let mut command_encoder =
                        self.device
                            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                                label: if cfg!(build = "debug") {
                                    Some("main command encoder")
                                } else {
                                    None
                                },
                            });

                    let mouse_cursor = {
                        let ui_texture = self.compositor.get_ui_texture();

                        // Clear the ui texture.
                        command_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                                attachment: &ui_texture,
                                resolve_target: None,
                                load_op: wgpu::LoadOp::Clear,
                                store_op: wgpu::StoreOp::Store,
                                clear_color: wgpu::Color::TRANSPARENT,
                            }],
                            depth_stencil_attachment: None,
                        });

                        // Then draw the ui.
                        let mouse_cursor = self.iced.renderer.backend_mut().draw::<&str>(
                            &self.device,
                            &mut command_encoder,
                            &ui_texture,
                            &self.iced.viewport,
                            &self.iced.draw_output,
                            &[],
                        );

                        mouse_cursor
                    };

                    let frame = match self.swapchain.get_next_frame() {
                        Ok(frame) => frame,
                        Err(_) => {
                            self.swapchain = self
                                .device
                                .create_swap_chain(&self.surface, &self.swapchain_desc);
                            self.swapchain
                                .get_next_frame()
                                .expect("Failed to acquire next swap chain texture!")
                        }
                    };

                    // TODO(important): Implement buffer swap/belt to present previous render until new render arrives.
                    let scene_command_buffer = self.scene.recv_cmd_buffer().unwrap();

                    self.compositor
                        .blit(&frame.output.view, &mut command_encoder);

                    // Finally, submit everything to the GPU to draw!
                    self.queue.submit(
                        iter::once(scene_command_buffer)
                            .chain(iter::once(command_encoder.finish())),
                    );

                    self.window
                        .set_cursor_icon(iced_winit::conversion::mouse_interaction(mouse_cursor));
                }
                _ => {}
            }
        });
    }

    fn total_resize(&mut self, new_size: PhysicalSize<u32>) {
        self.swapchain_desc.width = new_size.width;
        self.swapchain_desc.height = new_size.height;

        self.iced.viewport = Viewport::with_physical_size(
            Size::new(new_size.width, new_size.height),
            self.window.scale_factor(),
        );
        self.swapchain = self
            .device
            .create_swap_chain(&self.surface, &self.swapchain_desc);
    }

    fn on_window_event(
        &mut self,
        event: WindowEvent,
        control_flow: &mut ControlFlow,
        scene_events: &mut Vec<SceneEvent>,
        should_resize: &mut Option<PhysicalSize<u32>>,
    ) {
        match event {
            WindowEvent::Resized(new_size) => {
                self.state.logical_size = new_size.to_logical(self.window.scale_factor());

                self.total_resize(new_size);

                *should_resize = Some(new_size);
            }
            WindowEvent::ModifiersChanged(new_modifiers) => self.state.modifiers = new_modifiers,
            WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
            ref event => {
                if let Ok(scene_event) = event.try_into() {
                    scene_events.push(scene_event);
                }
            }
        }

        if let Some(event) = iced_winit::conversion::window_event(
            &event,
            self.window.scale_factor(),
            self.state.modifiers,
        ) {
            self.iced_events.push(event);
        }
    }

    fn on_events_cleared(&mut self, scene_events: &mut Vec<SceneEvent>) {
        // if !self.iced_events.is_empty() {
        self.iced.update(
            &mut self.ui,
            scene_events,
            &self.state,
            self.iced_events.drain(..),
        );
        // }

        self.window.request_redraw()
    }
}

impl Iced {
    pub fn new(device: &wgpu::Device, window: &Window) -> Self {
        let size = window.inner_size();
        Self {
            cache: Some(Cache::default()),
            clipboard: Clipboard::new(window),
            draw_output: (Primitive::None, mouse::Interaction::Idle),
            renderer: Renderer::new(Backend::new(device, Settings::default())),
            viewport: Viewport::with_physical_size(
                Size::new(size.width, size.height),
                window.scale_factor(),
            ),
        }
    }

    pub fn update<I>(
        &mut self,
        ui: &mut ui::Root,
        scene_events: &mut Vec<SceneEvent>,
        state: &State,
        events: I,
    ) where
        I: IntoIterator<Item = IcedEvent>,
    {
        let mut user_interface = UserInterface::build(
            ui.view(),
            Size::new(state.logical_size.width, state.logical_size.height),
            self.cache.take().unwrap(),
            &mut self.renderer,
        );

        let messages = user_interface.update(
            events,
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
                ui.update(msg, scene_events);
            }

            UserInterface::build(
                ui.view(),
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

async fn get_wgpu_objects(
    instance: &wgpu::Instance,
    surface: &wgpu::Surface,
) -> Result<(wgpu::Device, wgpu::Queue)> {
    let adapter = instance
        .request_adapter(
            &wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(surface),
            },
            wgpu::UnsafeExtensions::disallow(),
            wgpu::BackendBit::PRIMARY,
        )
        .await
        .context("Unable to request a webgpu adapter")?;

    adapter
        .request_device(
            &wgpu::DeviceDescriptor {
                extensions: wgpu::Extensions::empty(),
                limits: wgpu::Limits::default(),
                shader_validation: true,
            },
            None,
        )
        .await
        .map_err(|_| anyhow::anyhow!("Unable to request webgpu device"))
}
