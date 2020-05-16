use std::{mem, slice, sync::Arc, thread};
use ultraviolet::{projection::perspective_gl, Mat4, Vec3};

use crate::most_recent::{self, Receiver, RecvError, Sender, TryRecvError};

mod isosphere;
use isosphere::IsoSphere;

const DEFAULT_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8UnormSrgb;
/// Normal as in perpendicular, not usual.
const NORMAL_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba32Float;

pub unsafe trait Pod: Sized {
    fn bytes(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self as *const Self as *const u8, mem::size_of::<Self>()) }
    }
}
unsafe impl<T: Pod> Pod for &[T] {
    fn bytes(&self) -> &[u8] {
        unsafe {
            slice::from_raw_parts(self.as_ptr() as *const u8, mem::size_of::<T>() * self.len())
        }
    }
}
unsafe impl Pod for u16 {}

#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
}
unsafe impl Pod for Vertex {}

pub struct Entity {
    vertex_buffer: wgpu::Buffer,
    index_buffer: Option<wgpu::Buffer>,

    bind_group: wgpu::BindGroup,
    render_pipeline: wgpu::RenderPipeline,

    vertex_num: usize,
}

#[derive(Debug)]
pub enum Event {
    Resize {
        new_texture: wgpu::Texture,
        width: u32,
        height: u32,
    },
}

#[derive(Debug)]
struct Output {
    cmd_buf: wgpu::CommandBuffer,
}

pub struct SceneHandle {
    input_tx: Sender<Vec<Event>>,
    output_rx: Receiver<wgpu::CommandBuffer>,
    scene_thread: thread::JoinHandle<()>,
}

struct Scene {
    global_bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
    normals_fbo: wgpu::Texture,
    // sobel_bind_group: wgpu::BindGroup,
    render_texture: wgpu::Texture,

    icosphere: Entity,
}

fn generate_matrix(aspect_ratio: f32) -> Mat4 {
    let opengl_to_wgpu_matrix: Mat4 = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.5, 1.0,
    ]
    .into();

    let mx_projection = perspective_gl(45_f32.to_radians(), aspect_ratio, 1.0, 10.0);
    let mx_view = Mat4::look_at(Vec3::new(1.5, -5.0, 3.0), Vec3::zero(), Vec3::unit_z());

    opengl_to_wgpu_matrix * mx_projection * mx_view
}

impl SceneHandle {
    /// Spawn the scene thread and return a handle to it, as well as the first texture view.
    pub fn create_scene(
        device: Arc<wgpu::Device>,
        (width, height): (u32, u32),
    ) -> (SceneHandle, wgpu::TextureView) {
        let mut scene = Scene::new(&device, (width, height));

        let (input_tx, input_rx) = most_recent::channel();
        let (output_tx, output_rx) = most_recent::channel();

        let texture_view = scene.render_texture.create_default_view();

        let scene_thread = thread::spawn(move || {
            'main_loop: loop {
                let events: Vec<Event> = match input_rx.recv() {
                    Ok(events) => dbg!(events),
                    Err(RecvError) => {
                        // The sending side has disconnected, time to shut down.
                        break;
                    }
                };

                let mut command_encoder =
                    device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        // TODO: Make all wgpu types have labels in dev build mode.
                        label: if cfg!(dev) {
                            Some("scene command encoder")
                        } else {
                            None
                        },
                    });

                scene.process_events(&device, &mut command_encoder, events);

                scene.draw(&mut command_encoder);

                output_tx
                    .send(command_encoder.finish())
                    .expect("unable to send output");
            }
        });

        let scene_handle = SceneHandle {
            input_tx,
            output_rx,
            scene_thread,
        };

        (scene_handle, texture_view)
    }

    /// Send a collection of events to the scene thread.
    ///
    /// The return type is temporary.
    pub fn apply_events(&mut self, events: Vec<Event>) -> Result<(), String> {
        self.input_tx
            .send(events)
            .map_err(|_| "failed to send item to scene thread".to_string())
    }

    pub fn recv_cmd_buffer(&mut self) -> Result<wgpu::CommandBuffer, String> {
        // match self.output_rx.try_recv() {
        //     Ok(value) => Ok(Some(value)),
        //     Err(TryRecvError::Empty) => Ok(None),
        //     Err(TryRecvError::Disconnected) => Err("disconnected".to_string()),
        // }
        self.output_rx
            .recv()
            .map_err(|_| "failed to receive from scene thread".to_string())
    }

    pub fn build_render_texture(
        &self,
        device: &wgpu::Device,
        (width, height): (u32, u32),
    ) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            size: wgpu::Extent3d {
                width,
                height,
                depth: 1,
            },
            array_layer_count: 1,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEFAULT_FORMAT,
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT | wgpu::TextureUsage::SAMPLED,
            label: None,
        })
    }
}

