// Copyright (c) 2020 by Lachlan Sneff <lachlan@charted.space>
// Copyright (c) 2020 by Mark Friedenbach <mark@friedenbach.org>
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use anyhow::{Context, Result};
use bytemuck;
use std::{mem, sync::Arc, thread};
use ultraviolet::{projection::perspective_gl, Isometry3, Mat4, Vec2, Vec3};
use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    event::{ElementState, MouseButton},
};

use crate::{
    arcball,
    most_recent::{self, Receiver, RecvError, Sender},
};

mod event;
mod isosphere;

pub use event::{Event, Events, Resize};
use isosphere::IsoSphere;

const DEFAULT_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8UnormSrgb;
/// Normal as in perpendicular, not usual.
const NORMAL_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba32Float;

#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
}

unsafe impl bytemuck::Zeroable for Vertex {}
unsafe impl bytemuck::Pod for Vertex {}

pub struct Entity {
    vertex_buffer: wgpu::Buffer,
    render_pipeline: wgpu::RenderPipeline,

    vertex_num: usize,
}

enum Msg {
    Events(Events),
    Exit,
}

#[derive(Debug)]
struct Mouse {
    pub old_cursor: Option<PhysicalPosition<u32>>,
    pub cursor: Option<PhysicalPosition<u32>>,
    pub left_button: ElementState,
}

#[derive(Debug)]
struct State {
    pub mouse: Mouse,
}

impl State {
    pub fn new() -> Self {
        State {
            mouse: Mouse {
                old_cursor: None,
                cursor: None,
                left_button: ElementState::Released,
            },
        }
    }
}

pub struct SceneHandle {
    input_tx: Sender<Msg>,
    output_rx: Receiver<Result<wgpu::CommandBuffer>>,
    scene_thread: Option<thread::JoinHandle<()>>,
}

struct Scene {
    global_bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
    render_texture: wgpu::Texture,
    normals_fbo: wgpu::Texture,

    size: PhysicalSize<u32>,
    world_mx: Mat4,
    camera: Vec3,

    icosphere: Entity,

    state: State,
}

fn generate_matrix(camera: Vec3, aspect_ratio: f32) -> Mat4 {
    let opengl_to_wgpu_matrix: Mat4 = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.5, 1.0,
    ]
    .into();

    let mx_projection = perspective_gl(45_f32.to_radians(), aspect_ratio, 1.0, 10.0);
    let mx_view = Mat4::look_at(camera, Vec3::zero(), Vec3::unit_z());

    opengl_to_wgpu_matrix * mx_projection * mx_view
}

/// TODO: Replace with `queue.write_buffer`.
fn upload_matrix(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    uniform: &wgpu::Buffer,
    mx: Mat4,
) {
    let matrix_src =
        device.create_buffer_with_data(mx.as_byte_slice(), wgpu::BufferUsage::COPY_SRC);

    encoder.copy_buffer_to_buffer(&matrix_src, 0, &uniform, 0, mem::size_of_val(&mx) as u64);
}

impl SceneHandle {
    /// Spawn the scene thread and return a handle to it, as well as the first texture view.
    pub fn create_scene(
        device: Arc<wgpu::Device>,
        size: PhysicalSize<u32>,
    ) -> (SceneHandle, wgpu::TextureView) {
        let mut scene = Scene::new(&device, size);

        let (input_tx, input_rx) = most_recent::channel();
        let (output_tx, output_rx) = most_recent::channel();

        let texture_view = scene.render_texture.create_default_view();

        let scene_thread = thread::spawn(move || {
            loop {
                let events: Events = match input_rx.recv() {
                    Ok(Msg::Events(events)) => events,
                    Ok(Msg::Exit) // the sending side has requested the scene thread to shut down.
                    | Err(RecvError) // The sending side has disconnected, time to shut down.
                        => break,
                };

                let mut command_encoder =
                    device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        // TODO: Make all wgpu types have labels in dev build mode.
                        label: if cfg!(build = "debug") {
                            Some("scene command encoder")
                        } else {
                            None
                        },
                    });

                match scene.run_frame(&device, &mut command_encoder, events) {
                    Ok(_) => output_tx
                        .send(Ok(command_encoder.finish()))
                        .expect("unable to send command buffer to main thread"),
                    Err(e) => output_tx
                        .send(Err(e))
                        .expect("unable to send error to main thread"),
                }
            }

            log::info!("scene thread is shutting down");
        });

        let scene_handle = SceneHandle {
            input_tx,
            output_rx,
            scene_thread: Some(scene_thread),
        };

        (scene_handle, texture_view)
    }

    /// Send a collection of events to the scene thread.
    ///
    /// The return type is temporary.
    pub fn apply_events(&mut self, events: Events) -> Result<()> {
        self.input_tx
            .send(Msg::Events(events))
            .context("failed to send item to scene thread")
    }

    pub fn recv_cmd_buffer(&mut self) -> Result<wgpu::CommandBuffer> {
        // self.output_rx.try_recv()
        //     .context("didn't retrieve the command buffer from the scene thread in time")?
        //     .context("the scene thread reported an error")
        self.output_rx
            .recv()
            .context("unable to retrieve a command buffer from the scene thread")?
            .context("the scene thread reported an error")
    }

    pub fn build_render_texture(
        &self,
        device: &wgpu::Device,
        size: PhysicalSize<u32>,
    ) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            size: wgpu::Extent3d {
                width: size.width,
                height: size.height,
                depth: 1,
            },
            array_layer_count: 1,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEFAULT_FORMAT,
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT | wgpu::TextureUsage::SAMPLED,
            label: if cfg!(build = "debug") {
                Some("scene render texture")
            } else {
                None
            },
        })
    }
}

impl Drop for SceneHandle {
    fn drop(&mut self) {
        self.input_tx.send(Msg::Exit).unwrap();
        self.scene_thread.take().unwrap().join().unwrap();
    }
}