impl Scene {
    fn new(device: &wgpu::Device, (width, height): (u32, u32)) -> Scene {
        let mx_total = generate_matrix(width as f32 / height as f32);

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
                label: None,
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
            label: None,
        });

        let icosphere = create_unit_icosphere_entity(&device, &global_bind_group_layout);

        // Create the texture that normals are stored in.
        // This is used for filters.
        let normals_fbo = device.create_texture(&wgpu::TextureDescriptor {
            size: wgpu::Extent3d {
                width,
                height,
                depth: 1,
            },
            array_layer_count: 1,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: NORMAL_FORMAT,
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
            label: None,
        });

        // The scene renders to this texture.
        // The main (UI) thread has a view of this texture and copies
        // from it at 60fps.
        let render_texture = device.create_texture(&wgpu::TextureDescriptor {
            size: wgpu::Extent3d {
                width,
                height,
                depth: 1,
            },
            array_layer_count: 1,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEFAULT_FORMAT,
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT | wgpu::TextureUsage::SAMPLED,
            label: None,
        });

        Self {
            global_bind_group,
            uniform_buffer,
            normals_fbo,
            render_texture,

            icosphere,
        }
    }

    fn process_events<I>(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        events: I,
    ) where
        I: IntoIterator<Item = Event>,
    {
        for event in events.into_iter() {
            match event {
                Event::Resize {
                    new_texture,
                    width,
                    height,
                } => {
                    self.resize(&device, encoder, new_texture, (width, height));
                }
                // TODO: Add more events: mouse, etc.
            }
        }
    }

    fn resize(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        render_texture: wgpu::Texture,
        (width, height): (u32, u32),
    ) {
        let mx_total = generate_matrix(width as f32 / height as f32);

        // TODO: Replace this with queue.writeBuffer when it gets merged.
        let matrix_src =
            device.create_buffer_with_data(mx_total.as_byte_slice(), wgpu::BufferUsage::COPY_SRC);

        encoder.copy_buffer_to_buffer(
            &matrix_src,
            0,
            &self.uniform_buffer,
            0,
            mem::size_of_val(&mx_total) as u64,
        );

        self.normals_fbo = device.create_texture(&wgpu::TextureDescriptor {
            size: wgpu::Extent3d {
                width,
                height,
                depth: 1,
            },
            array_layer_count: 1,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: NORMAL_FORMAT,
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
            label: None,
        });

        self.render_texture = render_texture;
        // self.render_texture = device.create_texture(&wgpu::TextureDescriptor {
        //     size: wgpu::Extent3d {
        //         width,
        //         height,
        //         depth: 1,
        //     },
        //     array_layer_count: 1,
        //     mip_level_count: 1,
        //     sample_count: 1,
        //     dimension: wgpu::TextureDimension::D2,
        //     format: DEFAULT_FORMAT,
        //     usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
        //     label: None,
        // });
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

    let vertex_buffer =
        device.create_buffer_with_data(icosphere.vertices().bytes(), wgpu::BufferUsage::VERTEX);

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        bindings: &[],
        label: None,
    });

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        layout: &bind_group_layout,
        bindings: &[],
        label: None,
    });

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
        index_buffer: None,

        bind_group,
        render_pipeline,

        vertex_num: icosphere.vertices().len(),
    }
}

// fn create_cube_entity(device: &wgpu::Device) -> Entity {

//     let vertex_buffer = device
//         .create_buffer_with_data(vertex_data.as_slice().bytes(), wgpu::BufferUsage::VERTEX);
//     let index_buffer = device
//         .create_buffer_with_data(index_data.as_slice().bytes(), wgpu::BufferUsage::INDEX);

//     let bind_group_layout =
//         device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
//             bindings: &[wgpu::BindGroupLayoutEntry {
//                 binding: 0,
//                 visibility: wgpu::ShaderStage::VERTEX | wgpu::ShaderStage::FRAGMENT,
//                 ty: wgpu::BindingType::UniformBuffer { dynamic: false },
//             }],
//         });

//     Entity {
//         vertex_buffer,
//         index_buffer,

//     }
// }