impl Scene {
    fn new(device: &wgpu::Device, size: PhysicalSize<u32>) -> Scene {
        let camera = Vec3::new(1.5, -5.0, 3.0);

        let mx_total = generate_matrix(camera, size.width as f32 / size.height as f32);

        let uniform_buffer = device.create_buffer_with_data(
            mx_total.as_byte_slice(),
            wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_DST,
        );

        let global_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                bindings: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStage::VERTEX,
                    ty: wgpu::BindingType::UniformBuffer { dynamic: false },
                }],
                label: if cfg!(build = "debug") {
                    Some("scene global bind group layout")
                } else {
                    None
                },
            });

        let global_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &global_bind_group_layout,
            bindings: &[wgpu::Binding {
                binding: 0,
                resource: wgpu::BindingResource::Buffer {
                    buffer: &uniform_buffer,
                    range: 0..mem::size_of::<Mat4>() as u64,
                },
            }],
            label: if cfg!(build = "debug") {
                Some("scene global bind group")
            } else {
                None
            },
        });

        let icosphere = create_unit_icosphere_entity(&device, &global_bind_group_layout);

        // Create the texture that normals are stored in.
        // This is used for filters.
        let normals_fbo = device.create_texture(&wgpu::TextureDescriptor {
            size: wgpu::Extent3d {
                width: size.width,
                height: size.height,
                depth: 1,
            },
            array_layer_count: 1,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: NORMAL_FORMAT,
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
            label: if cfg!(build = "debug") {
                Some("scene normal texture")
            } else {
                None
            },
        });

        // The scene renders to this texture.
        // The main (UI) thread has a view of this texture and copies
        // from it at 60fps.
        let render_texture = device.create_texture(&wgpu::TextureDescriptor {
            size: wgpu::Extent3d {
                width: size.width,
                height: size.height,
                depth: 1,
            },
            array_layer_count: 1,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEFAULT_FORMAT,
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT | wgpu::TextureUsage::SAMPLED,
            label: if cfg!(build = "debug") {
                Some("scene render texture")
            } else {
                None
            },
        });

        Self {
            global_bind_group,
            uniform_buffer,
            normals_fbo,
            render_texture,
            size,
            world_mx: mx_total,
            camera,

            icosphere,

            state: State::new(),
        }
    }

    fn run_frame(
        &mut self,
        device: &wgpu::Device,
        command_encoder: &mut wgpu::CommandEncoder,
        mut events: Events,
    ) -> Result<()> {
        if let Some(Resize { new_texture, size }) = events.resize.take() {
            self.resize(&device, command_encoder, new_texture, size);
        }

        self.process_events(&device, command_encoder, events.events.drain(..));

        // Do rotation with arcball.
        if let Mouse {
            old_cursor: Some(old_cursor),
            cursor: Some(new_cursor),
            left_button: ElementState::Pressed,
            ..
        } = self.state.mouse
        {
            let scale = |cursor: PhysicalPosition<u32>| {
                Vec2::new( // scale pixel coordinates to [0, 2]
                    cursor.x as f32 / (self.size.width as f32 / 2.0),
                    cursor.y as f32 / (self.size.height as f32 / 2.0),
                )
                .map(|i| i - 1.0) // scale from [0, 2] to [-1, 1]
                    * Vec2::new(-1.0, 1.0)
            };

            let old_cursor = scale(old_cursor);
            let new_cursor = scale(new_cursor);

            let rotor = arcball::create_rotor(old_cursor, new_cursor);
            let rotation = Isometry3::new(Vec3::zero(), rotor).into_homogeneous_matrix();
            self.world_mx = self.world_mx * rotation;

            upload_matrix(
                &device,
                command_encoder,
                &self.uniform_buffer,
                self.world_mx,
            );
        }

        self.draw(command_encoder);

        Ok(())
    }

    fn process_events(
        &mut self,
        _device: &wgpu::Device,
        _encoder: &mut wgpu::CommandEncoder,
        events: impl Iterator<Item = Event>,
    ) {
        let mut cursor_left = false;

        self.state.mouse.old_cursor = self.state.mouse.cursor.take();

        for event in events {
            match event {
                Event::MouseInput { button, state } => match button {
                    MouseButton::Left => {
                        self.state.mouse.left_button = state;
                    }
                    _ => {}
                },
                Event::CursorMoved { new_pos } => {
                    self.state.mouse.cursor = Some(new_pos);
                }
                Event::CursorLeft => {
                    cursor_left = true;
                }
            }
        }

        if cursor_left {
            self.state.mouse.old_cursor = None;
            self.state.mouse.cursor = None;
        }
    }

    fn resize(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        render_texture: wgpu::Texture,
        size: PhysicalSize<u32>,
    ) {
        self.world_mx = generate_matrix(self.camera, size.width as f32 / size.height as f32);

        upload_matrix(device, encoder, &self.uniform_buffer, self.world_mx);

        self.normals_fbo = device.create_texture(&wgpu::TextureDescriptor {
            size: wgpu::Extent3d {
                width: size.width,
                height: size.height,
                depth: 1,
            },
            array_layer_count: 1,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: NORMAL_FORMAT,
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
            label: if cfg!(build = "debug") {
                Some("scene normal texture")
            } else {
                None
            },
        });

        self.render_texture = render_texture;
        self.size = size;
    }

    fn draw(&mut self, encoder: &mut wgpu::CommandEncoder) {
        let render_view = self.render_texture.create_default_view();
        let normals_view = self.normals_fbo.create_default_view();

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[
                    wgpu::RenderPassColorAttachmentDescriptor {
                        attachment: &render_view,
                        resolve_target: None,
                        load_op: wgpu::LoadOp::Clear,
                        store_op: wgpu::StoreOp::Store,
                        clear_color: wgpu::Color::WHITE,
                    },
                    wgpu::RenderPassColorAttachmentDescriptor {
                        attachment: &normals_view,
                        resolve_target: None,
                        load_op: wgpu::LoadOp::Clear,
                        store_op: wgpu::StoreOp::Store,
                        clear_color: wgpu::Color::TRANSPARENT,
                    },
                ],
                depth_stencil_attachment: None,
            });

            render_pass.set_pipeline(&self.icosphere.render_pipeline);
            render_pass.set_bind_group(0, &self.global_bind_group, &[]);
            // render_pass.set_bind_group(1, &self.icosphere.bind_group, &[]);
            render_pass.set_vertex_buffer(0, &self.icosphere.vertex_buffer, 0, 0);
            // render_pass.set_bind_group(index, bind_group, offsets)
            render_pass.draw(0..self.icosphere.vertex_num as u32, 0..1);
        }

        {
            // let mut compute_pass = encoder.begin_compute_pass();
        }
    }
}

fn create_unit_icosphere_entity(
    device: &wgpu::Device,
    global_bind_group_layout: &wgpu::BindGroupLayout,
) -> Entity {
    let vert_shader = include_shader_binary!("icosphere.vert");
    let frag_shader = include_shader_binary!("icosphere.frag");

    let vert_module = device.create_shader_module(vert_shader);
    let frag_module = device.create_shader_module(frag_shader);

    let icosphere = IsoSphere::new();

    let vertex_buffer = device.create_buffer_with_data(
        bytemuck::cast_slice(icosphere.vertices()),
        wgpu::BufferUsage::VERTEX,
    );

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        bind_group_layouts: &[global_bind_group_layout],
    });

    let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        layout: &pipeline_layout,
        vertex_stage: wgpu::ProgrammableStageDescriptor {
            module: &vert_module,
            entry_point: "main",
        },
        fragment_stage: Some(wgpu::ProgrammableStageDescriptor {
            module: &frag_module,
            entry_point: "main",
        }),
        rasterization_state: Some(wgpu::RasterizationStateDescriptor {
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: wgpu::CullMode::None,
            depth_bias: 0,
            depth_bias_slope_scale: 0.0,
            depth_bias_clamp: 0.0,
        }),
        primitive_topology: wgpu::PrimitiveTopology::TriangleList,
        color_states: &[
            wgpu::ColorStateDescriptor {
                format: DEFAULT_FORMAT,
                color_blend: wgpu::BlendDescriptor::REPLACE,
                alpha_blend: wgpu::BlendDescriptor::REPLACE,
                write_mask: wgpu::ColorWrite::ALL,
            },
            wgpu::ColorStateDescriptor {
                format: NORMAL_FORMAT,
                color_blend: wgpu::BlendDescriptor::REPLACE,
                alpha_blend: wgpu::BlendDescriptor::REPLACE,
                write_mask: wgpu::ColorWrite::ALL,
            },
        ],
        depth_stencil_state: None,
        vertex_state: wgpu::VertexStateDescriptor {
            index_format: wgpu::IndexFormat::Uint16,
            vertex_buffers: &[wgpu::VertexBufferDescriptor {
                stride: mem::size_of::<Vertex>() as u64,
                step_mode: wgpu::InputStepMode::Vertex,
                attributes: &[
                    wgpu::VertexAttributeDescriptor {
                        format: wgpu::VertexFormat::Float3,
                        offset: 0,
                        shader_location: 0,
                    },
                    wgpu::VertexAttributeDescriptor {
                        format: wgpu::VertexFormat::Float3,
                        offset: 4 * 3,
                        shader_location: 1,
                    },
                ],
            }],
        },
        sample_count: 1,
        sample_mask: !0,
        alpha_to_coverage_enabled: false,
    });

    Entity {
        vertex_buffer,
        render_pipeline,

        vertex_num: icosphere.vertices().len(),
    }
}

// End of File
